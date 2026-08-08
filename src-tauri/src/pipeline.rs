//! Production `coordinator::Pipeline`: drives audio capture, the engine worker,
//! the overlay, and paste. Owned SOLELY by the coordinator thread (single
//! mutator), so it holds `AudioCapture` (Send !Sync) directly — no `Arc<Mutex>`
//! needed, unlike the brief's sketch.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::audio::AudioCapture;
use crate::coordinator::{Command, Pipeline};
use crate::dictionary::Dictionary;
use crate::engine::{AutoStopConfig, EngineCmd, EngineHandle};
use crate::errors::Result;
use crate::settings::{PostMode, ProfileEntry, RecognitionMode, Settings};
use crate::{overlay, paste};

pub struct AppPipeline {
    app: AppHandle,
    audio: AudioCapture,
    engine: EngineHandle,
    target: Option<isize>,
    settings: Settings,
    // Resolved per-app profile at `begin_session` (PTT/toggle press); applied
    // as a `post_mode` + `prompt` override into the finalize closure for that
    // one session. `None` (default/empty-profiles/no-match/fail-open) → global
    // settings stand. Reassigned on every begin_session, so no cancel reset.
    session_profile: Option<ProfileEntry>,
    cmd_tx: mpsc::Sender<Command>,
    // Session generation counter. `begin_session` and `cancel_session` both bump
    // it; `finalize_session`'s side thread captures the generation at spawn and
    // compares before paste — a mismatch means the session was superseded
    // (Cancel during Processing, or a new session began before the old finalize
    // inference returned) and the paste + ProcessingFinished signal are skipped
    // (spec §6.5: Cancel during Processing aborts, no paste). Strictly stronger
    // than a bool flag: also covers the "cancel then immediately re-press" race
    // where a stale finalize would otherwise paste into the new target.
    generation: Arc<AtomicU32>,
    dictionary: Arc<Mutex<Dictionary>>,
    /// Snippet store forwarded to the finalize side-thread for the Smart-step
    /// `expand()` (Task 8b). Same shape as `dictionary` — see AppState doc.
    snippets: Arc<Mutex<crate::snippets::Snippets>>,
    // Capture run-state single source of truth. `session_active` is AtomicBool
    // because `finalize_session` is `&self` (interior mutability); `preview` is
    // plain bool (only ever set via `mic_preview(&mut self)`). Capture runs iff
    // `session_active || preview` — see `sync_capture`. Privacy §10.1: these
    // drive only the local level meter, never recording/transmitting audio.
    session_active: AtomicBool,
    preview: bool,
}

impl AppPipeline {
    // ponytail: 7 params — one positional arg per owned dependency; mirrors the
    // established AppPipeline::new shape. A bag struct for these is unrequested
    // abstraction.
    pub fn new(
        app: AppHandle,
        audio: AudioCapture,
        engine: EngineHandle,
        settings: Settings,
        cmd_tx: mpsc::Sender<Command>,
        dictionary: Arc<Mutex<Dictionary>>,
        snippets: Arc<Mutex<crate::snippets::Snippets>>,
    ) -> Self {
        Self {
            app,
            audio,
            engine,
            target: None,
            settings,
            session_profile: None,
            cmd_tx,
            generation: Arc::new(AtomicU32::new(0)),
            dictionary,
            snippets,
            session_active: AtomicBool::new(false),
            preview: false,
        }
    }

    /// Single source of truth for capture run-state: capture runs iff a
    /// recording session is active OR mic-preview is on. cpal `play()`/`pause()`
    /// are idempotent (verified ctx7 /rustaudio/cpal 0.18) and errors are
    /// already swallowed by `AudioCapture`, so redundant calls are free.
    fn sync_capture(&self) {
        if self.session_active.load(Ordering::Relaxed) || self.preview {
            self.audio.resume();
        } else {
            self.audio.pause();
        }
    }
}

