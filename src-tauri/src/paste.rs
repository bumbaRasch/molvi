use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, InputError, Key, Keyboard, Settings,
};

use crate::errors::{MolviError, Result};
use crate::settings::PasteMode;

/// The modifier held for a paste/replace/command chord. Windows + Linux = Ctrl;
/// macOS = ⌘ (`Key::Meta`). (There is no `Key::Command` variant — `Key::Meta`
/// IS Command on macOS, Super on Linux, Win on Windows.)
pub fn paste_modifier() -> Key {
    #[cfg(target_os = "macos")]
    {
        Key::Meta
    }
    #[cfg(not(target_os = "macos"))]
    {
        Key::Control
    }
}

/// The key clicked for a paste. Windows = VK_V (`Key::Other(0x56)` — enigo's
/// Unicode path is rejected as Ctrl+V by some Windows apps); macOS =
/// `Key::Other(9)` (virtualKey 9 = physical V, layout-robust under ⌘ per
/// layout-robust); Linux/X11 = `Key::Unicode('v')` (XKB keysym).
pub fn paste_key() -> Key {
    #[cfg(target_os = "windows")]
    {
        Key::Other(0x56)
    }
    #[cfg(target_os = "macos")]
    {
        Key::Other(9)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Key::Unicode('v')
    }
}

/// Capture the current foreground window (the intended paste target).
/// Called at hotkey-down, before the overlay could ever steal focus.
/// HWND.0 is a public `*mut c_void` in windows 0.62; cast through isize for
/// Send storage (HWND itself is !Send).
#[cfg(target_os = "windows")]
pub fn capture_target() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    let h = hwnd.0 as isize;
    if h == 0 { None } else { Some(h) }
}

/// macOS: capture the frontmost app pid (Task 8's helper). Verify-only guard
/// in `ensure_focus` (spike #3) — no SetFrontmost attempt.
#[cfg(target_os = "macos")]
pub fn capture_target() -> Option<isize> {
    crate::profiles::macos_frontmost_pid()
}

/// Linux: capture the active X11 window id (Task 12). Wayland has no active-
/// window API → None (paste_text bypasses to the Wayland clipboard path before
/// the target guard).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn capture_target() -> Option<isize> {
    if crate::x11::is_wayland() {
        None // Wayland: no active-window id; paste_text bypasses to paste_wayland.
    } else {
        crate::x11::active_window_id().map(|w| w as isize)
    }
}

#[cfg(target_os = "windows")]
fn foreground_is(target: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let fg = unsafe { GetForegroundWindow() };
    (fg.0 as isize) == target
}

/// Focus guard (§6.6 invariant: app never takes foreground mid-session). If the
/// overlay somehow lost focus to the wrong window, attempt one restore; if still
/// wrong, leave any payload on the clipboard + Err so the caller can toast
/// rather than misdeliver into a stranger's window. Shared by `paste_text` and
/// `run_command_chord`.
#[cfg(target_os = "windows")]
fn ensure_focus(target: isize) -> Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
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

/// macOS focus guard (spike #3, verify-only). A background/accessory app
/// cannot reliably re-activate another app on macOS. The non-activating
/// NSPanel overlay (Task 7) keeps focus on the user's app, so a mismatch here
/// means an explicit user ⌘-tab → refuse (leave text on the clipboard + toast),
/// do NOT attempt SetFrontmost/FrontmostApplication restore.
#[cfg(target_os = "macos")]
fn ensure_focus(target: isize) -> Result<()> {
    if crate::profiles::macos_frontmost_pid() == Some(target) {
        Ok(())
    } else {
        tracing::warn!("paste: macOS frontmost app changed; left on clipboard");
        Err(MolviError::Paste(
            "focus mismatch; text left on clipboard".into(),
        ))
    }
}

/// Linux focus guard (Task 12). X11: verify the active window is still the
/// captured target, request activation if not, re-verify (mirrors xdotool/
/// wmctrl). Wayland: no restore (paste_text bypasses here before this is
/// reached; defensive Ok). Err leaves text on the clipboard (§6.6).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn ensure_focus(target: isize) -> Result<()> {
    if crate::x11::is_wayland() {
        Ok(()) // Wayland: no restore (paste_text bypasses here; defensive Ok).
    } else {
        crate::x11::ensure_active_window(target as u32)
    }
}

