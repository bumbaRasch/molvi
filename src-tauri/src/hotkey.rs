use std::sync::mpsc;

use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::coordinator::Command;
use crate::errors::{MolviError, Result};

// ponytail: `init()` does not exist in tauri-plugin-global-shortcut 2.3.2; the
// plugin is registered via `Builder::new().build()` in main.rs. Verified
// 2026-08-03 against docs.rs/2.3.2/src/lib.rs and v2.tauri.app/plugin/global-shortcut.
// `on_shortcut` handler: Fn(&AppHandle, &Shortcut, ShortcutEvent) + Send + Sync
// + 'static; press/release is `event.state: ShortcutState` (a public field,
// re-exported from global-hotkey::HotKeyState). Both edges fire for one
// registration (verified Phase-1 smoke; §16.4 PASS).
//
// AltGr caveat (Win32 RegisterHotKey, confirmed via gh research 2026-08-03):
// the default `Alt+`` binding matches LEFT Alt only. Right Alt on RU/EU
// layouts is AltGr, synthesized by the keyboard driver as Ctrl+Alt — fails
// the MOD_ALT-only match. molvi registers a `Ctrl+Alt+…` mirror of any
// Alt-based binding when `settings.hotkey_altgr_mirror` is on (opt-in; default
// off), so AltGr fires the same handler. See `altgr_mirror_of` + `register`.

/// If `binding` is an Alt-based binding (`Alt+…`), return its AltGr mirror
/// (`Ctrl+Alt+…`). Right-Alt on RU/EU layouts is synthesized as Ctrl+Alt by
/// the keyboard driver and won't fire a plain `Alt+` registration. Non-Alt
/// bindings return None (the mirror is meaningless for them).
fn altgr_mirror_of(binding: &str) -> Option<String> {
    // ponytail: to_ascii_lowercase allocates once per call (registration +
    // rebind only); safer than byte-slicing a possibly-multibyte string.
    if !binding.to_ascii_lowercase().starts_with("alt+") {
        return None;
    }
    Some(format!("Ctrl+{binding}"))
}

/// Register the PTT binding. A single registration fires on both edges
/// (press + release) via `event.state`; release edge is the PTT-defining
/// behavior (spec §6.5 / §16.4). When `settings.hotkey_altgr_mirror` is on and
/// the binding is Alt-based, also registers the `Ctrl+Alt+…` mirror so RU/EU
/// AltGr fires the same handler.
pub fn register(app: &AppHandle, binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    register_one(app, binding, cmd_tx.clone())?;
    let mirror_on = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .hotkey_altgr_mirror;
    if mirror_on && let Some(mirror) = altgr_mirror_of(binding) {
        // ponytail: best-effort — the mirror is an opt-in enhancement. A mirror
        // parse/register failure leaves the primary binding working, so
        // downgrade to a warning rather than failing the whole registration
        // (which would leave the user with no hotkey). rebind() always
        // unregister_all()s first, so no stuck partial state across a rebind.
        if let Err(e) = register_one(app, &mirror, cmd_tx) {
            tracing::warn!("AltGr mirror '{mirror}' failed (primary still active): {e}");
        }
    }
    Ok(())
}

/// Register exactly one shortcut + its handler. Shared by the primary binding
/// and the AltGr mirror (same handler, same coordinator channel).
fn register_one(app: &AppHandle, binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let shortcut = binding
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| MolviError::Hotkey(format!("parse binding '{binding}': {e}")))?;

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            let is_pressed = matches!(event.state, ShortcutState::Pressed);
            // ponytail: read the mode live at fire time so flipping it (a
            // later settings task) takes effect immediately without forcing a
            // hotkey re-register on mode-only changes and without a stale
            // captured value. Lock held for microseconds on human-paced edges.
            let mode = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .recognition_mode;
            if let Err(e) = cmd_tx.send(Command::Input { is_pressed, mode }) {
                tracing::error!("coordinator channel closed: {e}");
            }
        })
        .map_err(|e| MolviError::Hotkey(format!("register shortcut: {e}")))?;
    tracing::info!("registered hotkey: {binding}");
    Ok(())
}

/// Re-register after a settings change: unregister everything, then register
/// the new binding. molvi has exactly one binding at a time, so unregister_all
/// is the simplest correct rebind.
pub fn rebind(app: &AppHandle, new_binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    app.global_shortcut()
        .unregister_all()
        .map_err(|e| MolviError::Hotkey(format!("unregister all: {e}")))?;
    register(app, new_binding, cmd_tx)
}

#[cfg(test)]
mod tests {
    use super::altgr_mirror_of;

    #[test]
    fn altgr_mirror_default_binding() {
        assert_eq!(altgr_mirror_of("Alt+`"), Some("Ctrl+Alt+`".to_string()));
    }

    #[test]
    fn altgr_mirror_case_insensitive_prefix() {
        assert_eq!(altgr_mirror_of("alt+x"), Some("Ctrl+alt+x".to_string()));
    }

    #[test]
    fn altgr_mirror_non_alt_returns_none() {
        assert_eq!(altgr_mirror_of("Ctrl+Space"), None);
    }

    #[test]
    fn altgr_mirror_preserves_modifiers_after_alt() {
        assert_eq!(
            altgr_mirror_of("Alt+Shift+F2"),
            Some("Ctrl+Alt+Shift+F2".to_string())
        );
    }

    #[test]
    fn altgr_mirror_empty_returns_none() {
        assert_eq!(altgr_mirror_of(""), None);
    }
}