impl Pipeline for AppPipeline {
    fn begin_session(&mut self, mode: RecognitionMode) -> Result<()> {
        // Bump generation: any still-running finalize side thread from a prior
        // session will see the mismatch and skip its paste.
        self.generation.fetch_add(1, Ordering::SeqCst);
        // Drop any live Polished edit-window sender: a waiting finalize thread
        // (grace or Pause) sees Disconnect → Skip, preventing a stale paste
        // after a supersede (the generation guard already passed for it).
        *self
            .app
            .state::<crate::AppState>()
            .pending_paste
            .lock()
            .unwrap() = None;
        // Capture the paste target BEFORE the overlay could ever steal focus.
        self.target = paste::capture_target();
        // Resolve the foreground app's per-app profile. Gated on non-empty
        // profiles so the DEFAULT path (empty vec) pays zero Win32 cost.
        // Fail-open: no fg window, OpenProcess denied (elevated target), or no
        // match → None → global post-proc stands. Privacy §10.1: log exe
        // basename + post_mode label ONLY (both metadata); NEVER the prompt.
        self.session_profile = if self.settings.profiles.is_empty() {
            None
        } else {
            match crate::profiles::foreground_exe() {
                Ok(exe) => match crate::profiles::resolve(&self.settings.profiles, &exe) {
                    Some(p) => {
                        tracing::info!(
                            "profile resolved: {exe} → {}",
                            crate::pipeline::post_mode_label(&p.post_mode)
                        );
                        Some(p.clone())
                    }
                    None => {
                        tracing::debug!("no profile for foreground exe {exe}");
                        None
                    }
                },
                Err(e) => {
                    // OpenProcess denies elevated windows routinely; debug, not warn.
                    tracing::debug!("profile resolve skipped: {e}");
                    None
                }
            }
        };
        self.session_active.store(true, Ordering::Relaxed);
        self.sync_capture();
        if self.settings.overlay.sounds.enabled {
            crate::audio::play_configured(
                &self.settings.overlay.sounds.start,
                crate::audio::Tone::Start,
            );
        }
        let app = self.app.clone();
        let on_partial: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |text| {
            let _ = overlay::emit_text(&app, text);
        });
        // Auto-stop arms only in Toggle mode with the endpoint enabled. PTT
        // and disabled-endpoint sessions pass None -> the worker builds no
        // SilenceTracker, zero cost on the default RU/PTT path.
        let auto_stop = if mode == RecognitionMode::Toggle && self.settings.endpoint.enabled {
            Some(AutoStopConfig {
                trailing_silence: Duration::from_millis(
                    self.settings.endpoint.trailing_silence_ms as u64,
                ),
                energy_threshold: self.settings.vad.energy_threshold as f64,
            })
        } else {
            None
        };
        let _ = self.engine.tx.send(EngineCmd::Start {
            on_partial,
            auto_stop,
        });
        // Task 10 D6: during onboarding practice the overlay stays hidden —
        // onboarding is foreground and shows its own caption via `stream-text`
        // (the on_partial emit above is independent of overlay show). Default
        // path (practice=false) is unchanged.
        if !self
            .app
            .state::<crate::AppState>()
            .onboarding_practice
            .load(Ordering::Relaxed)
        {
            overlay::show(&self.app, "recording")?;
            overlay::emit_phase(&self.app, "listening", "transcribing")?;
        }
        crate::tray::set_recording(&self.app, true);
        Ok(())
    }

    fn finalize_session(&self) {
        self.session_active.store(false, Ordering::Relaxed);
        self.sync_capture();
        crate::tray::set_recording(&self.app, false);
        if self.settings.overlay.sounds.enabled {
            crate::audio::play_configured(
                &self.settings.overlay.sounds.stop,
                crate::audio::Tone::Stop,
            );
        }
        let (tx, rx) = mpsc::channel::<(String, Option<String>)>();
        let _ = self.engine.tx.send(EngineCmd::Finalize { reply: tx });
        let _ = overlay::emit_phase(&self.app, "working", "transcribing");
        // Deliver on a side thread so the coordinator loop isn't blocked on
        // the (fast but synchronous) finalize inference + paste.
        let app = self.app.clone();
        let cmd_tx = self.cmd_tx.clone();
        let target = self.target;
        let mode = self.settings.paste_mode;
        let generation = self.generation.clone();
        let captured_gen = self.generation.load(Ordering::SeqCst);
        let dictionary = self.dictionary.clone();
        let snippets = self.snippets.clone();
        let snippets_enabled = self.settings.snippets_enabled;
        let backtrack = self.settings.backtrack_parsing;
        // Read history LIVE from AppState (not a construction-time clone) so a
        // mid-session enable/disable in `ipc::set_settings` takes effect on the
        // NEXT finalize. Privacy §10.1: the Arc is dropped here the instant
        // AppState sets it to None.
        let history = self
            .app
            .state::<crate::AppState>()
            .history
            .lock()
            .unwrap()
            .clone();
        // Apply the resolved per-app profile's post_mode + prompt override for
        // this session (None → noop → global post-proc stands). Applied BEFORE
        // the move so the side thread sees the effective config. Pure helper;
        // prompt is user content and never logged (privacy §10.1).
        let mut post = self.settings.post_processing.clone();
        crate::profiles::apply_profile_override(&mut post, self.session_profile.as_ref());
        let configured = self.settings.language.clone();
        let engine = self.settings.model.clone();
        let recognition_mode = self.settings.recognition_mode;
        std::thread::spawn(move || {
            let (text, detected) = rx.recv().unwrap_or_default();
            // Cancel-during-Processing / session-superseded guard. Early return
            // skips paste AND post-proc AND history — the history skip is the
            // deferred Phase-1 polish item (spec §6.5). Structural guarantee:
            // run_finalize is never called when the guard fires. Covered by the
            // coordinator cancel_* tests (state machine) + Task 19's end-to-end
            // log/privacy test.
            if generation.load(Ordering::SeqCst) != captured_gen {
                tracing::info!("finalize: session superseded, skipping paste + history");
                let _ = overlay::hide(&app);
                return;
            }
            // Command mode (DECISION 6): parse the transcript before the normal
            // paste/post-proc/history path. A match emits the chord and skips
            // paste + history (an "undo" must NOT be recorded as transcript
            // text). A no-match falls through to normal paste (graceful). The
            // OS-dependent chord emission lives in paste.rs (not unit-testable
            // without OS); the pure `parse()` is covered in commands.rs.
            if recognition_mode == RecognitionMode::Command
                && let Some(chord) = crate::commands::parse(&text)
            {
                match crate::paste::run_command_chord(&chord, target) {
                    Ok(()) => {
                        // Privacy §10.1: fixed content-free string — never
                        // the phrase, action, or VK.
                        tracing::info!("command-mode: chord delivered");
                        let _ = overlay::hide(&app);
                        let _ = cmd_tx.send(Command::ProcessingFinished);
                        return;
                    }
                    Err(e) => {
                        // Reuse the paste-failure surface but still skip
                        // paste + history — pasting the literal command
                        // phrase on chord failure is wrong UX. `e` is
                        // metadata-only (focus/enigo error strings).
                        tracing::warn!("command-mode: chord delivery failed: {e}");
                        let _ = overlay::show_paste_failed(&app);
                        let app_hide = app.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(2000));
                            let _ = overlay::hide(&app_hide);
                        });
                        let _ = cmd_tx.send(Command::ProcessingFinished);
                        return;
                    }
                }
            }
            // no match (or not Command mode) → fall through to the normal paste path
            // Polishing phase: overlay STAYS VISIBLE through post-proc + paste
            // (Task 8 hid here, before run_finalize — so a polishing emit would
            // have targeted a hidden window). Ruling 2 moves the hide to after
            // run_finalize. Privacy: emit carries only metadata strings.
            let _ = overlay::emit_phase(&app, "polishing", "polishing");
            // Detected lang wins (Nemotron auto-mode emits a `<xx-XX>` tag);
            // else fall back to the configured lang for fixed-lang / GigaAM /
            // no-tag cases. `lang` is metadata (a locale code) — never text.
            // `auto` is the recognition MODE, not a language: recording it would
            // pollute the history lang column + the history lang filter with a
            // non-language value. Record a lang only when we actually know it
            // (detected, or a forced locale); `auto`/empty → None (honest
            // "unknown": excluded from distinct_langs, hidden in row meta).
            let lang = detected.or_else(|| match configured.as_str() {
                "" | "auto" => None,
                other => Some(other.to_string()),
            });
            // ponytail: Dictionary + Snippets mutexes held only for post-proc
            // (narrowed from the pre-refactor full-run_finalize hold). Smart is
            // ms; Polished never touches either; only cost is blocking rare IPC
            // CRUD during one finalize. Lock order is dictionary -> snippets, and
            // ONLY this site holds both — IPC CRUD locks each independently, so
            // no reverse-order path can deadlock.
            let final_text = {
                let dict_guard = dictionary.lock().unwrap();
                let snip_guard = snippets.lock().unwrap();
                // `snippets_enabled` collapses into whether the Option is Some
                // (the store is always constructed in run(); the flag is the
                // sole real gate). Downstream carries only the Option.
                let snip_ref = if snippets_enabled {
                    Some(&*snip_guard)
                } else {
                    None
                };
                postproc_final_text(&text, &post, Some(&*dict_guard), snip_ref, backtrack)
            };
            // Task 10 D6: onboarding-practice branch. Between post-proc and
            // paste — emit the result to the onboarding window and skip paste +
            // history (practice is not a real dictation). ONE AtomicBool load
            // (~1ns); default `false` is byte-for-byte today's behavior (blaze).
            // Privacy §10.1: log metadata-only ("onboarding practice: result
            // emitted"); the text crosses the IPC bus only (same rule as
            // overlay::emit_text). `stream-text` partials already flowed to the
            // onboarding caption during the session — this is the final result.
            if app
                .state::<crate::AppState>()
                .onboarding_practice
                .load(Ordering::Relaxed)
            {
                emit_practice_result(&final_text, |t| {
                    let _ = app.emit_to(
                        "onboarding",
                        "practice-result",
                        serde_json::json!({ "text": t }),
                    );
                });
                // Still advance the coordinator (Processing -> Idle) and hide
                // any incidental overlay state (a no-op if never shown).
                let _ = overlay::hide(&app);
                let _ = cmd_tx.send(Command::ProcessingFinished);
                return;
            }
            let post_mode_str = post_mode_label(&post.mode);
            // Decision A: the inline edit-window engages ONLY when the effective
            // post-proc mode is Polished (`post.mode == Polished` AFTER
            // apply_profile_override, so a profile-promoted Polished session is
            // included). Smart/Raw are entirely unchanged — instant paste, no
            // edit UI, zero regression to the default RU/PTT/Smart blaze path.
            // Polished is the only mode with a real polishing window (the LLM
            // call) and the only mode where review-before-paste makes UX sense.
            // The gate is the existing `post.mode`; no new settings field.
            //
            // `paste_attempted` = the text actually sent to paste (may differ
            // from final_text when the user edits inline). Tracked so the
            // paste-failed path can retain it for "Paste anyway" recovery.
            // None = Skip (user cancelled / superseded → no paste at all).
            let paste_outcome: Option<(Result<()>, String)> = if post.mode == PostMode::Polished {
                // Build a one-shot channel + store the sender in
                // AppState::pending_paste (mirrors cmd_tx exactly — Decision C).
                // Dropping a prior sender → prior thread's resolve_edit sees
                // Disconnect → Skip; the generation guard is the backstop.
                // Privacy §10.1: emit_edit_ready carries text over IPC only,
                // never logged (same rule as emit_text).
                let (edit_tx, edit_rx) = mpsc::channel::<EditDecision>();
                *app.state::<crate::AppState>().pending_paste.lock().unwrap() = Some(edit_tx);
                let _ = overlay::emit_edit_ready(&app, &final_text);
                let outcome =
                    resolve_edit(edit_rx, Duration::from_millis(EDIT_GRACE_MS), &final_text);
                // Clear pending_paste: late IPC commands (request_edit/
                // confirm_paste/cancel_paste) become no-ops once the window
                // has resolved.
                *app.state::<crate::AppState>().pending_paste.lock().unwrap() = None;
                match outcome {
                    EditOutcome::Paste(edited) => {
                        let r = paste_and_record(
                            &edited,
                            history.as_ref().map(|h| h.as_ref()),
                            lang.as_deref(),
                            Some(&engine),
                            post_mode_str,
                            |t| paste::paste_text(t, target, mode),
                        );
                        Some((r, edited))
                    }
                    EditOutcome::Skip => {
                        let _ = overlay::hide(&app);
                        None
                    }
                }
            } else {
                let r = paste_and_record(
                    &final_text,
                    history.as_ref().map(|h| h.as_ref()),
                    lang.as_deref(),
                    Some(&engine),
                    post_mode_str,
                    |t| paste::paste_text(t, target, mode),
                );
                Some((r, final_text.clone()))
            };
            if let Some((res, attempted_text)) = paste_outcome {
                match res {
                    Ok(()) => {
                        // Decision D: teal check (400 ms) then hide. Mirrors the
                        // existing paste-failed delayed-hide pattern. Command-
                        // mode chord success keeps its immediate hide (above).
                        let _ = overlay::emit_phase(&app, "success", "success");
                        let app_hide = app.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(400));
                            let _ = overlay::hide(&app_hide);
                        });
                    }
                    Err(e) => {
                        tracing::warn!("paste failed: {e}");
                        // Retain the failed text for "Paste anyway" recovery
                        // (privacy: in-memory only, never logged; cleared after
                        // the 2s recovery window by the delayed-hide thread).
                        *app.state::<crate::AppState>()
                            .last_failed_text
                            .lock()
                            .unwrap() = Some(attempted_text);
                        let _ = overlay::show_paste_failed(&app);
                        let app_hide = app.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(2000));
                            *app_hide
                                .state::<crate::AppState>()
                                .last_failed_text
                                .lock()
                                .unwrap() = None;
                            let _ = overlay::hide(&app_hide);
                        });
                    }
                }
            }
            // Advance the coordinator: Processing -> Idle.
            let _ = cmd_tx.send(Command::ProcessingFinished);
        });
    }

    fn cancel_session(&mut self) {
        // Bump generation: the finalize side thread (if running) will see the
        // mismatch and skip its paste. Covers Cancel-during-Processing: the
        // worker's session is already taken by the first Finalize, so the
        // discard-Finalize below is a no-op at the engine; this generation
        // bump is what actually prevents the stale paste.
        self.generation.fetch_add(1, Ordering::SeqCst);
        // Drop a live Polished edit-window sender (same rationale as
        // begin_session): a waiting thread sees Disconnect → Skip.
        *self
            .app
            .state::<crate::AppState>()
            .pending_paste
            .lock()
            .unwrap() = None;
        self.session_active.store(false, Ordering::Relaxed);
        self.sync_capture();
        // Discard-reply Finalize: the worker still flushes + resets the chunker
        // (next session starts clean), and the reply lands on a dropped rx.
        // For Cancel-during-Recording this is the load-bearing call (session is
        // still active → real flush + reset); for Cancel-during-Processing it
        // hits the worker's `None` branch (no-op, logged).
        let (discard_tx, _discard_rx) = mpsc::channel::<(String, Option<String>)>();
        let _ = self
            .engine
            .tx
            .send(EngineCmd::Finalize { reply: discard_tx });
        let _ = overlay::hide(&self.app);
        crate::tray::set_recording(&self.app, false);
    }

    fn mic_preview(&mut self, on: bool) {
        self.preview = on;
        self.sync_capture();
    }
}

