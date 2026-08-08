use std::panic::AssertUnwindSafe;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::errors::Result;
use crate::settings::RecognitionMode;

#[derive(Debug, Clone)]
pub enum Command {
    Input {
        is_pressed: bool,
        mode: RecognitionMode,
    },
    Cancel,
    ProcessingFinished,
    /// Trailing-silence auto-stop, sent by the engine worker when the
    /// SilenceTracker fires (toggle mode + endpoint enabled only). Carries no
    /// text (privacy §10.1). Valid only in `Stage::Recording`.
    AutoStop,
    /// Settings-UI mic preview toggle. Sets capture run-state independently of
    /// a recording session (single source of truth lives in the Pipeline:
    /// `session_active || preview`). Valid in any Stage.
    MicPreview(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Idle,
    Recording,
    Processing,
}

const DEBOUNCE: Duration = Duration::from_millis(30);

/// Test seam (justified by spec §13): production wires this to audio+engine
/// +paste+overlay in Task 13. Trait stays Tauri-free so it is unit-testable.
pub trait Pipeline: Send {
    fn begin_session(&mut self, mode: RecognitionMode) -> Result<()>;
    fn finalize_session(&self);
    fn cancel_session(&mut self);
    /// Toggle on-demand mic-preview capture (metadata-only; never records).
    fn mic_preview(&mut self, on: bool);
}

/// Single-threaded owner of `Stage`. All input funnels through one mpsc; a
/// handler panic resets to Idle + cancels the session (logged), not aborts.
pub fn run(rx: mpsc::Receiver<Command>, p: impl Pipeline + 'static) {
    run_with_debounce(rx, p, DEBOUNCE);
}

// ponytail: `debounce` param is a test seam — lets the test module inject
// Duration::ZERO (second tap always fires) or a huge window (second tap always
// suppressed) so debounce behavior is exercised deterministically without
// wall-clock sleeps. Production always goes through `run` with the const.
fn run_with_debounce(
    rx: mpsc::Receiver<Command>,
    mut p: impl Pipeline + 'static,
    debounce: Duration,
) {
    let mut stage = Stage::Idle;
    let mut last_press: Option<Instant> = None;
    while let Ok(cmd) = rx.recv() {
        let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
            handle(cmd, &mut stage, &mut last_press, &mut p, debounce);
        }));
        if let Err(_e) = res {
            // Fixed string only — never interpolate the panic payload (privacy
            // §10.1: a Box<dyn Any> payload could carry transcript text). Matches
            // engine.rs catch_unwind handlers.
            tracing::error!("coordinator panic recovered, resetting to Idle");
            stage = Stage::Idle;
            p.cancel_session();
        }
    }
}

fn handle<P: Pipeline>(
    cmd: Command,
    stage: &mut Stage,
    last_press: &mut Option<Instant>,
    p: &mut P,
    debounce: Duration,
) {
    match (cmd, *stage) {
        // Press in Idle → begin (both modes; debounced to suppress key-repeat).
        // `mode` flows to the pipeline so it can arm auto-stop for Toggle.
        (
            Command::Input {
                is_pressed: true,
                mode,
            },
            Stage::Idle,
        ) => {
            if debounced(last_press, debounce) {
                match p.begin_session(mode) {
                    Ok(()) => {
                        *stage = Stage::Recording;
                        tracing::info!("stage: Idle → Recording");
                    }
                    Err(e) => tracing::error!("begin_session failed: {e}"),
                }
            }
        }
        // Release → finalize for PTT AND Command (PTT-release semantics).
        // Toggle release still falls through to the catch-all no-op (its tap
        // press arm finalizes). Backward compatible: PTT release finalizes as
        // before, Toggle release remains a no-op. DECISION 5.
        (
            Command::Input {
                is_pressed: false,
                mode,
            },
            Stage::Recording,
        ) if mode != RecognitionMode::Toggle => {
            p.finalize_session();
            *stage = Stage::Processing;
            tracing::info!("stage: Recording → Processing");
        }
        // Toggle tap → finalize (debounced to suppress key-repeat; spec §9)
        (
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::Toggle,
            },
            Stage::Recording,
        ) => {
            if debounced(last_press, debounce) {
                p.finalize_session();
                *stage = Stage::Processing;
                tracing::info!("stage: Recording → Processing");
            }
        }
        // Engine-driven trailing-silence auto-stop (toggle mode only). The
        // worker's `fired` flag guarantees it fires once; this Stage guard
        // covers the race where the user manually toggle-tapped to finalize
        // just as AutoStop arrived (Recording already → Processing/Idle).
        (Command::AutoStop, Stage::Recording) => {
            p.finalize_session();
            *stage = Stage::Processing;
            tracing::info!("stage: Recording → Processing (auto-stop)");
        }
        // Cancel mid-flight — aborts either Recording or Processing.
        (Command::Cancel, Stage::Recording) | (Command::Cancel, Stage::Processing) => {
            p.cancel_session();
            *stage = Stage::Idle;
            tracing::info!("stage: → Idle (cancel)");
        }
        (Command::ProcessingFinished, Stage::Processing) => {
            *stage = Stage::Idle;
            tracing::info!("stage: Processing → Idle");
        }
        // Mic preview is orthogonal to the Stage machine: valid in any stage.
        // Capture run-state is recomputed in the Pipeline (session_active ||
        // preview). Privacy §10.1: metadata-only log, never the level value.
        (Command::MicPreview(on), _) => {
            p.mic_preview(on);
            tracing::info!("mic preview {}", if on { "on" } else { "off" });
        }
        // No-ops (Toggle release in Recording, release without press, press
        // while processing, etc.): ignore.
        (other, s) => tracing::debug!("ignoring {other:?} in stage {s:?}"),
    }
}

