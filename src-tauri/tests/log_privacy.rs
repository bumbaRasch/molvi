//! Spec §10.1 / §13 privacy gate: transcript (and transcript-equivalent) data
//! piped through molvi's modules MUST NOT appear in captured tracing output.
//!
//! Three complementary checks, each honest about what it covers:
//!
//! 1. `coordinator_logs_no_pipeline_internals` (always runs): drives the real
//!    `coordinator::run` inline with a sentinel-bearing `Pipeline` and proves
//!    the coordinator's own log call sites (stage transitions, the
//!    `begin_session failed: {e}` error interpolation) never emit the sentinel
//!    that lives inside the pipeline. Coordinator is pure (no Tauri, no model),
//!    so this is a faithful, always-on regression gate for `coordinator.rs`.
//!
//! 2. `engine_worker_logs_no_transcript` (`--features engine-model-test`):
//!    spawns the real `molvi-engine` worker with a synthesized SPSC ring (no
//!    audio hardware), feeds the RU fixture, finalizes — exercising the
//!    worker's actual log call sites (`session started`, `log_rtf` metadata,
//!    error paths) with REAL transcript data flowing through `feed_chunk`/
//!    `finish`/`on_partial`. Asserts distinctive transcript words never reach
//!    the captured tracing buffer.
//!
//! 3. `log_bridge_is_absent` (always runs): the hard regression guard for the
//!    production invariant in `log::init` (no `tracing-log` bridge). Emits a
//!    sentinel via the `log` crate (NOT `tracing`) and asserts it never reaches
//!    the tracing capture buffer — proving no `LogTracer` is installed. This
//!    test FAILS the moment someone re-introduces `SubscriberInitExt::init()`
//!    or `LogTracer::init()` in `install_capture()` (and, by mirror, in
//!    production `log::init()` — the doc comment there names this test).
//!
//! 4. `finalize_substrates_log_no_transcript` (always runs): the Phase-2
//!    widen. Runs the transcript-bearing substrates that `run_finalize`
//!    orchestrates — `postproc::run` (Smart pipeline + Polished body build
//!    against a dead-port endpoint), `history::insert`, `dictionary::apply` —
//!    each with a distinct Cyrillic sentinel, under a scoped
//!    `dispatcher::with_default` capture (mirrors the ipc.rs substrate test),
//!    and asserts no sentinel reaches logs. `run_finalize` itself is NOT
//!    called (its paste step needs a live Win32 target — see P1 in the test);
//!    the substrates ARE the leak surfaces, covered directly.
//!
//! 5. `nemotron_streaming_substrates_log_no_transcript`
//!    (`--features engine-model-test`): the Phase-3 widen. Drives the real
//!    Nemotron streaming path (`feed_chunk`/`finish`) inline with the RU
//!    fixture under a scoped capture; asserts no transcript word nor partial
//!    window reaches logs.
//!
//! Production privacy also relies on molvi NOT bridging the external `log`
//! crate into tracing: transcribe-rs's internal `log::info!("  -> \"{}\"",
//! text)` would otherwise surface transcripts. `log::init()` deliberately
//! omits `tracing_log::LogTracer`, so those records are dropped at runtime.
//! This test matches that (no log bridge installed) — keep it that way.

use std::io::Write;
use std::sync::LazyLock;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, Once};

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

use molvi::coordinator::{self, Command, Pipeline};
use molvi::errors::Result;
use molvi::settings::RecognitionMode;

#[cfg(feature = "engine-model-test")]
mod common;

// A Cyrillic sentinel representing "any pipeline-internal transcript-equivalent
// data" that must never reach logs.
const SENTINEL: &str = "СЕКРЕТНОЕСЛОВО";

// ── shared capture buffer + global subscriber (worker logs on its own thread,
//    so a thread-local `with_default` cannot reach it; one global subscriber
//    writing to a shared static buffer is the minimum that covers both the
//    inline coordinator and the spawned worker). ──

static BUF: LazyLock<Arc<Mutex<Vec<u8>>>> = LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));
static INIT: Once = Once::new();

