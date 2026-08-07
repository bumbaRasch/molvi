//! molvi library root. Re-exports the Phase-1 modules so integration tests and
//! the thin binary can reach them (`molvi::engine::Engine`, etc.). `run()` is
//! the Tauri entrypoint; `src/main.rs` is a one-line shim.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as AutostartExt;

pub mod audio;
/// Deterministic command-mode grammar → enigo KeyChord (spec §6.2).
pub mod commands;
pub mod coordinator;
pub mod csv_util;
pub mod dictionary;
pub mod engine;
pub mod engine_adapter;
pub mod errors;
pub mod history;
pub mod hotkey;
pub mod ipc;
pub mod log;
pub mod model_store;
pub mod ort_affinity;
pub mod overlay;
pub mod paste;
pub mod paths;
pub mod pipeline;
pub mod postproc;
/// Per-app post-processing profiles (spec §1.6). Foreground-exe resolver +
/// first-enabled match. No DB — profiles live in `settings.json`.
pub mod profiles;
pub mod resample;
pub mod settings;
/// Snippet engine (spec §1.5). Voice-cue → stored-block expansion, persisted
/// in snippets.db. Whole-text equality match (NOT token substitution).
pub mod snippets;
pub mod tray;
pub mod tray_locales;
pub mod updater;
/// X11 EWMH helpers (Linux only): active-window/pid queries + focus restore.
#[cfg(all(unix, not(target_os = "macos")))]
pub mod x11;

pub use engine::Engine;
pub use settings::Settings;

// macOS overlay focus fix (spike #3; tauri#14102). Converts the "overlay"
// webview window into a non-activating NSPanel so show/hide never steals
// keyboard focus from the user's app (paste routes correctly).
// `can_become_key_window: false` is the whole point. Idempotent + best-effort:
// any error logs + skips (the app still runs with Tauri's default
// focus-stealing window — better than a startup crash). Verified against
// tauri-nspanel rev a3122e8.
#[cfg(target_os = "macos")]
mod macos_overlay {
    use tauri::Manager;
    use tauri_nspanel::{CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt, tauri_panel};

    tauri_panel! {
        panel!(OverlayPanel {
            config: {
                // The panel MUST NOT take keyboard focus (paste → user's app).
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true,
                // An accessory app is permanently "deactivated"; the default
                // hide-on-deactivate would vanish the overlay.
                hides_on_deactivate: false,
            }
        })
    }

    /// Convert the "overlay" window to a non-activating NSPanel, ONCE at
    /// startup. Also sets Accessory activation policy (no Dock icon; required
    /// for the non-activating model). Idempotent + best-effort.
    pub fn init_overlay_panel(app: &tauri::AppHandle) {
        // Accessory policy = background/accessory app (no Dock icon). The
        // bundle's LSUIElement (Task 9) is the declarative equivalent for
        // release builds.
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let Some(win) = app.get_webview_window("overlay") else {
            tracing::warn!("macOS overlay panel: window not found");
            return;
        };
        let panel = match win.to_panel::<OverlayPanel>() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("macOS overlay to_panel failed: {e}");
                return;
            }
        };
        // Non-activating style; Status level (above normal windows, over the
        // menu bar region); show alongside fullscreen; join all spaces.
        panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
        panel.set_level(PanelLevel::Status.value());
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .full_screen_auxiliary()
                .can_join_all_spaces()
                .into(),
        );
        tracing::info!("macOS overlay panel initialized (non-activating, Status level)");
    }
}

