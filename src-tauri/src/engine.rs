use std::path::Path;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use transcribe_rs::onnx::Quantization;
use transcribe_rs::onnx::gigaam::GigaAMModel;
use transcribe_rs::transcriber::{Transcriber, VadChunked, VadChunkedConfig};
use transcribe_rs::vad::{EnergyVad, SmoothedVad};
use transcribe_rs::{SpeechModel, TranscribeOptions, TranscriptionResult};

use crate::coordinator;
use crate::engine_adapter::SpeechEngine;
use crate::errors::{MolviError, Result};
use crate::resample::Resampler;
use crate::settings::Settings;

// VAD frame size (30ms @16k). Matches EnergyVad::new(480, _) and the worker's
// per-block feed granularity.
const VAD_FRAME: usize = 480;

// Silence floor for the finalize guard (session-wide RMS). If a session's
// captured audio RMS is below this AND no chunk was finalized, the audio was
// effectively silence (mic muted/quiet) -> chunker.finish() flushes near-silence
// -> the CTC model hallucinates garbage. Discard in that case. Tunable: a quiet
// mic or noisy environment may need this adjusted (ponytail: calibration knob).
const SILENCE_RMS_FLOOR: f64 = 0.005;

// Callback type shared across the worker boundary as `Arc<dyn Fn(&str)+Send+
// Sync>` (EngineCmd::Start). feed_chunk/finish take the borrowed trait object
// directly: `dyn Fn` is callable but does not implement the `Fn` trait, so it
// cannot fill a generic `F: Fn` bound — taking `&(dyn Fn+Send+Sync)` matches the
// Arc's deref type and makes call sites a plain `.as_ref()`.
type PartialCb<'a> = &'a (dyn Fn(&str) + Send + Sync);

/// Trailing-silence auto-stop config, built by the pipeline (toggle mode +
/// endpoint enabled) and shipped to the worker via `EngineCmd::Start`.
/// `energy_threshold` reuses `settings.vad.energy_threshold` (one less knob).
pub struct AutoStopConfig {
    pub trailing_silence: Duration,
    pub energy_threshold: f64,
}

/// Pure trailing-silence detector — the one testable seam. Fed per-frame RMS
/// and a wall-clock `now`; fires exactly once after `trailing_silence` has
/// elapsed since the last loud frame. "Loud" = `rms >= energy_threshold`.
///
/// Does NOT fire until at least one loud frame has been seen: a purely-silent
/// toggle session never auto-stops (matches the "trailing silence" contract —
/// the user must have spoken first). Owned by the engine worker; constructed
/// only when `auto_stop = Some` (PTT/default never builds one -> zero cost).
pub(crate) struct SilenceTracker {
    cfg: AutoStopConfig,
    last_speech_at: Option<Instant>,
    fired: bool,
}

impl SilenceTracker {
    pub(crate) fn new(cfg: AutoStopConfig) -> Self {
        Self {
            cfg,
            last_speech_at: None,
            fired: false,
        }
    }
    pub(crate) fn observe_rms(&mut self, rms: f64, now: Instant) {
        if !self.fired && rms >= self.cfg.energy_threshold {
            self.last_speech_at = Some(now);
        }
    }
    pub(crate) fn should_fire(&self, now: Instant) -> bool {
        !self.fired
            && self
                .last_speech_at
                .is_some_and(|t| now.duration_since(t) >= self.cfg.trailing_silence)
    }
    pub(crate) fn mark_fired(&mut self) {
        self.fired = true;
    }
}

/// Loaded GigaAM-v3 model + VAD chunker. Owns the ort `Session` (`Send`, but the
/// worker thread is the sole owner — single-threaded inference). `chunks` holds
/// finalized-chunk texts for the current session and is the source of the
/// growing partial transcript emitted via `on_partial`.
pub struct Engine {
    model: GigaAMModel,
    chunker: VadChunked,
    chunks: Vec<String>,
}