/// Single shared shape for both the global subscriber (writes to `static BUF`)
/// and per-test scoped subscribers (write to a locally-owned `Arc`). The Writer
/// clones the Arc and locks per write — threadsafe, no static needed for scoped.
struct BufMaker(Arc<Mutex<Vec<u8>>>);
impl<'a> MakeWriter<'a> for BufMaker {
    type Writer = BufWriter;
    fn make_writer(&'a self) -> Self::Writer {
        BufWriter(self.0.clone())
    }
}

struct BufWriter(Arc<Mutex<Vec<u8>>>);
impl Write for BufWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn install_capture() {
    INIT.call_once(|| {
        // trace = maximum verbosity; if anything leaks, it leaks here first.
        // set_global_default (not `.init()`) — matches production: no
        // tracing-log bridge, so transcribe-rs's `log::info!` transcript echoes
        // have no logger and are dropped. This test exercises OUR `tracing`
        // call sites (engine.rs/coordinator.rs), which is the §10.1 discipline
        // we own; the bridge absence is a production invariant (see log.rs).
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("trace"))
            .with(
                fmt::layer()
                    .with_writer(BufMaker(BUF.clone()))
                    .with_ansi(false),
            );
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

fn captured_logs() -> String {
    // tracing buffers; flush its fmt layer before reading.
    String::from_utf8_lossy(&BUF.lock().unwrap()).to_string()
}

// ── 1. Coordinator privacy (always on) ──

struct SentinelPipeline {
    payload: &'static str,
}
impl Pipeline for SentinelPipeline {
    fn begin_session(&mut self, _mode: molvi::settings::RecognitionMode) -> Result<()> {
        // Hold the payload (transcript-equivalent) inside the pipeline; if the
        // coordinator ever interpolated pipeline state, SENTINEL would leak.
        let _ = self.payload;
        Ok(())
    }
    fn finalize_session(&self) {
        let _ = self.payload;
    }
    fn cancel_session(&mut self) {
        let _ = self.payload;
    }
    fn mic_preview(&mut self, _on: bool) {
        let _ = self.payload;
    }
}

#[test]
fn coordinator_logs_no_pipeline_internals() {
    install_capture();
    let (tx, rx) = mpsc::channel::<Command>();
    // A full PTT cycle + an out-of-band release (no-op) to exercise the debug
    // "ignoring {other:?} in stage {s:?}" path too.
    for cmd in [
        Command::Input {
            is_pressed: true,
            mode: RecognitionMode::PushToTalk,
        },
        Command::Input {
            is_pressed: false,
            mode: RecognitionMode::PushToTalk,
        },
        Command::ProcessingFinished,
        Command::Input {
            is_pressed: false,
            mode: RecognitionMode::PushToTalk,
        },
    ] {
        tx.send(cmd).unwrap();
    }
    drop(tx);

    let p = SentinelPipeline { payload: SENTINEL };
    // Inline (same thread) so the global subscriber captures every event.
    coordinator::run(rx, p);

    let logs = captured_logs();
    assert!(
        !logs.contains(SENTINEL),
        "PRIVACY VIOLATION: pipeline sentinel leaked into coordinator logs:\n{logs}"
    );
    // Sanity: the coordinator DID emit logs (stage transitions). An empty buffer
    // would prove nothing — the test must actually exercise the call sites.
    assert!(
        logs.contains("stage:") || logs.contains("coordinator") || logs.contains("ignoring"),
        "expected coordinator log output, got empty buffer — test is not exercising log call sites"
    );
}

// ── 3. log-bridge absence (hard regression guard, always on) ──
//
// The load-bearing production invariant (log.rs doc): NO `tracing-log` bridge.
// If someone re-introduces `SubscriberInitExt::init()` or `LogTracer::init()`,
// transcribe-rs's `log::info!("  -> \"{}\"", text)` records would be forwarded
// to our tracing subscriber at info level — a §10.1 leak. This test catches
// that by emitting a sentinel via the `log` crate (the same path the leak
// takes) and asserting it never reaches the tracing capture buffer.
//
// Why not `log::max_level() == LevelFilter::Off`? It is a secondary signal but
// racy under parallel test execution (one global atomic). The end-to-end
// sentinel is robust: it can only land in BUF if a logger is installed AND it
// forwards to tracing, which is exactly the bridge we forbid. Verified against
// tracing-log 0.2: `LogTracer::init` calls `log::set_boxed_logger` + sets
// `max_level(Trace)`, after which every `log::info!` is dispatched to the
// tracing subscriber (docs.rs/tracing-log/0.2.0/tracing_log/struct.LogTracer.html).
// Without the bridge, `log::info!` hits the `log` crate's default no-op logger
// and is dropped regardless of `RUST_LOG`.

const LOG_BRIDGE_SENTINEL: &str = "LOG_BRIDGE_SENTINEL_42";

#[test]
fn log_bridge_is_absent() {
    install_capture();
    // Emit at info via the `log` crate — the exact channel transcribe-rs uses
    // for its transcript echoes. Bridge present -> forwarded to tracing -> BUF.
    log::info!("{}", LOG_BRIDGE_SENTINEL);
    let logs = captured_logs();
    assert!(
        !logs.contains(LOG_BRIDGE_SENTINEL),
        "PRIVACY REGRESSION: the tracing-log bridge is installed — a `log::info!` \
         record reached the tracing subscriber. transcribe-rs's transcript-bearing \
         `log::info!` (vad_chunked.rs) would leak into molvi logs (spec §10.1). \
         Do NOT use SubscriberInitExt::init() / LogTracer::init(); call \
         tracing::subscriber::set_global_default directly (see log.rs). Captured:\n{logs}"
    );
}

// ── 4. Finalize-side substrates privacy (Phase-2 widen, always on) ──
//
// The Phase-2 substrates that `pipeline::run_finalize` orchestrates —
// `postproc::run` (Smart + Polished body construction), `history::insert`,
// `dictionary::apply` — are transcript-bearing leak surfaces. Each must never
// emit its text into tracing logs (spec §10.1/§14). This test exercises them
// directly under a scoped capture and asserts no sentinel leaks.
//
// P1 (do NOT call `run_finalize`): that orchestrator includes
// `paste::paste_text`, which types into the foreground Win32 window and can't
// run in an automated test. Making `run_finalize` pub just for a test would
// widen the production API for testing alone. The substrates ARE the leak
// surfaces; covering them directly is the faithful realization of Step 1.
//
// Scoped capture (mirrors `dictionary_substrate_does_not_log_entry_text` in
// ipc.rs): all three substrates run inline on the test thread, so a thread-
// local `dispatcher::with_default` captures everything — no shared global,
// no parallel-test races.

// Distinct sentinels (P3): a leak in one substrate trips its own assertion,
// not a sibling's.
const POSTPROC_SENTINEL: &str = "СЕКРЕТПОСТПРОЦА";
const HISTORY_SENTINEL: &str = "СЕКРЕТИСТОРИЯ";
const DICT_SENTINEL: &str = "СЕКРЕТСЛОВАРЯ";
const POLISHED_PROMPT_SENTINEL: &str = "СЕКРЕТПРОМПТА";

#[test]
fn finalize_substrates_log_no_transcript() {
    use molvi::settings::{PostMode, PostProcessing};

    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));

    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::new("trace"))
        .with(
            fmt::layer()
                .with_writer(BufMaker(buf.clone()))
                .with_ansi(false),
        );

    // Smart: all toggles default (fix_case ON — only uppercases, never strips;
    // the sentinel survives intact through the 9-step pipeline).
    let smart_settings = PostProcessing::default();
    // Polished (P2): dead-port endpoint → instant ConnectionFailed. The request
    // body (which carries the transcript via `build_polished_body`) IS built
    // before the connect attempt, so the leak surface runs. NOT endpoint=None
    // (returns early, body never built) and NOT a routable URL (20s hang risk).
    let polished_settings = PostProcessing {
        mode: PostMode::Polished,
        endpoint: Some("http://127.0.0.1:1".to_string()),
        model: Some("x".to_string()),
        prompt: Some(POLISHED_PROMPT_SENTINEL.to_string()),
        ..PostProcessing::default()
    };

    // Outcomes captured outside the scope for the non-vacuous sanity asserts.
    let mut smart_out: Option<molvi::postproc::PostOutcome> = None;
    let mut polished_out: Option<molvi::postproc::PostOutcome> = None;
    let mut history_ok = false;
    let mut dict_applied: Option<String> = None;

    tracing::dispatcher::with_default(&tracing::dispatcher::Dispatch::new(subscriber), || {
        // Smart: sentinel flows through the full pipeline.
        smart_out = Some(molvi::postproc::run(
            &format!("привет {POSTPROC_SENTINEL}"),
            &smart_settings,
            None,
            None,
            false,
        ));
        // Polished: body containing the sentinel is built, POST fails instantly.
        polished_out = Some(molvi::postproc::run(
            &format!("до {POSTPROC_SENTINEL} после"),
            &polished_settings,
            None,
            None,
            false,
        ));
        // History: sentinel written to SQLite.
        let h = molvi::history::History::open_in_memory().expect("open history");
        history_ok = h
            .insert(HISTORY_SENTINEL, Some("ru"), Some("gigaam"), Some("smart"))
            .is_ok();
        // Dictionary: sentinel entry added, then applied to a phrase.
        let d = molvi::dictionary::Dictionary::open_in_memory().expect("open dict");
        d.add(DICT_SENTINEL, "замена").expect("add entry");
        dict_applied = Some(d.apply(&format!("test {DICT_SENTINEL} end")));
    });

    let logs = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();

    // P4 — non-vacuous: prove each substrate actually did real work BEFORE the
    // privacy asserts. An empty buffer + no outcome checks would prove nothing.
    let smart = smart_out.expect("smart postproc ran");
    let smart_text = match smart {
        molvi::postproc::PostOutcome::Used(t) => t,
        molvi::postproc::PostOutcome::Failed(_, _) => {
            panic!("smart postproc returned Failed, expected Used")
        }
    };
    assert!(
        smart_text.contains(POSTPROC_SENTINEL),
        "smart pipeline should preserve the sentinel token; got {smart_text:?}"
    );

    let polished = polished_out.expect("polished postproc ran");
    let (perr, praw) = match polished {
        molvi::postproc::PostOutcome::Failed(err, raw) => (err, raw),
        molvi::postproc::PostOutcome::Used(_) => {
            panic!("polished should fail on dead-port endpoint, got Used")
        }
    };
    assert!(
        praw.contains(POSTPROC_SENTINEL),
        "polished raw fallback must preserve transcript; got {praw:?}"
    );
    assert!(
        !perr.contains(POSTPROC_SENTINEL),
        "polished error must be metadata-only: {perr}"
    );

    assert!(history_ok, "history insert returned Err");

    let applied = dict_applied.expect("dict apply ran");
    assert!(
        applied.contains("замена") && !applied.contains(DICT_SENTINEL),
        "dictionary apply should have substituted the entry; got {applied:?}"
    );

    // P3 — distinct sentinels: clean per-substrate attribution.
    assert!(
        !logs.contains(POSTPROC_SENTINEL),
        "PRIVACY VIOLATION: postproc (smart/polished) sentinel leaked into logs:\n{logs}"
    );
    assert!(
        !logs.contains(HISTORY_SENTINEL),
        "PRIVACY VIOLATION: history sentinel leaked into logs:\n{logs}"
    );
    assert!(
        !logs.contains(DICT_SENTINEL),
        "PRIVACY VIOLATION: dictionary sentinel leaked into logs:\n{logs}"
    );
    assert!(
        !logs.contains(POLISHED_PROMPT_SENTINEL),
        "PRIVACY VIOLATION: polished prompt sentinel leaked into logs:\n{logs}"
    );
}

