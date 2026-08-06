use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, InputError, Key, Keyboard, Settings,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

use crate::errors::{MolviError, Result};
use crate::settings::PasteMode;

/// Capture the current foreground window (the intended paste target).
/// Called at hotkey-down, before the overlay could ever steal focus.
/// HWND.0 is a public `*mut c_void` in windows 0.62; cast through isize for
/// Send storage (HWND itself is !Send).
pub fn capture_target() -> Option<isize> {
    let hwnd = unsafe { GetForegroundWindow() };
    let h = hwnd.0 as isize;
    if h == 0 { None } else { Some(h) }
}

fn foreground_is(target: isize) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    (fg.0 as isize) == target
}

/// Focus guard (§6.6 invariant: app never takes foreground mid-session). If the
/// overlay somehow lost focus to the wrong window, attempt one restore; if still
/// wrong, leave any payload on the clipboard + Err so the caller can toast
/// rather than misdeliver into a stranger's window. Shared by `paste_text` and
/// `run_command_chord`.
fn ensure_focus(target: isize) -> Result<()> {
    if foreground_is(target) {
        return Ok(());
    }
    tracing::warn!("paste: foreground mismatch, attempting SetForegroundWindow");
    unsafe {
        let _ = SetForegroundWindow(HWND(target as *mut _));
    }
    thread::sleep(Duration::from_millis(40));
    if foreground_is(target) {
        Ok(())
    } else {
        tracing::warn!("paste: could not restore focus; left on clipboard");
        Err(MolviError::Paste(
            "focus mismatch; text left on clipboard".into(),
        ))
    }
}

/// Paste per spec §6.6. Clipboard-paste primary, type fallback, focus-guarded.
/// Privacy (§10.1): logs metadata only (char count); never the transcript text.
pub fn paste_text(text: &str, target: Option<isize>, mode: PasteMode) -> Result<()> {
    if text.is_empty() {
        tracing::info!("paste: empty transcript, nothing to do");
        return Ok(());
    }

    // No captured target (capture_target returned NULL): no window to paste
    // into. Bail rather than mispaste into whatever happens to be foreground.
    let t = target
        .ok_or_else(|| MolviError::Paste("no captured target; text left on clipboard".into()))?;

    // Focus guard (§6.6), hoisted BEFORE the mode branch so BOTH clipboard +
    // type (and command-chord) paths are protected.
    ensure_focus(t)?;

    // Target confirmed. Type mode bypasses clipboard entirely.
    if mode == PasteMode::Type {
        return type_text(text);
    }

    let mut clip = Clipboard::new().map_err(|e| MolviError::Paste(format!("clipboard: {e}")))?;
    clip.set_text(text)
        .map_err(|e| MolviError::Paste(format!("set clipboard: {e}")))?;
    drop(clip); // release before simulating keys

    // Ctrl+V (verified enigo 0.6.1 API: Keyboard::key(Key, Direction) -> InputResult).
    // ponytail: Key::Other(0x56) is VK_V, NOT Key::Unicode('v'). enigo's Unicode
    // path on Windows sends SendInput KEYEVENTF_UNICODE, which types the literal
    // char and does NOT combine with the held Ctrl → would type "v" instead of
    // pasting. Key::Other(u32) sends a Virtual_Key (ctx7/enigo docs) which
    // respects the modifier state → real Ctrl+V paste. Caught in Phase-1 smoke.
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;

    // Replace mode: select the focused control's whole text first so the
    // paste overwrites it. Ctrl+A in a form field selects just that field;
    // in full-document editors (Word/Google Docs) it selects the whole doc —
    // the i18n hint (text.paste_replace_hint) names this caveat. No
    // exe-denylist: browser editors run as chrome.exe/msedge.exe and would
    // leak any denylist (false security); honest labeling is the real guard.
    // ponytail: VK_A=0x41 per Win32 VK table; Key::Other(u32)=VK pattern
    // proven at the Ctrl+V chord below (VK_V=0x56). Two separate chords
    // (not one held Ctrl) — robust to apps that key off Ctrl transitions.
    if mode == PasteMode::Replace {
        enigo
            .key(Key::Control, Press)
            .map_err(paste_err("ctrl down (select-all)"))?;
        enigo
            .key(Key::Other(0x41), Click)
            .map_err(paste_err("a click"))?;
        enigo
            .key(Key::Control, Release)
            .map_err(paste_err("ctrl up (select-all)"))?;
        // Let the selection settle before the paste chord overwrites it.
        thread::sleep(Duration::from_millis(20));
        tracing::info!("paste: Ctrl+A delivered (replace mode)");
    }

    enigo
        .key(Key::Control, Press)
        .map_err(paste_err("ctrl down"))?;
    enigo
        .key(Key::Other(0x56), Click)
        .map_err(paste_err("v click"))?;
    enigo
        .key(Key::Control, Release)
        .map_err(paste_err("ctrl up"))?;
    tracing::info!("paste: Ctrl+V delivered");
    Ok(())
}

/// Emit a command-mode key chord into the captured target window (spec §6.5.4).
/// Reuses the same focus guard as `paste_text`. Logs nothing content-bearing
/// here — the caller logs a fixed string on Ok (privacy §10.1). No sleep
/// between modifier down and the key clicks (a single chord needs no settle,
/// unlike paste.rs's Ctrl+A→Ctrl+V selection pause).
pub fn run_command_chord(chord: &crate::commands::KeyChord, target: Option<isize>) -> Result<()> {
    let t = target.ok_or_else(|| MolviError::Paste("no captured target".into()))?;
    ensure_focus(t)?;
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;
    if chord.hold_ctrl {
        enigo
            .key(Key::Control, Press)
            .map_err(paste_err("ctrl down"))?;
    }
    for k in &chord.keys {
        enigo.key(*k, Click).map_err(paste_err("key click"))?;
    }
    if chord.hold_ctrl {
        enigo
            .key(Key::Control, Release)
            .map_err(paste_err("ctrl up"))?;
    }
    Ok(())
}

fn type_text(text: &str) -> Result<()> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;
    enigo.text(text).map_err(paste_err("type"))?;
    tracing::info!("paste: typed {} chars", text.chars().count());
    Ok(())
}

fn paste_err(ctx: &'static str) -> impl Fn(InputError) -> MolviError {
    move |e| MolviError::Paste(format!("{ctx}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests cover the two early-return branches that exit BEFORE any external
    // (arboard/enigo/Win32) call. The focus-guard, clipboard-set, Ctrl+V, and
    // type paths all require a real OS environment (GetForegroundWindow,
    // arboard::Clipboard, enigo::Enigo) — deeper coverage is integration-test
    // territory, not unit-testable without mocking the OS.

    #[test]
    fn empty_text_is_noop() {
        // Empty transcript: early-return Ok, no target needed.
        assert!(paste_text("", None, PasteMode::Clipboard).is_ok());
        assert!(paste_text("", None, PasteMode::Type).is_ok());
        // Replace routes through the same shared guard (precedes the mode branch).
        assert!(paste_text("", None, PasteMode::Replace).is_ok());
    }

    #[test]
    fn missing_target_errors_before_any_external_call() {
        // No captured target (capture_target returned NULL): Err before
        // foreground_is / arboard / enigo are ever reached.
        let err = paste_text("привет", None, PasteMode::Clipboard).unwrap_err();
        assert!(err.to_string().contains("no captured target"));
    }

    #[test]
    fn missing_target_errors_regardless_of_mode() {
        // Type mode hits the same target-None guard (it precedes the mode branch).
        assert!(paste_text("test", None, PasteMode::Type).is_err());
    }
}