impl Engine {
    /// Load the model (mirrors molvi-task0's `GigaAMModel::load`) and build a
    /// `SmoothedVad`-backed `VadChunked` chunker from `settings.vad`.
    pub fn load(model_dir: &Path, settings: &Settings) -> Result<Self> {
        let model = GigaAMModel::load(model_dir, &Quantization::Int8)
            .map_err(|e| MolviError::Inference(format!("load model: {e}")))?;

        // VAD chain: EnergyVad -> SmoothedVad (onset/hangover/prefill) -> chunker.
        let inner = EnergyVad::new(VAD_FRAME, settings.vad.energy_threshold);
        let vad = SmoothedVad::new(Box::new(inner), 15, 15, 2);
        let chunker = VadChunked::new(
            Box::new(vad),
            VadChunkedConfig {
                min_chunk_secs: settings.vad.min_chunk_secs,
                max_chunk_secs: settings.vad.max_chunk_secs,
                padding_secs: settings.vad.padding_secs,
                smart_split_search_secs: Some(3.0),
                merge_separator: " ".into(),
            },
            TranscribeOptions {
                language: Some("ru".into()),
                ..Default::default()
            },
        );
        Ok(Self {
            model,
            chunker,
            chunks: Vec::new(),
        })
    }

    /// Offline transcribe of a 16k/16-bit/mono WAV (Task-0 logic; used by the
    /// golden WER test). Mirrors molvi-task0's read + transcribe_raw.
    pub fn transcribe_offline(&mut self, wav: &Path) -> Result<TranscriptionResult> {
        let samples = transcribe_rs::audio::read_wav_samples(wav)
            .map_err(|e| MolviError::Inference(format!("read wav: {e}")))?;
        self.model
            .transcribe_raw(
                &samples,
                &TranscribeOptions {
                    language: Some("ru".into()),
                    ..Default::default()
                },
            )
            .map_err(|e| MolviError::Inference(format!("transcribe: {e}")))
    }
}

impl SpeechEngine for Engine {
    /// Feed a block of 16 kHz mono f32 samples. Each time the chunker finalizes
    /// a speech region, its (trimmed, non-empty) text is appended to the session
    /// transcript and `on_partial` is called with the full growing transcript.
    ///
    /// Phase-1: the model is offline, so a "partial" is the transcript of all
    /// chunks finalized so far (space-joined, matching the chunker's merge).
    fn feed_chunk(&mut self, samples: &[f32], on_partial: PartialCb<'_>) -> Result<()> {
        let results = self
            .chunker
            .feed(&mut self.model, samples)
            .map_err(|e| MolviError::Inference(format!("chunker feed: {e}")))?;
        let mut changed = false;
        for r in results {
            let t = r.text.trim();
            if !t.is_empty() {
                self.chunks.push(t.to_string());
                changed = true;
            }
        }
        if changed {
            on_partial(&self.chunks.join(" "));
        }
        Ok(())
    }

    /// Finalize the session: flush any remaining buffered audio and return the
    /// authoritative full-session transcript + the detected language. GigaAM is
    /// a fixed monolingual (Russian) CTC head — there is no per-utterance
    /// detection, so the detected lang is always `Some("ru")` (matches the
    /// hardcoded `TranscribeOptions::language`). `chunker.finish()` returns the
    /// merged text of ALL session chunks (feed results plus the final flush),
    /// so the partial transcript is REPLACED with that merged text — appending
    /// would double-count. `on_partial` receives it so the overlay shows the
    /// finalized text.
    ///
    /// DEPARTURE from brief: finish takes `on_partial` (the brief had no
    /// callback and appended finish's result to an already-accumulated
    /// transcript, double-counting since finish merges the whole session).
    /// Verified against transcribe-rs 0.3.11 source: vad_chunked.rs finish()
    /// returns merge_sequential_with_separator over self.results.
    fn finish(&mut self, on_partial: PartialCb<'_>) -> Result<(String, Option<String>)> {
        let final_result = self
            .chunker
            .finish(&mut self.model)
            .map_err(|e| MolviError::Inference(format!("chunker finish: {e}")))?;
        self.chunks.clear();
        let text = final_result.text;
        if !text.is_empty() {
            on_partial(&text);
        }
        Ok((text, Some("ru".into())))
    }