/// History metadata: stable snake_case label for the active post-proc mode
/// (matches `PostMode`'s `#[serde(rename_all = "snake_case")]`).
fn post_mode_label(mode: &crate::settings::PostMode) -> &'static str {
    match mode {
        crate::settings::PostMode::Raw => "raw",
        crate::settings::PostMode::Smart => "smart",
        crate::settings::PostMode::Polished => "polished",
    }
}

/// Post-proc → final text. Extracted from the pre-refactor `run_finalize` body
/// so the Polished edit-window can run between post-proc and paste. Pure logic:
/// Used → the post-processed text; Failed(err, raw) → metadata-only warn + the
/// raw fallback (transcript is never lost). Privacy §10.1: `err` is metadata.
fn postproc_final_text(
    text: &str,
    post: &crate::settings::PostProcessing,
    dict: Option<&crate::dictionary::Dictionary>,
    snippets: Option<&crate::snippets::Snippets>,
    backtrack: bool,
) -> String {
    let outcome = crate::postproc::run(text, post, dict, snippets, backtrack);
    match outcome {
        crate::postproc::PostOutcome::Used(t) => t,
        // ponytail: no overlay toast on post-proc failure (overlay::emit_toast
        // doesn't exist; adding one + frontend wiring is scope creep). Raw is
        // pasted — the transcript is never lost.
        crate::postproc::PostOutcome::Failed(err, raw) => {
            tracing::warn!("post-proc failed: {err} (paste raw)");
            raw
        }
    }
}