/// Paste per spec §6.6. Clipboard-paste primary, type fallback, focus-guarded.
/// Privacy (§10.1): logs metadata only (char count); never the transcript text.
pub fn paste_text(text: &str, target: Option<isize>, mode: PasteMode) -> Result<()> {
    if text.is_empty() {
        tracing::info!("paste: empty transcript, nothing to do");
        return Ok(());
    }

    // Wayland (Linux): no active-window API + enigo inject only reaches XWayland
    // apps. Bypass to the clipboard-primary path before the X11/Win/macOS target
    // + focus-guard logic below.
    #[cfg(all(unix, not(target_os = "macos")))]
    if crate::x11::is_wayland() {
        return paste_wayland(text, mode);
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

    // Clipboard settle delay: arboard closes the clipboard synchronously, but in
    // release builds the enigo Ctrl+V fires so fast that some apps read a stale
    // clipboard. A settle delay matches the proven paste-delay pattern.
    thread::sleep(Duration::from_millis(30));

    // macOS: Enigo::new fires the Accessibility-permission prompt on first use
    // (Settings::default() sets open_prompt_to_get_permissions = true). The user
    // grants it once; subsequent pastes work.
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;

    deliver_paste_chord(&mut enigo, mode)?;
    Ok(())
}

/// Deliver the paste chord: optional select-all (Replace mode) then the
/// platform paste key combo. Shared by the focus-guarded path (X11/Win/macOS)
/// and the Wayland clipboard-primary path.
//
// Platform paste keys (verified enigo 0.6.1 API: Keyboard::key(Key, Direction)):
// Windows = Ctrl+VK_V (`Key::Other(0x56)` — enigo's Unicode path sends
// KEYEVENTF_UNICODE which types the literal char and does NOT combine with held
// Ctrl → would type "v" instead of pasting; Key::Other respects modifier state
// → real Ctrl+V. Caught in Phase-1 smoke); macOS = ⌘+VK 9; Linux = Ctrl+'v'.
fn deliver_paste_chord(enigo: &mut Enigo, mode: PasteMode) -> Result<()> {
    // Replace mode: select the focused control's whole text first so the
    // paste overwrites it. Ctrl/⌘+A in a form field selects just that field;
    // in full-document editors (Word/Google Docs) it selects the whole doc —
    // the i18n hint (text.paste_replace_hint) names this caveat. No
    // exe-denylist: browser editors run as chrome.exe/msedge.exe and would
    // leak any denylist (false security); honest labeling is the real guard.
    // ponytail: select-all key via letter_key (platform VK); two separate
    // chords (not one held modifier) — robust to apps that key off Ctrl
    // transitions.
    if mode == PasteMode::Replace {
        let select_all_key = crate::commands::letter_key('a');
        enigo
            .key(paste_modifier(), Press)
            .map_err(paste_err("mod down (select-all)"))?;
        enigo
            .key(select_all_key, Click)
            .map_err(paste_err("a click"))?;
        enigo
            .key(paste_modifier(), Release)
            .map_err(paste_err("mod up (select-all)"))?;
        // Let the selection settle before the paste chord overwrites it.
        thread::sleep(Duration::from_millis(20));
        tracing::info!("paste: select-all delivered (replace mode)");
    }

    enigo
        .key(paste_modifier(), Press)
        .map_err(paste_err("mod down"))?;
    enigo
        .key(paste_key(), Click)
        .map_err(paste_err("paste key click"))?;
    enigo
        .key(paste_modifier(), Release)
        .map_err(paste_err("mod up"))?;
    tracing::info!("paste: paste chord delivered");
    Ok(())
}

/// Wayland clipboard-primary paste (Linux only). Wayland has no active-window
/// API + enigo's keystroke inject only reaches XWayland apps (native Wayland
/// apps reject it). Path: arboard sets the clipboard (shells out to wl-copy),
/// blast-release all modifiers (can't read modifier state on Wayland — the PTT
/// key may still be logically held; mirrors wdotool --clearmodifiers), then
/// best-effort Ctrl+V. On enigo failure the caller's paste-failed recovery
/// surfaces "text on clipboard, press Ctrl+V". Privacy §10.1: logs metadata
/// only (char count), never the text.
#[cfg(all(unix, not(target_os = "macos")))]
fn paste_wayland(text: &str, mode: PasteMode) -> Result<()> {
    if mode == PasteMode::Type {
        // Best-effort enigo text inject (XWayland only). Native Wayland apps
        // may reject it; the caller handles Err via paste-failed recovery.
        return type_text(text);
    }
    let mut clip = Clipboard::new().map_err(|e| MolviError::Paste(format!("clipboard: {e}")))?;
    clip.set_text(text)
        .map_err(|e| MolviError::Paste(format!("set clipboard: {e}")))?;
    drop(clip);
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;
    blast_modifiers(&mut enigo);
    deliver_paste_chord(&mut enigo, mode)
}

/// Release every modifier unconditionally (Wayland: can't read modifier state;
/// the PTT key may still be logically held after the compositor keybinding).
/// Releasing an un-held key is a harmless no-op. (espanso/wdotool pattern.)
#[cfg(all(unix, not(target_os = "macos")))]
fn blast_modifiers(enigo: &mut Enigo) {
    for key in [Key::Shift, Key::Control, Key::Alt, Key::Meta] {
        let _ = enigo.key(key, Release);
    }
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
            .key(paste_modifier(), Press)
            .map_err(paste_err("mod down"))?;
    }
    for k in &chord.keys {
        enigo.key(*k, Click).map_err(paste_err("key click"))?;
    }
    if chord.hold_ctrl {
        enigo
            .key(paste_modifier(), Release)
            .map_err(paste_err("mod up"))?;
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

    #[test]
    fn paste_modifier_matches_platform() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(paste_modifier(), Key::Meta);
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(paste_modifier(), Key::Control);
        }
    }

    #[test]
    fn paste_key_matches_platform() {
        #[cfg(target_os = "windows")]
        {
            assert_eq!(paste_key(), Key::Other(0x56));
        }
        #[cfg(target_os = "macos")]
        {
            assert_eq!(paste_key(), Key::Other(9));
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            assert_eq!(paste_key(), Key::Unicode('v'));
        }
    }
}