    /// Whether any speech region was finalized during the session so far
    /// (`feed_chunk` appended a non-empty chunk). Used by the worker's
    /// finalize silence-guard (alongside session RMS) to decide whether the
    /// `finish()` text is real speech or a CTC hallucination on near-silence.
    fn had_speech(&self) -> bool {
        !self.chunks.is_empty()
    }
}

// ── Worker thread: continuously drains SPSC ring → resample → feed_chunk ──
//
// Consumer-lifecycle resolution (Task 13): the rtrb Consumer is take-once
// (Task 7 AudioCapture::consumer), so it is handed to the worker ONCE at
// spawn and drained continuously for the whole app lifetime. Start/Finalize
// are per-session TOGGLES over that continuous drain:
//   Start{on_partial}    = a session is active; begin feeding the chunker +
//                          emit growing partials via on_partial.
//   Finalize{reply}      = flush + return the transcript, then stop feeding
//                          (drained samples discard until the next Start).
//                          Also used for Cancel (caller passes a discard
//                          channel; the worker still flushes + resets, reply
//                          send fails harmlessly on the dropped rx).
//   Shutdown             = exit the worker.

/// Commands to the engine worker. `consumer` is NOT here — it is provided
/// once at `EngineHandle::spawn` (rtrb Consumer is take-once, not Clone).
pub enum EngineCmd {
    Start {
        on_partial: Arc<dyn Fn(&str) + Send + Sync>,
        auto_stop: Option<AutoStopConfig>,
    },
    Finalize {
        reply: mpsc::Sender<(String, Option<String>)>,
    },
    Shutdown,
}

pub struct EngineHandle {
    pub tx: mpsc::Sender<EngineCmd>,
}

impl EngineHandle {
    /// Spawn the "molvi-engine" worker that owns the ort `Session` + chunker +
    /// resampler + the SPSC `consumer` (drained continuously across sessions).
    /// `model_dir` is loaded on the calling thread (Sessions are `Send`,
    /// single-thread-owned); the worker thread then owns everything.
    pub fn spawn(
        model_dir: &Path,
        settings: &Settings,
        native_rate: u32,
        consumer: rtrb::Consumer<f32>,
        cmd_tx: mpsc::Sender<coordinator::Command>,
    ) -> Result<Self> {
        let engine = crate::engine_adapter::load_engine(model_dir, settings)?;
        let resampler = Resampler::new(native_rate, 16_000, 1)?;
        let (tx, rx) = mpsc::channel::<EngineCmd>();
        std::thread::Builder::new()
            .name("molvi-engine".into())
            .spawn(move || worker_loop(engine, resampler, consumer, rx, cmd_tx))
            .map_err(|e| MolviError::Engine(format!("spawn worker: {e}")))?;
        Ok(EngineHandle { tx })
    }
}

/// Per-session feed state. Created on Start, consumed on Finalize.
struct Session {
    on_partial: Arc<dyn Fn(&str) + Send + Sync>,
    frame_buf: Vec<f32>,
    feed_wall: std::time::Duration,
    feed_samples: usize,
    /// Sum-of-squares of resampled samples fed this session (for the
    /// finalize silence-guard RMS — discriminates real speech from a
    /// mic-muted/near-silent session whose finish() would hallucinate).
    feed_sq: f64,
    /// Trailing-silence auto-stop. None = off (PTT, or endpoint disabled) ->
    /// no tracker, no per-frame RMS, zero cost on the default path.
    auto_stop: Option<SilenceTracker>,
}