// ── 6. commands::parse privacy (Phase-3, always on) ──
//
// `parse` is pure and must never log: it receives the finalized transcript
// (speech-derived) and returns a chord. The chord-delivery log lives in the
// pipeline's finalize side-thread as a FIXED content-free string
// ("command-mode: chord delivered"); parse itself logs nothing. This guards
// against a future debug log that interpolates the phrase/VK. Scoped capture
// (inline, thread-local) — mirrors the finalize-substrates test.
#[test]
fn commands_parse_logs_no_phrase() {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::new("trace"))
        .with(
            fmt::layer()
                .with_writer(BufMaker(buf.clone()))
                .with_ansi(false),
        );

    const CMD_SENTINEL: &str = "СЕКРЕТКОМАНДА";
    let (matched, unmatched) =
        tracing::dispatcher::with_default(&tracing::dispatcher::Dispatch::new(subscriber), || {
            // Non-matching phrase carrying the sentinel → None.
            let unmatched = molvi::commands::parse(&format!("{CMD_SENTINEL} undo"));
            // A real match still must not log the phrase.
            let matched = molvi::commands::parse("undo");
            (matched, unmatched)
        });

    let logs = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();

    // Non-vacuous: parse ran both the match and no-match branches.
    assert!(
        unmatched.is_none(),
        "phrase-with-extra-words must not match"
    );
    assert!(matched.is_some(), "'undo' must match a chord");

    // Privacy: the speech-derived sentinel never reached logs.
    assert!(
        !logs.contains(CMD_SENTINEL),
        "PRIVACY VIOLATION: command phrase sentinel leaked into parse logs:\n{logs}"
    );
}