/// Onboarding-practice branch (Task 10 D6). Deliver the post-processed text to
/// the onboarding window via the `practice-result` event (the caller injects
/// the emit fn so this is unit-testable without an AppHandle). Privacy §10.1:
/// log metadata-only — the fixed string `onboarding practice: result emitted`,
/// NEVER the text. The text crosses the IPC bus only (same rule as
/// `overlay::emit_text`). Pure helper so the privacy test can exercise the leak
/// surface directly.
pub fn emit_practice_result<F: FnOnce(&str)>(text: &str, emit: F) {
    emit(text);
    tracing::info!("onboarding practice: result emitted");
}

/// Paste + (on paste success) history insert. Returns the paste result so the
/// caller can branch on Ok/Err for the overlay phase + paste-failed recovery.
/// `paste` is injected so this is unit-testable without an AppHandle.
/// Privacy §10.1: `final_text` never crosses tracing; errors are metadata-only.
fn paste_and_record(
    final_text: &str,
    history: Option<&crate::history::History>,
    lang: Option<&str>,
    engine: Option<&str>,
    post_mode: &str,
    paste: impl Fn(&str) -> Result<()>,
) -> Result<()> {
    let r = paste(final_text);
    if r.is_ok()
        && let Some(h) = history
    {
        // ponytail: record only on successful paste — don't persist a transcript
        // the user never received. Errors are metadata-only.
        if let Err(e) = h.insert(final_text, lang, engine, Some(post_mode)) {
            tracing::warn!("history insert failed: {e}");
        }
    }
    r
}