/// Continuous drain loop: owns `consumer` for the app lifetime. A `session`
/// of `Some` means feed drained samples to the chunker + emit partials; `None`
/// means drain + discard (idle between sessions, or after Cancel/Finalize).
/// Model calls (`feed_chunk`/`finish`) run under `catch_unwind` so an ort
/// panic resets just the session (logged), not the worker thread.
fn worker_loop(
    mut engine: Box<dyn SpeechEngine>,
    mut resampler: Resampler,
    mut consumer: rtrb::Consumer<f32>,
    rx: mpsc::Receiver<EngineCmd>,
    cmd_tx: mpsc::Sender<coordinator::Command>,
) {
    let mut session: Option<Session> = None;
    let mut batch: Vec<f32> = Vec::new();
    loop {
        // 1. Command dispatch. recv_timeout(2ms) replaces the old try_recv +
        //    sleep(2ms) pattern: a Finalize command wakes INSTANTLY (≤2ms
        //    paste latency) instead of waiting for a sleep to elapse. On
        //    Timeout the consumer drain below proceeds as before.
        match rx.recv_timeout(std::time::Duration::from_millis(2)) {
            Ok(EngineCmd::Start {
                on_partial,
                auto_stop,
            }) => {
                if session.is_some() {
                    tracing::error!(
                        target: "molvi::engine",
                        "Start during active session (ignored)"
                    );
                } else {
                    // Clear the resampler's FFT overlap + leftover from the
                    // previous session so the first output frames are clean
                    // (not the prior utterance's attenuated ~10ms tail). No-op
                    // on the 16k passthrough path.
                    resampler.reset();
                    session = Some(Session {
                        on_partial,
                        frame_buf: Vec::with_capacity(VAD_FRAME * 4),
                        feed_wall: std::time::Duration::ZERO,
                        feed_samples: 0,
                        feed_sq: 0.0,
                        auto_stop: auto_stop.map(SilenceTracker::new),
                    });
                    tracing::info!(target: "molvi::engine", "session started");
                }
            }
            Ok(EngineCmd::Finalize { reply }) => match session.take() {
                Some(s) => {
                    let result = finalize_session(&mut *engine, &mut resampler, &mut consumer, s);
                    let _ = reply.send(result);
                }
                None => {
                    tracing::warn!(
                        target: "molvi::engine",
                        "Finalize with no active session"
                    );
                    let _ = reply.send((String::new(), None));
                }
            },
            Ok(EngineCmd::Shutdown) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }

        // 2. Drain all available samples (non-blocking).
        batch.clear();
        while let Ok(s) = consumer.pop() {
            batch.push(s);
        }

        // 3. Feed only when a session is active; otherwise discard (batch is
        //    cleared next iteration). Idle/empty -> the recv_timeout above
        //    already paced this iteration; loop straight back (no extra sleep).
        if let Some(sess) = session.as_mut() {
            // Auto-stop fire check runs every iteration (2ms cadence),
            // BEFORE the batch gate, so trailing SILENCE — not just new audio
            // — drives it. Placed at the top of the block because the
            // feed_chunk panic arm below reassigns `session = None`, which the
            // borrow checker rejects while `sess` is live past the while loop;
            // keeping the fire-check scoped here lets `sess` drop before that
            // arm. Privacy: `Command::AutoStop` carries no text; timing is
            // metadata only.
            if let Some(tracker) = sess.auto_stop.as_mut()
                && tracker.should_fire(Instant::now())
            {
                tracker.mark_fired();
                let _ = cmd_tx.send(coordinator::Command::AutoStop);
            }
            if batch.is_empty() {
                continue;
            }
            let resampled = match resampler.process(&batch) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(target: "molvi::engine", "resample: {e}");
                    continue;
                }
            };
            sess.feed_sq += resampled
                .iter()
                .map(|s| (*s as f64) * (*s as f64))
                .sum::<f64>();
            sess.frame_buf.extend_from_slice(&resampled);
            while sess.frame_buf.len() >= VAD_FRAME {
                let chunk: Vec<f32> = sess.frame_buf.drain(..VAD_FRAME).collect();
                // Feed this 30ms block's RMS to the silence tracker
                // (auto-stop). RMS = sqrt(mean(s^2)); metadata-only.
                if let Some(tracker) = sess.auto_stop.as_mut() {
                    let sq: f64 = chunk.iter().map(|s| (*s as f64) * (*s as f64)).sum();
                    let rms = (sq / chunk.len() as f64).sqrt();
                    tracker.observe_rms(rms, Instant::now());
                }
                let cb: &(dyn Fn(&str) + Send + Sync) = sess.on_partial.as_ref();
                let t0 = std::time::Instant::now();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    engine.feed_chunk(&chunk, cb)
                }));
                match outcome {
                    Ok(Ok(())) => {
                        sess.feed_wall += t0.elapsed();
                        sess.feed_samples += chunk.len();
                    }
                    Ok(Err(e)) => tracing::error!(target: "molvi::engine", "feed_chunk: {e}"),
                    Err(_panic) => {
                        // Fixed string only — never interpolate the panic payload
                        // (privacy §10.1: a Box<dyn Any> payload could carry
                        // transcript text from a dependency panic).
                        tracing::error!(
                            target: "molvi::engine",
                            "feed_chunk panic recovered, ending session"
                        );
                        // Best-effort reset: finish() runs reset_state unless it
                        // panics too. ponytail: a guaranteed rebuild needs stored
                        // config (VadChunkedConfig isn't Clone); rare ort-internal
                        // panic, deferred.
                        let noop: &(dyn Fn(&str) + Send + Sync) = &|_: &str| {};
                        let _ = finish_safely(&mut *engine, noop);
                        session = None;
                        break;
                    }
                }
            }
        }
    }
}