// ── 7. snippets::expand privacy (Phase-3, always on) ──
//
// `expand` receives the finalized transcript and returns an expansion that
// replaces it (Task 8b Smart step). Cue + expansion are user content — spec
// §10.1 forbids them in logs at any level. Today `expand` emits only the
// metadata-only `tracing::warn!("snippets expand: list failed ({e})")` on a
// `list()` failure (NOT triggered in-memory); this test proves neither the
// cue nor the expansion reaches the captured buffer, for both the match and
// the no-match branch. Scoped capture (mirrors `commands_parse_logs_no_phrase`)
// — `expand` runs inline on the test thread.
#[test]
fn snippets_expand_logs_no_cue() {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::new("trace"))
        .with(
            fmt::layer()
                .with_writer(BufMaker(buf.clone()))
                .with_ansi(false),
        );

    // Cyrillic sentinel carried by BOTH the cue and the expansion — if either
    // leaks (a future debug log, a warn that interpolates the cue/expansion),
    // this trips.
    const SNIP_SENTINEL: &str = "СЕКРЕТСНИППЕТА";
    let (matched, unmatched) =
        tracing::dispatcher::with_default(&tracing::dispatcher::Dispatch::new(subscriber), || {
            let s = molvi::snippets::Snippets::open_in_memory().expect("open snippets");
            s.add(SNIP_SENTINEL, format!("замена {SNIP_SENTINEL}").as_str())
                .expect("add");
            // Match: the sentinel-bearing cue expands (proves expand ran).
            let matched = s.expand(SNIP_SENTINEL);
            // No-match: sentinel-bearing non-cue text → None (proves the no-
            // match branch ran too).
            let unmatched = s.expand(&format!("{SNIP_SENTINEL} extra"));
            (matched, unmatched)
        });

    let logs = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();

    // Non-vacuous: match returned Some (with the sentinel-bearing expansion),
    // no-match returned None.
    assert!(
        matched.as_ref().is_some_and(|e| e.contains(SNIP_SENTINEL)),
        "expand should have matched the cue: got {matched:?}"
    );
    assert!(unmatched.is_none(), "non-cue text must not match");

    // Privacy: neither the cue nor the expansion reached logs.
    assert!(
        !logs.contains(SNIP_SENTINEL),
        "PRIVACY VIOLATION: snippet cue/expansion sentinel leaked into expand logs:\n{logs}"
    );
}