// ── Polished edit-window (Task 9, Decisions A–D) ──

/// IPC-driven decision carried via `AppState::pending_paste` (a
/// `Mutex<Option<Sender>>`, mirroring `cmd_tx` — Decision C). The frontend
/// `invoke`s `request_edit` / `confirm_paste` / `cancel_paste`, which send on
/// the live sender.
#[derive(Debug)]
pub enum EditDecision {
    /// User clicked Edit: pause the grace auto-paste, wait indefinitely for
    /// confirm/cancel.
    Pause,
    /// User confirmed (Enter). None = paste the original post-proc text.
    Confirm(Option<String>),
    /// User cancelled (Esc). Skip paste entirely.
    Cancel,
}

/// Outcome of the edit-window resolution (Task 9 Step 3).
#[derive(Debug, PartialEq, Eq)]
pub enum EditOutcome {
    /// Paste this text (may be the original or the user's edited version).
    Paste(String),
    /// Skip paste (user cancelled / session superseded / channel disconnected).
    Skip,
}

/// Grace auto-paste window before the original text is pasted (no data loss).
/// ponytail: a const, not a setting — avoids settings.rs / R4 / UI toggle churn.
/// Long enough to click Edit; short enough not to burden the no-edit case.
const EDIT_GRACE_MS: u64 = 1500;

