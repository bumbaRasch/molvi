//! Engine adapter: a thin trait abstracting GigaAM (CTC streaming partials) and
//! Nemotron (cache-aware RNN-T streaming partials) behind one incremental
//! feed/finish/had_speech interface. The worker (`engine.rs`) drains the SPSC
//! ring and calls these methods on `Box<dyn SpeechEngine>`; `load_engine` picks
//! the concrete impl from `settings.model`.
//!
//! Privacy §10.1: Nemotron's partial AND final text ARE transcript — the trait
//! never logs either. Partials flow only through the `on_partial` callback
//! (Tauri event). `NemotronEngine::finish` returns `(text, detected_lang)`; the
//! text never crosses `tracing::*` (the detected lang code is a locale string =
//! metadata, safe). Errors map to metadata-only `MolviError::Inference`.

use std::path::Path;

use parakeet_rs::{Nemotron, NemotronMode};

use crate::engine::Engine;
use crate::errors::{MolviError, Result};
use crate::settings::Settings;

/// Incremental speech-engine interface. Method names mirror GigaAM's prior
/// inherent methods so the worker calls (`feed_chunk`/`finish`/`had_speech`)
/// compile unchanged over `Box<dyn SpeechEngine>` and GigaAM's bodies move into
/// the trait impl verbatim. `on_partial` is live for BOTH engines now (Phase-3:
/// Nemotron switched from offline whole-buffer to cache-aware streaming via
/// `transcribe_chunk_with_tokens`; each feed emits the cumulative transcript
/// from `get_transcript()` as the growing caption).
pub trait SpeechEngine: Send {
    fn feed_chunk(
        &mut self,
        samples: &[f32],
        on_partial: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<()>;
    /// Returns `(text, detected_lang)`. `detected_lang` is `None` when the
    /// engine has no per-utterance detection (GigaAM reports its fixed lang;
    /// Nemotron in a fixed-lang mode emits no tag).
    fn finish(
        &mut self,
        on_partial: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<(String, Option<String>)>;
    fn had_speech(&self) -> bool;
}

/// Factory dispatch decision — pure (no model load), so it is unit-testable
/// without a 2.6GB model on disk. `true` for any model id mentioning
/// "nemotron"; `false` for GigaAM (the default).
pub fn is_nemotron(model_id: &str) -> bool {
    model_id.contains("nemotron")
}

/// Load the engine selected by `settings.model`. Nemotron ids route to
/// `NemotronEngine`; everything else (the GigaAM default) routes to `Engine`.
pub fn load_engine(model_dir: &Path, settings: &Settings) -> Result<Box<dyn SpeechEngine>> {
    if is_nemotron(&settings.model) {
        Ok(Box::new(NemotronEngine::new(model_dir, settings)?))
    } else {
        Ok(Box::new(Engine::load(model_dir, settings)?))
    }
}

// ── Nemotron ──

/// parakeet-rs Nemotron encoder chunk = 560 ms @ 16 kHz (nemotron.rs
/// `chunk_size * HOP_LENGTH`) — the granularity the cache-aware encoder runs
/// at. Feeding at THIS boundary is load-bearing for blaze: `process_chunk`
/// (nemotron.rs:779-881) recomputes the mel spectrogram over its bounded audio
/// buffer on EVERY call and processes exactly one chunk, so steady-state cost
/// ≈ call frequency. The worker hands us 30 ms VAD frames; calling the model
/// per frame made ~33 calls/s (≈RTF 1.0, live-caption froze mid-utterance —
/// regression caught in live smoke). Accumulating to 8960 drops it to ~1.8
/// calls/s (≈RTF 0.05). Verified ctx7 (`/altunenes/parakeet-rs`: "optimized for
/// 560ms chunks") + the 0.3.7 source.
const NEMOTRON_CHUNK: usize = 8960;

/// Nemotron adapter: cache-aware streaming via `transcribe_chunk_with_tokens`.
/// molvi accumulates capture samples to `NEMOTRON_CHUNK` and invokes the model
/// ONLY at that boundary (see the constant's why-comment), then emits
/// `get_transcript()` (the cumulative caption) to `on_partial`. `finish`
/// zero-pads the < 1 chunk remainder so trailing tokens are emitted, returns
/// the final transcript + detected locale, and `reset()`s for the next
/// utterance (reset preserves `target_lang`).
///
/// License: OpenMDW-1.1 (upstream nvidia cardData `license_name`; no LICENSE
/// file in the pantinor repo — canonical text at openmdw.ai).
pub struct NemotronEngine {
    model: Nemotron,
    /// Harvested from the first lang-tag token (`<en-US>`/`<ru>`); cleared at
    /// `finish()`. `None` in fixed-lang mode (no tag emitted) — caller falls
    /// back to `settings.language`.
    detected_lang: Option<String>,
    fed_any: bool,
    /// Capture samples accumulated to `NEMOTRON_CHUNK`; the model is invoked
    /// only at the boundary (see `NEMOTRON_CHUNK` why-comment).
    frame_buf: Vec<f32>,
}

impl NemotronEngine {
    /// `settings.language` drives Nemotron's multilingual target-lang ("auto"
    /// = model auto-detect + emits a `<xx-XX>` tag token; a locale pins it).
    /// No-op on the English-only variant (gated on mode()).
    pub fn new(model_dir: &Path, settings: &Settings) -> Result<Self> {
        // parakeet default ExecutionConfig (None) = FASTEST measured (Task-17
        // RUN: Level3 was RTF-neutral but ~doubled cold-load — net-negative).
        let mut model = Nemotron::from_pretrained(model_dir, None)
            .map_err(|e| MolviError::Inference(format!("nemotron load: {e}")))?;
        if model.mode() == NemotronMode::Multilingual {
            model
                .set_target_lang(&settings.language)
                .map_err(|e| MolviError::Inference(format!("nemotron set_target_lang: {e}")))?;
        }
        Ok(Self {
            model,
            detected_lang: None,
            fed_any: false,
            frame_buf: Vec::with_capacity(NEMOTRON_CHUNK),
        })
    }
}

impl SpeechEngine for NemotronEngine {
    fn feed_chunk(
        &mut self,
        samples: &[f32],
        on_partial: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<()> {
        self.fed_any = true;
        self.frame_buf.extend_from_slice(samples);
        // Invoke the model ONLY at the 8960-sample boundary (see NEMOTRON_CHUNK
        // why-comment). sub-boundary feeds just accumulate — no wasted mel
        // recompute. Each 8960 feed keeps the model in lockstep (one feed → one
        // encoder chunk processed), so the only unprocessed audio at finish is
        // the < 1 chunk remainder left here.
        while self.frame_buf.len() >= NEMOTRON_CHUNK {
            let chunk: Vec<f32> = self.frame_buf.drain(..NEMOTRON_CHUNK).collect();
            let tokens = self
                .model
                .transcribe_chunk_with_tokens(&chunk)
                .map_err(|e| MolviError::Inference(format!("nemotron chunk: {e}")))?;
            // Harvest the detected locale from the first lang-tag token (emitted
            // once, in the first speech-bearing chunk). Fixed-lang mode emits no
            // tag → stays None → caller falls back to settings.language.
            if self.detected_lang.is_none() {
                for t in &tokens {
                    if let Some(loc) = lang_tag_locale(&t.text) {
                        self.detected_lang = Some(loc.to_string());
                        break;
                    }
                }
            }
            // Privacy §10.1: the partial is transcript — it flows ONLY through
            // the on_partial callback (Tauri event). Never tracing::*-interpolate
            // it. get_transcript() is cumulative (the growing caption) and
            // already strips lang-tag tokens (parakeet-rs nemotron.rs:581).
            on_partial(self.model.get_transcript().as_str());
        }
        Ok(())
    }

    fn finish(
        &mut self,
        _on_partial: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<(String, Option<String>)> {
        // Flush the < 1 chunk remainder: zero-pad to NEMOTRON_CHUNK so the
        // model processes the final partial chunk and emits trailing tokens.
        // feed_chunk keeps the model in lockstep (one 8960 feed → one chunk), so
        // this remainder is the only unprocessed audio. RNN-T blanks on the
        // silence pad are filtered out by get_transcript().
        if !self.frame_buf.is_empty() {
            let mut rem = std::mem::take(&mut self.frame_buf);
            rem.resize(NEMOTRON_CHUNK, 0.0);
            let _ = self
                .model
                .transcribe_chunk_with_tokens(&rem)
                .map_err(|e| MolviError::Inference(format!("nemotron flush: {e}")))?;
        }
        // trim_start() stays byte-for-byte consistent with parakeet-rs's
        // vocab.decode (SentencePiece leading-▁ → space; see parakeet-rs vocab.rs).
        let text = self.model.get_transcript().trim_start().to_string();
        let lang = self.detected_lang.take();
        // ponytail: reset() is explicit session isolation — the model persists
        // across sessions (one worker) and reset() guarantees a clean start.
        // reset() preserves target_lang (prompt_index not reset), so no re-set
        // needed after it.
        self.model.reset();
        Ok((text, lang))
    }

    fn had_speech(&self) -> bool {
        // ponytail: the GigaAM finalize silence-guard (SILENCE_RMS_FLOOR) is a
        // CTC-hallucination guard; RNN-T Nemotron handles silence via blank
        // tokens, so any captured audio counts as speech here.
        self.fed_any
    }
}

/// Replicates parakeet-rs 0.3.7 `is_lang_tag` (src/nemotron.rs). Returns the
/// locale inside `<>` when `piece` is a lang tag (`<xx>` or `<xx-XX>`), else
/// None. Used by `NemotronEngine::feed_chunk` to harvest the detected locale
/// from the first lang-tag `TokenInfo.text` the multilingual model emits.
fn lang_tag_locale(piece: &str) -> Option<&str> {
    let bytes = piece.as_bytes();
    if bytes.len() < 4 || bytes[0] != b'<' || bytes[bytes.len() - 1] != b'>' {
        return None;
    }
    let inner = &bytes[1..bytes.len() - 1];
    let is_tag = match inner.len() {
        2 => inner[0].is_ascii_lowercase() && inner[1].is_ascii_lowercase(),
        5 => {
            inner[0].is_ascii_lowercase()
                && inner[1].is_ascii_lowercase()
                && inner[2] == b'-'
                && inner[3].is_ascii_uppercase()
                && inner[4].is_ascii_uppercase()
        }
        _ => false,
    };
    if is_tag {
        // Inner is pure ASCII (verified above), so byte range == char range.
        Some(&piece[1..piece.len() - 1])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure factory dispatch — no model load, no I/O. Covers the GigaAM
    /// default, the Nemotron id, and the empty-string edge.
    #[test]
    fn load_engine_dispatches_on_model_id() {
        assert!(is_nemotron("nemotron-3.5-asr-streaming-0.6b"));
        assert!(is_nemotron("nemotron-3.5-asr-streaming-0.6b-onnx"));
        assert!(!is_nemotron("gigaam-v3-e2e-ctc"));
        assert!(!is_nemotron(""));
    }

    // `lang_tag_locale` is model-free: it classifies a decoded piece text.
    // The leading space on word pieces is the SentencePiece `▁` marker —
    // irrelevant here since lang_tag_locale only inspects the tag token itself.

    /// `<xx-XX>` form → extracted locale; strict classifier rejects non-tag pieces.
    #[test]
    fn lang_tag_locale_extracts_two_part_locale() {
        assert_eq!(lang_tag_locale("<en-US>"), Some("en-US"));
        assert_eq!(lang_tag_locale("<ru-RU>"), Some("ru-RU"));
    }

    /// parakeet's `is_lang_tag` matches the 2-letter `<xx>` form too. Guards a
    /// `<ru>` tag from being missed (→ detected_lang stays None wrongly).
    #[test]
    fn lang_tag_locale_two_letter_form() {
        assert_eq!(lang_tag_locale("<ru>"), Some("ru"));
        assert_eq!(lang_tag_locale("<en>"), Some("en"));
    }

    /// No tag → None; a non-tag piece containing `<` must NOT be misclassified
    /// (the strict classifier rejects `<3`, `<html>`, empty, bare `<>`).
    #[test]
    fn lang_tag_locale_rejects_non_tags() {
        assert!(lang_tag_locale("<3").is_none());
        assert!(lang_tag_locale("<html>").is_none());
        assert!(lang_tag_locale("<>").is_none());
        assert!(lang_tag_locale("hello").is_none());
        assert!(lang_tag_locale("").is_none());
        // Wrong case / shape — must not slip through.
        assert!(lang_tag_locale("<EN-us>").is_none());
        assert!(lang_tag_locale("<en-US>").is_some()); // sanity: canonical still matches
    }

    // Model-gated: requires the ~2.6GB Nemotron model at $MOLVI_NEMOTRON_MODEL_DIR
    // (or ../models/nemotron-3.5-asr-streaming-0.6b). Run with:
    //   MOLVI_NEMOTRON_MODEL_DIR=<dir> cargo test --features engine-model-test nemotron
    #[cfg(feature = "engine-model-test")]
    #[test]
    fn nemotron_loads_and_feeds_without_error() {
        let model_dir = std::env::var_os("MOLVI_NEMOTRON_MODEL_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("models")
                    .join("nemotron-3.5-asr-streaming-0.6b")
            });
        if !model_dir.exists() {
            eprintln!(
                "skipping: nemotron model not present at {}",
                model_dir.display()
            );
            return;
        }
        let settings = Settings::default();
        let mut engine = NemotronEngine::new(&model_dir, &settings).expect("nemotron load");
        let noop: &(dyn Fn(&str) + Send + Sync) = &|_: &str| {};
        // ~1s of zero samples (silence): feed must not err; had_speech is true
        // once fed. finish must not err (privacy: result text is never asserted
        // or logged — only metadata).
        engine
            .feed_chunk(&vec![0.0f32; 16_000], noop)
            .expect("feed_chunk");
        assert!(engine.had_speech());
        let (_, _) = engine.finish(noop).expect("finish");
    }

    // Model-gated streaming contract: feed_chunk MUST invoke on_partial on
    // every feed (the overlay's growing caption depends on it). Feeds 3 full
    // mel-chunks of silence and asserts the callback fired on ≥2 of them.
    #[cfg(feature = "engine-model-test")]
    #[test]
    fn nemotron_streaming_emits_partials_on_each_feed() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let model_dir = std::env::var_os("MOLVI_NEMOTRON_MODEL_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("models")
                    .join("nemotron-3.5-asr-streaming-0.6b")
            });
        if !model_dir.exists() {
            eprintln!(
                "skipping: nemotron model not present at {}",
                model_dir.display()
            );
            return;
        }
        let settings = Settings::default();
        let mut engine = NemotronEngine::new(&model_dir, &settings).expect("nemotron load");
        let count = Arc::new(AtomicUsize::new(0));
        let count_cb = count.clone();
        let on_partial = move |_: &str| {
            count_cb.fetch_add(1, Ordering::Relaxed);
        };
        // ponytail: 8960 = Nemotron default chunk_samples (560ms @ 16kHz,
        // chunk_size * HOP_LENGTH — parakeet-rs nemotron.rs:524). Feeding at the
        // chunk boundary guarantees each feed crosses a mel-chunk and the
        // encoder runs (so on_partial fires). Silence → blank tokens → empty
        // get_transcript(), but the callback still fires unconditionally.
        let chunk = 8960usize;
        for _ in 0..3 {
            engine
                .feed_chunk(&vec![0.0f32; chunk], &on_partial)
                .expect("feed_chunk");
        }
        let calls = count.load(Ordering::Relaxed);
        assert!(
            calls >= 2,
            "on_partial should fire on ≥2 of 3 feeds (got {calls}); \
             the overlay caption depends on per-feed partials"
        );
        let _ = engine.finish(&on_partial).expect("finish");
    }
}