/// True when another process holds Secure Event Input (password field, Terminal
/// "Secure Keyboard Entry", 1Password, a stuck loginwindow). While held, macOS
/// suppresses Carbon global hotkeys → molvi's PTT stops working with no error
/// (Handy finding, issue #1578). Detection turns a mystifying failure into an
/// explainable one. Off-macOS → always false (the API doesn't exist).
#[cfg(target_os = "macos")]
pub fn secure_input_held() -> bool {
    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        // Carbon HIToolbox; `Boolean` is an unsigned char.
        fn IsSecureEventInputEnabled() -> u8;
    }
    // SAFETY: IsSecureEventInputEnabled is a pure query of process-global
    // HIToolbox state; no handles, no side effects, thread-safe.
    unsafe { IsSecureEventInputEnabled() != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn secure_input_held() -> bool {
    false
}

/// Tauri-managed app state. `settings` is read by future settings-UI commands;
/// `cmd_tx` lets the `cancel_operation` IPC command forward to the coordinator
/// (filled in `setup` once the coordinator channel exists).
pub struct AppState {
    pub settings: Mutex<settings::Settings>,
    pub cmd_tx: Mutex<Option<std::sync::mpsc::Sender<coordinator::Command>>>,
    pub dictionary: Arc<Mutex<dictionary::Dictionary>>,
    /// Snippet store (spec §1.5/§6.3). Mirrors `dictionary`'s
    /// `Arc<Mutex<...>>` shape — `Snippets` is `Send` (Connection: Send),
    /// proven in production by the identical Dictionary pattern. Consumed by
    /// `postproc::smart_pipeline` via `expand()` (Task 8b).
    pub snippets: Arc<Mutex<snippets::Snippets>>,
    pub history: Mutex<Option<Arc<history::History>>>,
    /// On-demand mic-preview flag. Read by the mic-level poller so it emits the
    /// level event while preview is on (even with the overlay hidden). Set by
    /// `ipc::set_mic_preview`, which also forwards `Command::MicPreview` to the
    /// coordinator (which owns the actual capture run-state). Privacy §10.1:
    /// the level is a scalar (RMS×1000), metadata-only.
    pub mic_preview: Arc<AtomicBool>,
    /// Onboarding-practice gate (Task 10 D6). Mirrors `mic_preview` exactly.
    /// While `true`, the finalize side-thread routes the post-processed text to
    /// the onboarding window via `practice-result` (NOT paste) and skips
    /// history; `begin_session` suppresses `overlay::show` so onboarding is the
    /// foreground caption. Read on the finalize hot path as ONE `Relaxed` load
    /// (~1ns); default `false` keeps the default RU/PTT/Smart path byte-for-byte
    /// unchanged (blaze). Privacy §10.1: practice text crosses the IPC bus only.
    pub onboarding_practice: Arc<AtomicBool>,
    /// Polished edit-window one-shot (Task 9, Decision C). Mirrors `cmd_tx`'s
    /// shape exactly: the finalize side-thread stores `Some(tx)` when it enters
    /// the Polished edit-window; the `request_edit`/`confirm_paste`/
    /// `cancel_paste` IPC commands send on it. Dropping/replacing the sender
    /// (new session, resolution) disconnects the waiting thread → Skip. Late
    /// commands after resolution find `None` → no-op.
    pub pending_paste: Mutex<Option<mpsc::Sender<pipeline::EditDecision>>>,
    /// Retained failed-paste text for the "Paste anyway" recovery button
    /// (Task 9 Step 4). Privacy §10.1: in-memory only, NEVER logged; cleared
    /// after the 2s recovery window (delayed-hide thread) or on successful
    /// `paste_anyway`. Set right before `show_paste_failed`.
    pub last_failed_text: Mutex<Option<String>>,
    /// Background model-download handle (Task 14.3). `Some` while a download
    /// spawned by `download_model` is in flight; the guard in `download_model`
    /// rejects a 2nd concurrent download via `inner().is_finished()`. Cancel =
    /// `abort()` (hf-hub content cache resumes completed chunks on retry).
    /// Privacy §10.1: holds no user content.
    pub model_download: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

/// Overlay × button (or any frontend cancel affordance) -> coordinator Cancel.
#[tauri::command]
fn cancel_operation(state: tauri::State<'_, AppState>) {
    let tx = state.cmd_tx.lock().unwrap();
    if let Some(tx) = tx.as_ref() {
        let _ = tx.send(coordinator::Command::Cancel);
        tracing::info!("cancel_operation: sent Cancel");
    } else {
        tracing::warn!("cancel_operation: coordinator channel not ready");
    }
}

// ── Polished edit-window + paste-failed recovery (Task 9) ──
// Mirror `cancel_operation`'s IPC→State→Mutex→send pattern (Decision C). The
// overlay is focusable:false during recording to keep the paste target
// focused; request_edit flips it (+set_focus) so the contenteditable caption
// receives keyboard input. confirm/cancel flip it back. Privacy §10.1: no
// transcript/edited text ever crosses tracing — the channel carries it in-
// memory only.

#[tauri::command]
fn request_edit(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    // Make the overlay focusable + foreground so the contenteditable caption
    // receives keystrokes (WS_EX_NOACTIVATE is removed by set_focusable(true);
    // set_focus brings it to foreground under the user's click gesture).
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_focusable(true);
        let _ = w.set_focus();
    }
    let tx = state.pending_paste.lock().unwrap();
    if let Some(tx) = tx.as_ref() {
        let _ = tx.send(pipeline::EditDecision::Pause);
    }
}

#[tauri::command]
fn confirm_paste(app: tauri::AppHandle, state: tauri::State<'_, AppState>, text: Option<String>) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_focusable(false);
    }
    let tx = state.pending_paste.lock().unwrap();
    if let Some(tx) = tx.as_ref() {
        let _ = tx.send(pipeline::EditDecision::Confirm(text));
    }
}