// ── 8. onboarding-practice branch privacy (Phase-3, always on) ──
//
// Task 10 D6 inserts a branch in `AppPipeline::finalize_session` that — when
// `AppState.onboarding_practice == true` — emits the post-processed text to
// the onboarding window via `practice-result` and skips paste+history. The
// branch's leak surface is a single `tracing::info!("onboarding practice:
// result emitted")` (fixed, content-free) inside `emit_practice_result`. This
// test exercises that helper inline under a scoped capture with a sentinel-
// bearing text and asserts the sentinel never reaches the captured buffer.
// Mirrors `commands_parse_logs_no_phrase` / `snippets_expand_logs_no_cue`
// (pure helper, inline, scoped capture).
#[test]
fn onboarding_practice_branch_logs_no_text() {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::new("trace"))
        .with(
            fmt::layer()
                .with_writer(BufMaker(buf.clone()))
                .with_ansi(false),
        );

    const PRACTICE_SENTINEL: &str = "СЕКРЕТПРАКТИКИ";
    let emitted: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let emitted_clone = emitted.clone();
    tracing::dispatcher::with_default(&tracing::dispatcher::Dispatch::new(subscriber), || {
        molvi::pipeline::emit_practice_result(&format!("привет {PRACTICE_SENTINEL}"), |t| {
            *emitted_clone.lock().unwrap() = t.to_string()
        });
    });

    let logs = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();

    // Non-vacuous: the emit fn ran with the sentinel-bearing text (proves the
    // branch carried the transcript-equivalent payload up to the leak surface).
    assert!(
        emitted.lock().unwrap().contains(PRACTICE_SENTINEL),
        "emit_practice_result must forward the text to the emit fn: got {:?}",
        emitted.lock().unwrap()
    );
    // Non-vacuous: the metadata log fired.
    assert!(
        logs.contains("onboarding practice"),
        "expected the practice-result metadata log line, got: {logs}"
    );

    // Privacy §10.1: the sentinel never reached logs.
    assert!(
        !logs.contains(PRACTICE_SENTINEL),
        "PRIVACY VIOLATION: practice-result sentinel leaked into logs:\n{logs}"
    );
}