/// Drain the SPSC ring + resampler leftover + sub-frame remainder, then finish.
/// Called from the Finalize arm so the last phoneme/word isn't clipped: samples
/// still in the ring (arrived after the last feed iteration) and the
/// `<VAD_FRAME` remainder in `frame_buf` are fed to the chunker before
/// `finish()` flushes it. Also flushes the resampler's pending `leftover`.
fn finalize_session(
    engine: &mut dyn SpeechEngine,
    resampler: &mut Resampler,
    consumer: &mut rtrb::Consumer<f32>,
    mut sess: Session,
) -> (String, Option<String>) {
    // 1. Drain the SPSC ring fully (non-blocking; audio is paused by the time
    //    Finalize arrives, so this is a bounded final batch).
    let mut tail = Vec::new();
    while let Ok(s) = consumer.pop() {
        tail.push(s);
    }
    // 2. Resample the tail + flush the resampler's internal leftover (zero-pad).
    let mut resampled = resampler.process(&tail).unwrap_or_default();
    resampled.extend(resampler.flush().unwrap_or_default());
    sess.feed_sq += resampled
        .iter()
        .map(|s| (*s as f64) * (*s as f64))
        .sum::<f64>();
    sess.feed_samples += resampled.len();
    sess.frame_buf.extend_from_slice(&resampled);

    // 3. Feed remaining VAD_FRAME blocks through the chunker.
    let cb: &(dyn Fn(&str) + Send + Sync) = sess.on_partial.as_ref();
    while sess.frame_buf.len() >= VAD_FRAME {
        let chunk: Vec<f32> = sess.frame_buf.drain(..VAD_FRAME).collect();
        let t0 = std::time::Instant::now();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.feed_chunk(&chunk, cb)
        })) {
            Ok(Ok(())) => {
                sess.feed_wall += t0.elapsed();
                sess.feed_samples += chunk.len();
            }
            Ok(Err(e)) => tracing::error!(target: "molvi::engine", "feed_chunk (finalize): {e}"),
            Err(_panic) => {
                tracing::error!(
                    target: "molvi::engine",
                    "feed_chunk panic recovered during finalize"
                );
                break;
            }
        }
    }
    // 4. Sub-VAD_FRAME remainder: zero-pad to one final frame so the VAD gets a
    //    complete window; finish() flushes whatever the chunker buffers.
    if !sess.frame_buf.is_empty() {
        let mut rem = std::mem::take(&mut sess.frame_buf);
        rem.resize(VAD_FRAME, 0.0);
        if let Err(e) = engine.feed_chunk(&rem, cb) {
            tracing::error!(target: "molvi::engine", "feed_chunk remainder: {e}");
        }
    }

    // Silence guard: if no chunk was finalized AND the session-wide audio RMS
    // is below the floor, the captured audio was effectively silence (mic muted
    // / very quiet) -> chunker.finish() flushes near-silence -> the CTC model
    // hallucinates garbage (e.g. "hmmmm. ^reach this page"). Still call
    // finish_safely (resets chunker state) but discard its text. Real speech
    // (chunks finalized OR RMS above floor) keeps the transcript. Had_speech is
    // captured BEFORE finish_safely (which clears chunks).
    let session_rms = if sess.feed_samples > 0 {
        (sess.feed_sq / sess.feed_samples as f64).sqrt()
    } else {
        0.0
    };
    let had_speech = engine.had_speech() || session_rms > SILENCE_RMS_FLOOR;
    let finish_t0 = std::time::Instant::now();
    let out = finish_safely(engine, cb);
    let finish_wall = finish_t0.elapsed();
    // RTF logged AFTER finish_safely so finish_wall (Nemotron's real cost) is
    // included — feed_wall alone reads 0.000 for Nemotron (feed just buffers).
    log_rtf(sess.feed_wall, finish_wall, sess.feed_samples);
    if had_speech {
        out
    } else {
        tracing::info!(
            target: "molvi::engine",
            "session silent (rms={session_rms:.4}, no chunks); discarding finish text to avoid CTC hallucination"
        );
        (String::new(), None)
    }
}

