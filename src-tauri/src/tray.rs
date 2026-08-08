//! System-tray icon + menu (Status / Settings / History / Quit).
//! Built once in `setup`; `set_status` / `set_recording` update the Status
//! item text (+ tooltip) at runtime; `rebuild` re-labels every item when
//! `settings.ui_lang` changes. Privacy §10.1: every label here is a
//! fixed UI string — no transcript/dict content crosses the tray.

use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::AppState;
use crate::tray_locales::tray_t;

/// Which logical status the tray Status item currently shows. Tracked so
/// `rebuild` (on language change) re-applies the right localized string
/// instead of clobbering a "recording"/"ready" state with "warming up".
#[derive(Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Warming,
    Ready,
    Recording,
}

/// Handles to all 4 label-bearing menu items + the current status kind, held
/// in managed state so `set_status` / `set_recording` / `rebuild` reach them
/// without menu-by-id lookups. Items are `Arc`-based (tauri 2.11.5) — cheap
/// to clone; `set_text` re-labels in place (verified docs.rs tauri 2.11.5).
pub struct TrayState {
    status: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    history: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    status_kind: Mutex<StatusKind>,
}

/// Build the tray icon + menu. Call once in `setup`; manages `TrayState`.
pub fn build(app: &AppHandle) -> tauri::Result<tauri::tray::TrayIcon> {
    let ui_lang = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .ui_lang
        .clone();
    let s = tray_t(&ui_lang);
    let status = MenuItem::with_id(app, "status", s.status_warming, false, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let settings = MenuItem::with_id(app, "settings", s.settings, true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", s.history, true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", s.quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &sep1, &settings, &history, &sep2, &quit])?;
    app.manage(TrayState {
        status: status.clone(),
        settings: settings.clone(),
        history: history.clone(),
        quit: quit.clone(),
        status_kind: Mutex::new(StatusKind::Warming),
    });
    TrayIconBuilder::with_id("main")
        .tooltip(s.status_warming)
        .icon(app.default_window_icon().cloned().expect("no icon"))
        .menu(&menu)
        // Left-click opens Settings (Tauri canonical tray pattern); the native
        // menu stays available on right-click.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" | "history" => {
                show_settings(app);
                if event.id.as_ref() == "history" {
                    // No payload: just a signal to switch the settings UI to
                    // the History section (frontend fetches rows via Task 13).
                    let _ = app.emit("navigate-history", ());
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_settings(tray.app_handle());
            }
        })
        .build(app)
}

/// Warming-up -> ready. Called once from the bg setup thread when PTT is
/// ready. (The warming-up state is the INITIAL build state; nothing reverts to
/// it, so there's no `ready: bool` — this always flips to Ready.) Drives both
/// the Status item text and the tooltip.
pub fn set_status(app: &AppHandle) {
    let (s, st) = ctx(app);
    set_status_text(app, &st, StatusKind::Ready, s.status_ready);
}

/// Live recording indicator. Called from the pipeline on begin/finalize/cancel.
/// Only sets a fixed status string (no transcript).
pub fn set_recording(app: &AppHandle, active: bool) {
    let (s, st) = ctx(app);
    let (kind, text) = if active {
        (StatusKind::Recording, s.recording)
    } else {
        (StatusKind::Ready, s.status_ready)
    };
    set_status_text(app, &st, kind, text);
}

/// Re-label every menu item + the tooltip after `ui_lang` changes. Uses
/// `MenuItem::set_text` on the existing handles (verified docs.rs tauri
/// 2.11.5) — cheaper than rebuilding the `Menu` and keeps `TrayState` + all
/// handlers valid. Called from `ipc::set_settings`.
pub fn rebuild(app: &AppHandle) {
    let (s, st) = ctx(app);
    let _ = st.settings.set_text(s.settings);
    let _ = st.history.set_text(s.history);
    let _ = st.quit.set_text(s.quit);
    let kind = *st.status_kind.lock().unwrap();
    let text = match kind {
        StatusKind::Warming => s.status_warming,
        StatusKind::Ready => s.status_ready,
        StatusKind::Recording => s.recording,
    };
    set_status_text(app, &st, kind, text);
}

/// Shared preamble for `set_status` / `set_recording` / `rebuild`: read the
/// current `ui_lang`, resolve the localized strings, and borrow `TrayState`.
/// Lock ordering (settings dropped before TrayState is borrowed) + error
/// handling are identical to the pre-extraction form.
fn ctx<'a>(
    app: &'a AppHandle,
) -> (
    &'static crate::tray_locales::TrayStrings,
    tauri::State<'a, TrayState>,
) {
    let ui_lang = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .ui_lang
        .clone();
    let s = tray_t(&ui_lang);
    let st = app.state::<TrayState>();
    (s, st)
}

/// Write `kind` + `text` to the Status item, tooltip, and the kind tracker.
/// Hold the kind-guard across set_text + set_tooltip so a concurrent rebuild /
/// set_recording can't interleave a stale value in between (set_text/set_tooltip
/// are leaf FFI — no re-entry into TrayState). Mirrors the tray tooltip for the
/// tray-hover native UI (fixed UI string, no transcript; privacy §10.1 safe).
fn set_status_text(app: &AppHandle, st: &TrayState, kind: StatusKind, text: &str) {
    let mut g = st.status_kind.lock().unwrap();
    *g = kind;
    let _ = st.status.set_text(text);
    if let Some(t) = app.tray_by_id("main") {
        let _ = t.set_tooltip(Some(text));
    }
}

pub(crate) fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}