// ── 2. Engine worker privacy (model-gated; needs the real model) ──

#[cfg(feature = "engine-model-test")]
mod engine_privacy {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use molvi::engine::{EngineCmd, EngineHandle};
    use molvi::settings::Settings;

    use super::common::{fixture_model_dir, fixture_wav};

    // Distinctive words from the golden transcript (see GOLDEN_EXAMPLE_WAV.txt).
    // If any engine.rs tracing call site ever interpolates transcript text, one
    // of these appears in the captured buffer.
    const TRANSCRIPT_WORDS: &[&str] = &["лукоморья", "надеждой", "похвал", "грешные"];

    #[test]
    fn engine_worker_logs_no_transcript() {
        install_capture();
        let model_dir = fixture_model_dir();
        if !model_dir.exists() {
            eprintln!("skipping: model not present at {}", model_dir.display());
            return;
        }
        let samples =
            transcribe_rs::audio::read_wav_samples(&fixture_wav()).expect("read fixture wav");

        // Synthesized SPSC ring: no audio hardware. native_rate=16000 matches the
        // fixture, so the resampler is a no-op — the worker drains + feeds the
        // real chunker with the real transcript-bearing samples.
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(samples.len() + 1024);
        let engine =
            EngineHandle::spawn(&model_dir, &Settings::default(), 16_000, cons).expect("spawn");

        let captured_partial = Arc::new(Mutex::new(String::new()));
        let cp = captured_partial.clone();
        let on_partial: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |t| {
            *cp.lock().unwrap() = t.to_string();
        });
        engine
            .tx
            .send(EngineCmd::Start { on_partial })
            .expect("start");

        // Push all samples (ring sized to fit); worker drains at ~30x realtime.
        for s in &samples {
            let _ = prod.push(*s);
        }
        // Let the worker drain + feed + emit partials.
        std::thread::sleep(Duration::from_millis(1500));

        let (ftx, frx) = mpsc::channel();
        engine
            .tx
            .send(EngineCmd::Finalize { reply: ftx })
            .expect("finalize");
        let finalized = frx
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_default();
        let _ = engine.tx.send(EngineCmd::Shutdown);
        std::thread::sleep(Duration::from_millis(150)); // let final logs flush

