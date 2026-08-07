use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::errors::{MolviError, Result};

// Privacy (spec §10.1): the overlay DISPLAYS transcript text in the webview
// (that is its job), but no fn here ever passes the text to `tracing::`.
// emit_text sends the payload over the Tauri IPC bus only.

pub fn window(app: &AppHandle) -> Result<WebviewWindow> {
    app.get_webview_window("overlay")
        .ok_or_else(|| MolviError::Overlay("overlay window not found".into()))
}

/// Emit `show-overlay { state }` and unhide the window. `state` is metadata
/// (e.g. "recording"/"processing"), never transcript text.
pub fn show(app: &AppHandle, state: &str) -> Result<()> {
    let w = window(app)?;
    // Bottom-center of the primary monitor's work area (taskbar-excluded).
    // ponytail: runs once per show (off the hot path); manual-smoke-only —
    // needs a real window/monitor, not unit-testable.
    if let Ok(Some(mon)) = w.primary_monitor() {
        let wa = mon.work_area();
        let size = w.outer_size().unwrap_or(tauri::PhysicalSize {
            width: 720,
            height: 120,
        });
        let x = wa.position.x + (wa.size.width as i32 - size.width as i32).max(0) / 2;
        let y = wa.position.y + wa.size.height as i32 - size.height as i32;
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
    }
    let _ = app.emit("show-overlay", json!({ "state": state }));
    w.show()
        .map_err(|e| MolviError::Overlay(format!("show: {e}")))?;
    Ok(())
}

pub fn hide(app: &AppHandle) -> Result<()> {
    let _ = app.emit("hide-overlay", ());
    if let Ok(w) = window(app) {
        // ponytail: focusable is set true ONLY by request_edit (Polished edit-
        // window); hide is the single chokepoint that resets it across every
        // exit path (success, paste-failed, Skip-via-disconnect, cancel) so the
        // overlay never stays activation-eligible into the next session.
        let _ = w.set_focusable(false);
        let _ = w.hide();
    }
    Ok(())
}

pub fn emit_text(app: &AppHandle, text: &str) -> Result<()> {
    app.emit("stream-text", json!({ "text": text }))
        .map_err(|e| MolviError::Overlay(format!("emit stream-text: {e}")))
}

pub fn emit_mic_level(app: &AppHandle, level: u32) -> Result<()> {
    app.emit("mic-level", json!({ "level": level }))
        .map_err(|e| MolviError::Overlay(format!("emit mic-level: {e}")))
}

/// Polished edit-window (Task 9, Decision B): emit the post-processed text so
/// the overlay can reveal the Edit affordance before paste. Privacy §10.1:
/// text crosses the IPC bus only (same rule as `emit_text`) — never logged.
pub fn emit_edit_ready(app: &AppHandle, text: &str) -> Result<()> {
    app.emit("edit-ready", json!({ "text": text }))
        .map_err(|e| MolviError::Overlay(format!("emit edit-ready: {e}")))
}

pub fn emit_phase(app: &AppHandle, phase: &str, kind: &str) -> Result<()> {
    app.emit("phase", json!({ "phase": phase, "kind": kind }))
        .map_err(|e| MolviError::Overlay(format!("emit phase: {e}")))
}

/// Signal a paste failure (spec §6.6) so the overlay can tell the user the
/// transcript landed on the clipboard. Re-shows the (already-hidden) overlay
/// window with a fixed-caption signal — the frontend hardcodes the Russian
/// message, so no transcript-equivalent text crosses the IPC bus (privacy
/// §10.1). The caller schedules a delayed `hide`.
pub fn show_paste_failed(app: &AppHandle) -> Result<()> {
    let w = window(app)?;
    let _ = app.emit("paste-failed", ());
    w.show()
        .map_err(|e| MolviError::Overlay(format!("show paste-failed: {e}")))?;
    Ok(())
}
