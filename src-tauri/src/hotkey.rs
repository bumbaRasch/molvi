use std::sync::mpsc;

use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::coordinator::Command;
use crate::errors::{MolviError, Result};

fn altgr_mirror_of(binding: &str) -> Option<String> {
    if !binding.to_ascii_lowercase().starts_with("alt+") {
        return None;
    }
    Some(format!("Ctrl+{binding}"))
}

pub fn register(app: &AppHandle, binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    register_one(app, binding, cmd_tx.clone())?;
    let mirror_on = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .hotkey_altgr_mirror;
    if mirror_on
        && let Some(mirror) = altgr_mirror_of(binding)
        && let Err(e) = register_one(app, &mirror, cmd_tx)
    {
        tracing::warn!("AltGr mirror '{mirror}' failed (primary still active): {e}");
    }
    Ok(())
}

fn register_one(app: &AppHandle, binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let shortcut = binding
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| MolviError::Hotkey(format!("parse binding '{binding}': {e}")))?;

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            let is_pressed = matches!(event.state, ShortcutState::Pressed);
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