#[tauri::command]
fn cancel_paste(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_focusable(false);
    }
    let tx = state.pending_paste.lock().unwrap();
    if let Some(tx) = tx.as_ref() {
        let _ = tx.send(pipeline::EditDecision::Cancel);
    }
}

/// Best-effort re-paste of the failed text against the CURRENT foreground
/// (the original target HWND may be why it failed). Privacy §10.1: text stays
/// in-memory; `paste_text` logs only metadata (char count). On success clears
/// `last_failed_text` + hides; on failure re-stores it for another attempt and
/// re-shows the recovery UI with a fresh 2s hide window.
#[tauri::command]
fn paste_anyway(app: tauri::AppHandle, state: tauri::State<'_, AppState>) {
    let text = state.last_failed_text.lock().unwrap().take();
    let Some(text) = text else {
        return; // no-op: nothing to re-paste (already consumed / timed out)
    };
    let target = crate::paste::capture_target();
    let mode = state.settings.lock().unwrap().paste_mode;
    match crate::paste::paste_text(&text, target, mode) {
        Ok(()) => {
            let _ = crate::overlay::hide(&app);
        }
        Err(e) => {
            tracing::warn!("paste_anyway failed: {e}");
            *state.last_failed_text.lock().unwrap() = Some(text);
            let _ = crate::overlay::show_paste_failed(&app);
            let app_hide = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2000));
                *app_hide
                    .state::<AppState>()
                    .last_failed_text
                    .lock()
                    .unwrap() = None;
                let _ = crate::overlay::hide(&app_hide);
            });
        }
    }
}

/// "Open history" recovery button: surface the settings window + switch it to
/// the History section. Reuses the existing `navigate-history` event (already
/// wired in settings/main.ts + emitted by the tray) — zero frontend diff.
#[tauri::command]
fn open_history(app: tauri::AppHandle) {
    crate::tray::show_settings(&app);
    let _ = app.emit("navigate-history", ());
}