/// Returns true if this press is a genuine tap (not key-repeat within
/// `debounce`), updating `last_press`; false if suppressed.
fn debounced(last_press: &mut Option<Instant>, debounce: Duration) -> bool {
    let now = Instant::now();
    if let Some(t) = *last_press
        && now.duration_since(t) < debounce
    {
        tracing::debug!("press debounced");
        false
    } else {
        *last_press = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, mpsc};

    use super::*;

    struct MockPipeline {
        effects: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Pipeline for MockPipeline {
        fn begin_session(&mut self, _mode: RecognitionMode) -> Result<()> {
            self.effects.lock().unwrap().push("begin");
            Ok(())
        }
        fn finalize_session(&self) {
            self.effects.lock().unwrap().push("finalize");
        }
        fn cancel_session(&mut self) {
            self.effects.lock().unwrap().push("cancel");
        }
        fn mic_preview(&mut self, _on: bool) {}
    }

    fn harness_debounce(cmds: Vec<Command>, debounce: Duration) -> Vec<&'static str> {
        let (tx, rx) = mpsc::channel::<Command>();
        let effects = Arc::new(Mutex::new(Vec::new()));
        let p = MockPipeline {
            effects: effects.clone(),
        };
        let handle = std::thread::spawn(move || run_with_debounce(rx, p, debounce));
        for c in cmds {
            tx.send(c).unwrap();
        }
        drop(tx);
        handle.join().unwrap();
        effects.lock().unwrap().clone()
    }

    fn harness(cmds: Vec<Command>) -> Vec<&'static str> {
        harness_debounce(cmds, DEBOUNCE)
    }

    #[test]
    fn press_release_yields_begin_then_finalize() {
        let e = harness(vec![
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::PushToTalk,
            },
            Command::Input {
                is_pressed: false,
                mode: RecognitionMode::PushToTalk,
            },
            Command::ProcessingFinished,
        ]);
        assert!(e.starts_with(&["begin"]));
        assert!(e.contains(&"finalize"));
    }

    #[test]
    fn cancel_mid_record_resets() {
        let e = harness(vec![
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::PushToTalk,
            },
            Command::Cancel,
        ]);
        assert!(e.contains(&"cancel"));
    }

    #[test]
    fn release_triggers_finalize() {
        let e = harness(vec![
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::PushToTalk,
            },
            Command::Input {
                is_pressed: false,
                mode: RecognitionMode::PushToTalk,
            },
            Command::ProcessingFinished,
        ]);
        assert!(e.contains(&"finalize"));
    }

    // ── Command mode (DECISION 5: Command = PTT release semantics) ──

    #[test]
    fn command_press_starts_recording() {
        let e = harness(vec![Command::Input {
            is_pressed: true,
            mode: RecognitionMode::Command,
        }]);
        assert!(e.starts_with(&["begin"]));
    }

    #[test]
    fn command_release_finalizes() {
        // Command mode uses the main hotkey press-release (PTT semantics), NOT
        // toggle tap. Release in Recording must finalize — mirrors
        // release_triggers_finalize for the Command variant.
        let e = harness(vec![
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::Command,
            },
            Command::Input {
                is_pressed: false,
                mode: RecognitionMode::Command,
            },
            Command::ProcessingFinished,
        ]);
        assert!(
            e.contains(&"finalize"),
            "Command release must finalize: {e:?}"
        );
    }

    // ── Toggle mode (spec §9) ──

    #[test]
    fn toggle_tap_starts_recording() {
        let e = harness(vec![Command::Input {
            is_pressed: true,
            mode: RecognitionMode::Toggle,
        }]);
        assert!(e.starts_with(&["begin"]));
    }

    #[test]
    fn toggle_tap_again_finalizes() {
        // Duration::ZERO test seam: the debounce gate never suppresses, so a
        // back-to-back second Toggle tap finalizes — no wall-clock sleep.
        let e = harness_debounce(
            vec![
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
            ],
            Duration::ZERO,
        );
        assert!(e.starts_with(&["begin"]));
        assert!(e.contains(&"finalize"));
    }

    #[test]
    fn toggle_rapid_tap_is_debounced() {
        // Back-to-back Toggle taps within the debounce window: the second is
        // suppressed (key-repeat guard, spec §9). A 60s window makes this
        // deterministic regardless of scheduling jitter.
        let e = harness_debounce(
            vec![
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
            ],
            Duration::from_secs(60),
        );
        assert!(e.starts_with(&["begin"]));
        assert!(
            !e.contains(&"finalize"),
            "rapid second tap must be debounced: {e:?}"
        );
    }

    #[test]
    fn toggle_release_is_noop() {
        let e = harness(vec![
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::Toggle,
            },
            Command::Input {
                is_pressed: false,
                mode: RecognitionMode::Toggle,
            },
        ]);
        assert!(e.starts_with(&["begin"]));
        assert!(
            !e.contains(&"finalize"),
            "Toggle release must not finalize: {e:?}"
        );
    }

    #[test]
    fn toggle_cancel_aborts() {
        let e = harness(vec![
            Command::Input {
                is_pressed: true,
                mode: RecognitionMode::Toggle,
            },
            Command::Cancel,
        ]);
        assert!(e.contains(&"cancel"));
    }

    // ── Auto-stop on trailing silence (toggle mode only) ──

    #[test]
    fn autostop_in_recording_finalizes() {
        // Engine-driven AutoStop arrives while Recording (Toggle) → the
        // pipeline sees finalize, stage advances to Processing. Zero debounce
        // seam keeps the second tap deterministic (not needed here but mirrors
        // the pattern).
        let e = harness_debounce(
            vec![
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
                Command::AutoStop,
            ],
            Duration::ZERO,
        );
        assert!(e.starts_with(&["begin"]));
        assert!(e.contains(&"finalize"), "AutoStop must finalize: {e:?}");
    }

    #[test]
    fn autostop_in_idle_is_noop() {
        // AutoStop with no active session is dropped (worker fired late, or a
        // stray) — must NOT finalize or begin.
        let e = harness(vec![Command::AutoStop]);
        assert!(
            !e.contains(&"finalize") && !e.contains(&"begin"),
            "AutoStop in Idle must be a no-op: {e:?}"
        );
    }

    #[test]
    fn autostop_in_processing_is_noop() {
        // Race: user toggle-tapped to finalize (Recording → Processing) just as
        // the worker emitted AutoStop. The stage guard drops the late AutoStop
        // — no double finalize, no begin. Duration::ZERO makes the second tap
        // finalize deterministically (default 30ms debounce would suppress it).
        let e = harness_debounce(
            vec![
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
                Command::Input {
                    is_pressed: true,
                    mode: RecognitionMode::Toggle,
                },
                Command::AutoStop,
            ],
            Duration::ZERO,
        );
        let finalize_count = e.iter().filter(|&&x| x == "finalize").count();
        assert_eq!(
            finalize_count, 1,
            "AutoStop in Processing must not double-finalize: {e:?}"
        );
    }
}