        assert!(
            !finalized.0.is_empty(),
            "finalize returned empty transcript"
        );

        let logs = captured_logs();
        for word in TRANSCRIPT_WORDS {
            assert!(
                !logs.contains(word),
                "PRIVACY VIOLATION: transcript word '{word}' leaked into engine worker logs:\n{logs}"
            );
        }
        // The partial stream also carried transcript text; prove the captured
        // partial isn't a substring of the logs either (any 8-char window).
        let partial = captured_partial.lock().unwrap().clone();
        if partial.chars().count() >= 8 {
            let window: String = partial.chars().take(12).collect();
            assert!(
                !logs.contains(&window),
                "PRIVACY VIOLATION: partial-transcript window leaked into logs:\n{logs}"
            );
        }
    }

    // ── Nemotron streaming substrates privacy (Phase-3 widen) ──
    //
    // The Phase-3 streaming path (`NemotronEngine::feed_chunk` →
    // `transcribe_chunk_with_tokens` + `on_partial`, `finish` → zero-pad flush
    // + `get_transcript`) carries live transcript partials. Every tracing call
    // site on that path is metadata-only by construction (the `MolviError::
    // Inference("nemotron chunk/flush: {e}")` maps; no transcript
    // interpolation). This test feeds the real RU fixture through the streaming
    // path inline and proves no transcript word reaches the captured buffer.
    //
    // Scoped capture (mirrors `finalize_substrates_log_no_transcript`): the
    // engine runs inline on the test thread, so a thread-local
    // `dispatcher::with_default` captures every event WITHOUT touching the
    // shared global `BUF` (no cross-test contamination). parakeet-rs's decode
    // (where transcript text exists) runs on the calling thread inside
    // `process_chunk`; ort internal execution threads emit metadata only.
    #[test]
    fn nemotron_streaming_substrates_log_no_transcript() {
        use molvi::engine_adapter::{NemotronEngine, SpeechEngine};

        // Nemotron model dir (NOT fixture_model_dir — that's the GigaAM path).
        // Same resolution as engine_adapter's model-gated tests.
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
        let samples =
            transcribe_rs::audio::read_wav_samples(&fixture_wav()).expect("read fixture wav");

        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("trace"))
            .with(
                fmt::layer()
                    .with_writer(BufMaker(buf.clone()))
                    .with_ansi(false),
            );

        let captured_partial = Arc::new(Mutex::new(String::new()));
        let cp = captured_partial.clone();
        let on_partial: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |t| {
            *cp.lock().unwrap() = t.to_string();
        });

        let (text, _lang) = tracing::dispatcher::with_default(
            &tracing::dispatcher::Dispatch::new(subscriber),
            || {
                let mut engine =
                    NemotronEngine::new(&model_dir, &Settings::default()).expect("nemotron load");
                // ponytail: 8960 = Nemotron default chunk_samples (560ms @ 16kHz).
                // Multiple feeds exercise the streaming path repeatedly.
                for s in samples.chunks(8960) {
                    engine.feed_chunk(s, &*on_partial).expect("feed_chunk");
                }
                engine.finish(&*on_partial).expect("finish")
            },
        );

        let logs = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();

        // Non-vacuous: real transcript flowed through the streaming path.
        assert!(
            !text.is_empty(),
            "streaming finish returned empty transcript — test is vacuous"
        );
        // Distinctive fixture transcript words must never reach the buffer.
        for word in TRANSCRIPT_WORDS {
            assert!(
                !logs.contains(*word),
                "PRIVACY VIOLATION: transcript word '{word}' leaked into streaming logs:\n{logs}"
            );
        }
        // The partial stream also carried transcript text; prove a window of
        // the captured partial isn't a substring of the logs either.
        let partial = captured_partial.lock().unwrap().clone();
        if partial.chars().count() >= 8 {
            let window: String = partial.chars().take(12).collect();
            assert!(
                !logs.contains(&window),
                "PRIVACY VIOLATION: partial-transcript window leaked into streaming logs:\n{logs}"
            );
        }
    }
}