/// Parse `record <verb>` from a single-instance argv. `args` is
/// `std::env::args()` (includes argv[0] = exe path), so the subcommand is in
/// args[1..]. Returns `(verb, is_pressed)` for exactly `record start|stop|toggle`;
/// None for anything else (bare launch, `--autostarted`, deep-link). Factored
/// out of `forward_record_subcommand` for unit testing.
fn parse_record_argv(args: &[String]) -> Option<(&'static str, bool)> {
    match (
        args.get(1).map(String::as_str),
        args.get(2).map(String::as_str),
    ) {
        (Some("record"), Some("start")) => Some(("start", true)),
        (Some("record"), Some("toggle")) => Some(("toggle", true)),
        (Some("record"), Some("stop")) => Some(("stop", false)),
        _ => None,
    }
}

/// Wayland PTT (Task 11): `molvi record toggle|start|stop` from a compositor
/// keybinding forwards to the coordinator as the SAME `Command::Input` the
/// hotkey sends. `start`/`toggle` = press; `stop` = release. Mode is read live
/// so flipping recognition_mode in settings takes effect without a restart
/// (mirrors the hotkey handler in hotkey.rs). Returns true when the argv was a
/// record subcommand (caller skips the default settings-surface behavior).
fn forward_record_subcommand(app: &tauri::AppHandle, args: &[String]) -> bool {
    let Some((verb, is_pressed)) = parse_record_argv(args) else {
        return false;
    };
    let state = app.state::<AppState>();
    let mode = state.settings.lock().unwrap().recognition_mode;
    let tx = state.cmd_tx.lock().unwrap();
    if let Some(tx) = tx.as_ref() {
        let _ = tx.send(coordinator::Command::Input { is_pressed, mode });
        tracing::info!("record {verb}: forwarded via single-instance argv");
    } else {
        tracing::warn!("record {verb}: coordinator not ready (engine warming up)");
    }
    true
}