/// Resolve the Polished edit-window. PURE + unit-testable (takes the rx, a
/// grace duration, and the original text; no AppHandle). Privacy §10.1: no
/// `tracing::` — the edited text never crosses logs.
///
/// - `Ok(Pause)` ⇒ switch to indefinite `recv()`: `Confirm(t)`→Paste(t),
///   `Cancel`/Disconnect→Skip.
/// - `Ok(Confirm(t))` (direct, within grace) ⇒ Paste(t or original if None).
/// - `Ok(Cancel)` ⇒ Skip.
/// - `Err(Timeout)` ⇒ Paste(original) — auto-paste, no data loss.
/// - `Err(Disconnected)` ⇒ Skip (superseded/cancelled).
fn resolve_edit(rx: mpsc::Receiver<EditDecision>, grace: Duration, original: &str) -> EditOutcome {
    match rx.recv_timeout(grace) {
        Ok(EditDecision::Pause) => match rx.recv() {
            Ok(EditDecision::Confirm(t)) => {
                EditOutcome::Paste(t.unwrap_or_else(|| original.to_string()))
            }
            _ => EditOutcome::Skip, // Cancel or Disconnect
        },
        Ok(EditDecision::Confirm(t)) => {
            EditOutcome::Paste(t.unwrap_or_else(|| original.to_string()))
        }
        Ok(EditDecision::Cancel) => EditOutcome::Skip,
        Err(mpsc::RecvTimeoutError::Timeout) => EditOutcome::Paste(original.to_string()),
        Err(mpsc::RecvTimeoutError::Disconnected) => EditOutcome::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MolviError;
    use crate::history::History;
    use crate::settings::{PostMode, PostProcessing};

    #[test]
    fn post_mode_label_matches_serde_snake_case() {
        assert_eq!(post_mode_label(&PostMode::Raw), "raw");
        assert_eq!(post_mode_label(&PostMode::Smart), "smart");
        assert_eq!(post_mode_label(&PostMode::Polished), "polished");
    }

    #[test]
    fn paste_ok_records_history_with_postproc_text() {
        let h = History::open_in_memory().unwrap();
        let post = PostProcessing::default(); // Smart
        let input = "привет.   как  дела...";
        // Smart transforms (case + ws + repeated marks) -> differs from input.
        let final_text = postproc_final_text(input, &post, None, None, false);
        let res = paste_and_record(
            &final_text,
            Some(&h),
            Some("ru"),
            Some("gigaam"),
            "smart",
            |_| Ok(()),
        );
        assert!(res.is_ok());
        let rows = h.query(None, None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_ne!(rows[0].text, input, "should record post-processed text");
        assert_eq!(rows[0].lang.as_deref(), Some("ru"));
        assert_eq!(rows[0].engine.as_deref(), Some("gigaam"));
        assert_eq!(rows[0].post_mode.as_deref(), Some("smart"));
    }

    #[test]
    fn paste_failure_records_no_history() {
        let h = History::open_in_memory().unwrap();
        let post = PostProcessing::default();
        let final_text = postproc_final_text("тест", &post, None, None, false);
        let res = paste_and_record(&final_text, Some(&h), None, None, "smart", |_| {
            Err(MolviError::Paste("boom".into()))
        });
        assert!(res.is_err());
        assert_eq!(h.query(None, None, None, 100, 0).unwrap().len(), 0);
    }

    #[test]
    fn no_history_when_disabled() {
        let post = PostProcessing::default();
        let final_text = postproc_final_text("тест", &post, None, None, false);
        let res = paste_and_record(&final_text, None, None, None, "smart", |_| Ok(()));
        assert!(res.is_ok());
    }

    #[test]
    fn postproc_failure_falls_back_to_raw_and_records() {
        let h = History::open_in_memory().unwrap();
        // Polished with no endpoint -> polished() fails -> raw fallback.
        let post = PostProcessing {
            mode: PostMode::Polished,
            ..PostProcessing::default()
        };
        // Messy input that Smart WOULD transform (extra ws, ellipsis, all-caps)
        // so the recorded text proves the TRUE raw fallback, not a Smart-cleaned
        // intermediate.
        let raw = "  ПРИВЕТ...  мир  ";
        let pasted = std::cell::RefCell::new(String::new());
        let final_text = postproc_final_text(raw, &post, None, None, false);
        let res = paste_and_record(&final_text, Some(&h), None, None, "polished", |t| {
            *pasted.borrow_mut() = t.to_string();
            Ok(())
        });
        assert!(res.is_ok());
        assert_eq!(pasted.borrow().as_str(), raw, "raw fallback must be pasted");
        let rows = h.query(None, None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, raw, "raw must be recorded on fallback");
    }

    // ── postproc_final_text parity (Task 9 Step 6) ──
    // Verifies the refactored helper matches the old run_finalize's outcome→text
    // logic (Used→t, Failed→raw) across all three PostModes.

    #[test]
    fn postproc_final_text_raw_passthrough() {
        let post = PostProcessing {
            mode: PostMode::Raw,
            ..PostProcessing::default()
        };
        // Raw must pass through unchanged — no Smart steps, no polished call.
        let input = "  ПРИВЕТ...  мир  ";
        assert_eq!(postproc_final_text(input, &post, None, None, false), input);
    }

    #[test]
    fn postproc_final_text_smart_transforms() {
        let post = PostProcessing::default(); // Smart
        let input = "привет.   как  дела...";
        let out = postproc_final_text(input, &post, None, None, false);
        assert_ne!(out, input, "Smart should transform the input");
    }

    #[test]
    fn postproc_final_text_polished_falls_back_to_raw() {
        // Polished with no endpoint -> polished() fails -> raw fallback.
        // The messy input proves it's the TRUE raw, not a Smart intermediate.
        let post = PostProcessing {
            mode: PostMode::Polished,
            ..PostProcessing::default()
        };
        let raw = "  ПРИВЕТ...  мир  ";
        assert_eq!(postproc_final_text(raw, &post, None, None, false), raw);
    }

    // ── resolve_edit (Task 9 Step 6) ──
    // Pure-fn coverage of all branches: grace-timeout, Pause→Confirm,
    // Pause→Cancel, Disconnect, direct Confirm(None), direct Cancel.
    // Uses a 50ms grace so the timeout arm is deterministic + fast.

    #[test]
    fn resolve_edit_grace_timeout_pastes_original() {
        let (_tx, rx) = mpsc::channel::<EditDecision>();
        // No sender activity -> grace times out -> Paste(original).
        let out = resolve_edit(rx, Duration::from_millis(50), "оригинал");
        assert_eq!(out, EditOutcome::Paste("оригинал".to_string()));
    }

    #[test]
    fn resolve_edit_pause_then_confirm_pastes_edited() {
        let (tx, rx) = mpsc::channel::<EditDecision>();
        tx.send(EditDecision::Pause).unwrap();
        tx.send(EditDecision::Confirm(Some("отредактировано".into())))
            .unwrap();
        let out = resolve_edit(rx, Duration::from_millis(50), "оригинал");
        assert_eq!(out, EditOutcome::Paste("отредактировано".to_string()));
    }

    #[test]
    fn resolve_edit_pause_then_cancel_skips() {
        let (tx, rx) = mpsc::channel::<EditDecision>();
        tx.send(EditDecision::Pause).unwrap();
        tx.send(EditDecision::Cancel).unwrap();
        let out = resolve_edit(rx, Duration::from_millis(50), "оригинал");
        assert_eq!(out, EditOutcome::Skip);
    }

    #[test]
    fn resolve_edit_disconnect_skips() {
        let (tx, rx) = mpsc::channel::<EditDecision>();
        drop(tx); // disconnect -> resolve_edit sees Disconnected -> Skip.
        let out = resolve_edit(rx, Duration::from_millis(50), "оригинал");
        assert_eq!(out, EditOutcome::Skip);
    }

    #[test]
    fn resolve_edit_direct_confirm_none_pastes_original() {
        let (tx, rx) = mpsc::channel::<EditDecision>();
        tx.send(EditDecision::Confirm(None)).unwrap();
        let out = resolve_edit(rx, Duration::from_millis(50), "оригинал");
        assert_eq!(out, EditOutcome::Paste("оригинал".to_string()));
    }

    #[test]
    fn resolve_edit_direct_cancel_skips() {
        let (tx, rx) = mpsc::channel::<EditDecision>();
        tx.send(EditDecision::Cancel).unwrap();
        let out = resolve_edit(rx, Duration::from_millis(50), "оригинал");
        assert_eq!(out, EditOutcome::Skip);
    }
}