/// Run `engine.finish` under `catch_unwind`; on panic or error, log and return
/// empty (the session is unrecoverable, but the worker stays alive).
fn finish_safely(
    engine: &mut dyn SpeechEngine,
    on_partial: PartialCb<'_>,
) -> (String, Option<String>) {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| engine.finish(on_partial))) {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            tracing::error!(target: "molvi::engine", "finish: {e}");
            (String::new(), None)
        }
        // Fixed string only — never interpolate the panic payload (§10.1).
        Err(_panic) => {
            tracing::error!(target: "molvi::engine", "finish panic recovered");
            (String::new(), None)
        }
    }
}

/// RTF measurement (metadata only — never transcript text). RTF = (feed +
/// finish) wall time / audio duration. `feed_wall` times `feed_chunk` (the
/// chunked streaming cost); `finish_wall` times `finish()` (for GigaAM the
/// chunker text-merge, for Nemotron the whole-utterance
/// `transcribe_audio_with_tokens` — Nemotron's feed is a cheap buffer append,
/// so without `finish_wall` its RTF reads 0.000). Task-0 GigaAM measured 0.067;
/// logged once per session on finalize.
fn log_rtf(feed_wall: std::time::Duration, finish_wall: std::time::Duration, samples: usize) {
    if samples == 0 {
        return;
    }
    let feed_secs = feed_wall.as_secs_f64();
    let finish_secs = finish_wall.as_secs_f64();
    let audio_secs = samples as f64 / 16_000.0;
    let rtf = if audio_secs > 0.0 {
        (feed_secs + finish_secs) / audio_secs
    } else {
        0.0
    };
    tracing::info!(
        target: "molvi::engine",
        feed_secs = %format!("{feed_secs:.3}"),
        finish_secs = %format!("{finish_secs:.3}"),
        audio_secs = %format!("{audio_secs:.3}"),
        rtf = %format!("{rtf:.3}"),
        "session totals"
    );
}

#[cfg(test)]
mod silence_tracker_tests {
    use super::*;
    use std::time::Duration;

    fn tracker(trailing_ms: u64, threshold: f64) -> SilenceTracker {
        SilenceTracker::new(AutoStopConfig {
            trailing_silence: Duration::from_millis(trailing_ms),
            energy_threshold: threshold,
        })
    }

    // `Instant`s can only be obtained from `Instant::now()` (no public ctor),
    // so tests build wall-clock anchors + Duration offsets. `duration_since`
    // is only ever called with `now >= last_speech_at` here, so no underflow.

    #[test]
    fn fires_after_trailing_silence_post_speech() {
        let mut t = tracker(50, 0.01);
        let t0 = Instant::now();
        t.observe_rms(0.5, t0); // loud frame starts the clock
        // Just under the trailing window -> not yet.
        assert!(!t.should_fire(t0 + Duration::from_millis(49)));
        // `>=` semantics: AT the boundary (50ms) and past it -> fires.
        assert!(t.should_fire(t0 + Duration::from_millis(50)));
        assert!(t.should_fire(t0 + Duration::from_millis(51)));
    }

    #[test]
    fn never_speech_never_fires() {
        // Only silent RMS -> last_speech_at stays None -> never fires, well
        // past the trailing window.
        let mut t = tracker(50, 0.01);
        let t0 = Instant::now();
        t.observe_rms(0.0, t0);
        t.observe_rms(0.001, t0 + Duration::from_millis(10));
        assert!(!t.should_fire(t0 + Duration::from_secs(5)));
    }