/// Tauri app entrypoint: logging, settings, tray, mic capture are set up
/// synchronously (tray interactive immediately); the model download, engine
/// worker, coordinator thread, and hotkey registration run on a background
/// thread so first-run cold start (~1s+) doesn't block the event loop. Holds
/// the log guard for the app lifetime.
pub fn run() {
    let _log_guard = log::init().expect("log init");
    // Engine-specific affinity (R6): pin to P-cores for GigaAM (measured help);
    // Nemotron uses all cores (P-core pinning is ~40% slower). Applied ONCE at
    // startup; engine-swap = restart, so no mid-process undo is needed.
    // Fail-open: p_core_mask -> None on a homogeneous CPU -> no pinning.
    let cfg = settings::Settings::load().unwrap_or_default();
    ort_affinity::apply_for_engine(crate::engine_adapter::is_nemotron(&cfg.model));
    match paths::settings_path() {
        Ok(p) => tracing::info!("settings loaded from {}", crate::paths::redact_appdata(&p)),
        Err(_) => tracing::info!("settings: using defaults"),
    }

    // Dictionary: best-effort open (in-memory fallback so the app runs even if
    // the DB is locked — CRUD would be ephemeral on that rare failure path).
    let dictionary = Arc::new(Mutex::new(dictionary::Dictionary::open().unwrap_or_else(
        |e| {
            tracing::error!("dictionary open failed, substitutions disabled until restart: {e}");
            dictionary::Dictionary::open_in_memory().unwrap()
        },
    )));
    // Snippets: best-effort open mirroring the dictionary block above. The
    // Smart-step gate (`snippets_enabled`) is off by default; the store is
    // always constructed so flipping the toggle at runtime needs no restart.
    let snippets = Arc::new(Mutex::new(snippets::Snippets::open().unwrap_or_else(|e| {
        tracing::error!("snippets open failed, expansion disabled until restart: {e}");
        snippets::Snippets::open_in_memory().unwrap()
    })));
    // History: opened only when the user opted in (privacy §10.1 consent gate).
    let history = match history::History::open_if_enabled(&cfg.history) {
        Some(Ok(h)) => Some(Arc::new(h)),
        Some(Err(e)) => {
            tracing::error!("history open failed (enabled), history disabled until restart: {e}");
            None
        }
        None => None,
    };

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Wayland PTT (Task 11): a compositor-keybinding launch with
            // `record <verb>` forwards to the coordinator and returns; any
            // other 2nd launch (bare, --autostarted) falls through to surface
            // the settings window.
            if forward_record_subcommand(app, &argv) {
                return;
            }
            // second launch: surface the settings window if it exists
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--autostarted"])
                .build(),
        )
        .manage(AppState {
            settings: Mutex::new(cfg),
            cmd_tx: Mutex::new(None),
            dictionary,
            snippets,
            history: Mutex::new(history),
            mic_preview: Arc::new(AtomicBool::new(false)),
            onboarding_practice: Arc::new(AtomicBool::new(false)),
            pending_paste: Mutex::new(None),
            last_failed_text: Mutex::new(None),
            model_download: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            cancel_operation,
            request_edit,
            confirm_paste,
            cancel_paste,
            paste_anyway,
            open_history,
            crate::ipc::get_settings,
            crate::ipc::set_settings,
            crate::ipc::dictionary_list,
            crate::ipc::dictionary_add,
            crate::ipc::dictionary_remove,
            crate::ipc::dictionary_import_preview,
            crate::ipc::dictionary_import_apply,
            crate::ipc::dictionary_export,
            crate::ipc::snippet_list,
            crate::ipc::snippet_add,
            crate::ipc::snippet_remove,
            crate::ipc::snippet_import,
            crate::ipc::snippet_export,
            crate::ipc::history_query,
            crate::ipc::history_bulk_delete,
            crate::ipc::history_distinct_langs,
            crate::ipc::history_delete,
            crate::ipc::history_clear,
            crate::ipc::history_disable_and_erase,
            crate::ipc::check_update,
            crate::ipc::apply_update,
            crate::ipc::re_paste,
            crate::ipc::list_audio_devices,
            crate::ipc::set_mic_preview,
            crate::ipc::set_onboarding_practice,
            crate::ipc::complete_onboarding,
            crate::ipc::pick_sound_file,
            crate::ipc::model_status,
            crate::ipc::download_model,
            crate::ipc::cancel_model_download,
            crate::ipc::restart_app,
        ])
        .setup(|app| {
            // Tray — interactive immediately. Real menu (Status/Toggle/
            // Settings/History/Quit) + handlers; Status text flips from
            // "warming up" to "molvi" once the bg load + hotkey register
            // complete (driven by tray::set_status below).
            let _tray = tray::build(app.handle())?;

            // macOS: convert the overlay window to a non-activating NSPanel
            // (focus fix). Best-effort; logs + skips on any error.
            #[cfg(target_os = "macos")]
            macos_overlay::init_overlay_panel(app.handle());

            let settings = app.state::<AppState>().settings.lock().unwrap().clone();

            // Task 10 D8: launch gate. First run (onboarded=false) shows the
            // onboarding window on top of the tray; the bg thread proceeds
            // UNCHANGED (download/engine/hotkey) — onboarding observes it via
            // engine-ready. The single-instance plugin surfaces the SETTINGS
            // window on second launch — leave as-is.
            if !settings.onboarded
                && let Some(w) = app.get_webview_window("onboarding")
            {
                let _ = w.show();
                let _ = w.set_focus();
            }

            // Autostart reconcile: force OS state to match the user's setting
            // (one-shot at startup; the IPC toggle in a later task writes both).
            let want = settings.autostart;
            let have = app.autolaunch().is_enabled().unwrap_or(false);
            if want != have {
                let corrected = if want {
                    app.autolaunch().enable().is_ok()
                } else {
                    app.autolaunch().disable().is_ok()
                };
                if corrected {
                    tracing::info!("autostart reconciled to {want}");
                } else {
                    tracing::warn!("autostart reconcile to {want} failed");
                }
            }

            // Startup update check (fire-and-forget, off the setup + inference
            // hot paths). Metadata-only logs (version strings / endpoint errors).
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let check = app_handle
                    .state::<AppState>()
                    .settings
                    .lock()
                    .unwrap()
                    .updater
                    .check_on_startup;
                if check {
                    match crate::updater::check(&app_handle).await {
                        Ok(r) => tracing::info!(
                            "startup update check: up_to_date={} version={:?} current={}",
                            r.up_to_date,
                            r.version,
                            r.current_version
                        ),
                        // ponytail: background best-effort check — a failure
                        // (pre-release PLACEHOLDER endpoint, transient network,
                        // no releases yet) isn't warn-worthy at startup. The
                        // user-initiated check (Updates UI) still surfaces errors.
                        Err(e) => tracing::debug!("startup update check failed: {e}"),
                    }
                }
            });

            // Mic capture: opens the device (fast), paused until begin_session.
            // The SPSC consumer is taken here and handed to the engine worker.
            let mut capture = audio::AudioCapture::start(settings.audio.input_device.as_deref())?;
            let native_rate = capture.native_rate();
            let mic_level = capture.mic_level();
            capture.pause();
            let consumer = capture
                .consumer()
                .ok_or_else(|| errors::MolviError::Audio("SPSC consumer already taken".into()))?;

            // mic-level poller (~30fps): one atomic read + one Tauri emit per
            // 33ms, no allocation. Stays in setup (cheap). Emits while the
            // overlay is visible (recording) OR mic-preview is on (Settings UI
            // meter test). app.emit is global → both windows receive it.
            {
                let app_handle = app.handle().clone();
                let mic_preview = app.state::<AppState>().mic_preview.clone();
                std::thread::Builder::new()
                    .name("molvi-mic-level".into())
                    .spawn(move || {
                        loop {
                            let visible = app_handle
                                .get_webview_window("overlay")
                                .map(|w| w.is_visible().unwrap_or(false))
                                .unwrap_or(false);
                            if visible || mic_preview.load(std::sync::atomic::Ordering::Relaxed) {
                                let level = mic_level.load(std::sync::atomic::Ordering::Relaxed);
                                let _ = overlay::emit_mic_level(&app_handle, level);
                            }
                            std::thread::sleep(std::time::Duration::from_millis(33));
                        }
                    })?;
            }

            // Background model-load + engine spawn + coordinator + hotkey.
            // setup() returns immediately so the tray is interactive; the
            // hotkey registers only once the engine is ready (a premature press
            // before registration is a silent no-op — spec §6.5). Verified:
            // AppHandle + TrayIcon are Clone+Send (docs.rs tauri 2.11.5); the
            // mpsc Sender is Send; cpal Stream + rtrb Consumer are Send.
            let app_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("molvi-setup-bg".into())
                .spawn(move || {
                    // First-run model download (~214MB) blocks here. Cached -> no-op.
                    // ensure_model is async (hf-hub async client); the cold-start thread
                    // stays sync (blaze-critical path) and bridges the one call via the
                    // global tokio runtime. |_| None = no per-byte progress here; the
                    // onboarding bar listens to engine-ready/engine-error (Task 10).
                    tracing::info!("ensuring model {} is present", settings.model);
                    let model_dir = match tauri::async_runtime::block_on(model_store::ensure_model(
                        &settings.model,
                        |_| None,
                    )) {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::error!("model setup failed, PTT disabled: {e}");
                            let _ = app_handle.emit("engine-error", ());
                            crate::tray::show_settings(&app_handle);
                            return;
                        }
                    };
                    tracing::info!("model dir: {}", crate::paths::redact_appdata(&model_dir));

                    // Coordinator channel created BEFORE the engine spawn so the
                    // worker can forward `Command::AutoStop` (trailing-silence
                    // auto-stop) without its own back-channel. Everything below
                    // (AppPipeline, hotkey, IPC expose) already consumed
                    // cmd_tx; the spawn now also takes a clone.
                    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<coordinator::Command>();

                    let engine = match engine::EngineHandle::spawn(
                        &model_dir,
                        &settings,
                        native_rate,
                        consumer,
                        cmd_tx.clone(),
                    ) {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::error!("engine spawn failed, PTT disabled: {e}");
                            let _ = app_handle.emit("engine-error", ());
                            crate::tray::show_settings(&app_handle);
                            return;
                        }
                    };

                    // Coordinator thread: owns AppPipeline + the state machine.
                    let dictionary = app_handle.state::<AppState>().dictionary.clone();
                    let snippets = app_handle.state::<AppState>().snippets.clone();
                    let pipeline = pipeline::AppPipeline::new(
                        app_handle.clone(),
                        capture,
                        engine,
                        settings.clone(),
                        cmd_tx.clone(),
                        dictionary,
                        snippets,
                    );
                    if let Err(e) = std::thread::Builder::new()
                        .name("molvi-coordinator".into())
                        .spawn(move || coordinator::run(cmd_rx, pipeline))
                    {
                        tracing::error!("coordinator spawn failed, PTT disabled: {e}");
                        return;
                    }

                    // Expose cmd_tx to the IPC layer (cancel_operation).
                    *app_handle.state::<AppState>().cmd_tx.lock().unwrap() = Some(cmd_tx.clone());

                    // Hotkey registers now — a premature press before this was a
                    // silent no-op (nothing was registered).
                    if let Err(e) = hotkey::register(&app_handle, &settings.hotkey, cmd_tx) {
                        tracing::error!("hotkey register failed: {e}");
                    }

                    // macOS landmine: if another process holds Secure Event
                    // Input at startup, the global hotkey is silently
                    // suppressed (Handy #1578). Mid-session polling + a user
                    // toast are a follow-up (need Mac UX testing); this catches
                    // the held-at-startup case.
                    #[cfg(target_os = "macos")]
                    if secure_input_held() {
                        tracing::warn!(
                            "Secure Event Input is held — global hotkey may be \
                             suppressed (password field / 1Password / Terminal \
                             Secure Keyboard Entry)"
                        );
                    }

                    // Drives the Status item text + tooltip (warming-up -> ready).
                    tray::set_status(&app_handle);
                    tracing::info!("PTT ready");
                    // Task 10 D3: signal onboarding (Step 1 auto-advance + Step 3
                    // gate). Global emit — only the onboarding window listens;
                    // harmless no-op on subsequent launches. Metadata-only.
                    let _ = app_handle.emit("engine-ready", ());
                })?;

            Ok(())
        });
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());
    builder
        .run(tauri::generate_context!())
        .expect("error while running molvi");
}

#[cfg(test)]
mod record_argv_tests {
    use super::parse_record_argv;

    #[test]
    fn parses_toggle_start_stop() {
        let exe = "molvi".to_string();
        assert_eq!(
            parse_record_argv(&[exe.clone(), "record".into(), "toggle".into()]),
            Some(("toggle", true))
        );
        assert_eq!(
            parse_record_argv(&[exe.clone(), "record".into(), "start".into()]),
            Some(("start", true))
        );
        assert_eq!(
            parse_record_argv(&[exe.clone(), "record".into(), "stop".into()]),
            Some(("stop", false))
        );
    }

    #[test]
    fn rejects_non_record_argv() {
        let exe = "molvi".to_string();
        assert_eq!(parse_record_argv(std::slice::from_ref(&exe)), None);
        assert_eq!(
            parse_record_argv(&[exe.clone(), "--autostarted".into()]),
            None
        );
        assert_eq!(
            parse_record_argv(&[exe.clone(), "record".into(), "frobnicate".into()]),
            None
        );
        assert_eq!(parse_record_argv(&[exe.clone(), "settings".into()]), None);
        assert_eq!(parse_record_argv(&[]), None);
    }
}