    #[test]
    fn fires_only_once() {
        let mut t = tracker(50, 0.01);
        let t0 = Instant::now();
        t.observe_rms(0.5, t0);
        let fire_at = t0 + Duration::from_millis(51);
        assert!(t.should_fire(fire_at));
        t.mark_fired();
        assert!(!t.should_fire(fire_at));
        assert!(!t.should_fire(fire_at + Duration::from_secs(2)));
        // Loud frames after firing must not revive it.
        t.observe_rms(0.9, fire_at + Duration::from_millis(5));
        assert!(!t.should_fire(fire_at + Duration::from_secs(3)));
    }

    #[test]
    fn loud_frame_resets_clock() {
        let mut t = tracker(50, 0.01);
        let t0 = Instant::now();
        t.observe_rms(0.5, t0);
        // A second loud frame shortly after (still within trailing) resets.
        let t1 = t0 + Duration::from_millis(20);
        t.observe_rms(0.5, t1);
        // At t0+51ms (old clock's fire point) we are only 31ms past t1 -> no.
        assert!(!t.should_fire(t0 + Duration::from_millis(51)));
        // 51ms past the RESET point t1 -> fires.
        assert!(t.should_fire(t1 + Duration::from_millis(51)));
    }
}

#[cfg(test)]
#[cfg(feature = "engine-model-test")]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    // Model-gated: requires the ~214MB model at molvi-task0/models. Run with:
    //   cargo test --manifest-path src-tauri/Cargo.toml --features engine-model-test engine
    #[cfg(feature = "engine-model-test")]
    fn fixture_model_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("molvi-task0")
            .join("models")
            .join("gigaam-v3-e2e-ctc")
    }

    #[cfg(feature = "engine-model-test")]
    fn fixture_wav() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("molvi-task0")
            .join("tests")
            .join("fixtures")
            .join("ru")
            .join("example.wav")
    }

    #[cfg(feature = "engine-model-test")]
    #[test]
    fn transcribes_fixture_with_punctuation() {
        let model_dir = fixture_model_dir();
        if !model_dir.exists() {
            eprintln!("skipping: model not present at {}", model_dir.display());
            return;
        }
        let settings = crate::settings::Settings::default();
        let mut engine = Engine::load(&model_dir, &settings).expect("engine load");

        let result = engine
            .transcribe_offline(&fixture_wav())
            .expect("transcribe_offline");
        assert!(!result.text.is_empty(), "transcript empty");
        assert!(
            result.text.chars().any(|c| ".,!?;:".contains(c)),
            "no punctuation: {}",
            result.text
        );
        // Cyrillic vowel sanity (not WER; rigorous WER is Task 13).
        assert!(
            result.text.contains('о') || result.text.contains('а'),
            "no cyrillic vowels: {}",
            result.text
        );
    }

    // Streaming: feed the fixture through feed_chunk (captures partials), then
    // finish. The finalized text must equal the last emitted partial.
    #[cfg(feature = "engine-model-test")]
    #[test]
    fn streaming_emits_growing_partials() {
        let model_dir = fixture_model_dir();
        if !model_dir.exists() {
            eprintln!("skipping: model not present at {}", model_dir.display());
            return;
        }
        let settings = crate::settings::Settings::default();

        let captured = Arc::new(Mutex::new(String::new()));
        let cap_cb = captured.clone();
        let on_partial: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |t| {
            *cap_cb.lock().unwrap() = t.to_string();
        });

        let mut engine = Engine::load(&model_dir, &settings).expect("engine load");
        let samples = transcribe_rs::audio::read_wav_samples(&fixture_wav()).expect("read wav");
        engine
            .feed_chunk(&samples, on_partial.as_ref())
            .expect("feed_chunk");
        let (final_text, lang) = engine.finish(on_partial.as_ref()).expect("finish");

        assert!(!final_text.is_empty(), "final transcript empty");
        assert_eq!(
            final_text,
            *captured.lock().unwrap(),
            "last partial must equal final"
        );
        // GigaAM is a fixed Russian CTC head — reports its lang verbatim.
        assert_eq!(lang.as_deref(), Some("ru"), "GigaAM must report ru");
    }
}
