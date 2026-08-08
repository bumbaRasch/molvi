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

// ── Windows: WH_KEYBOARD_LL for reliable PTT release detection ──────────────
//
// global-hotkey 0.8 detects release via GetAsyncKeyState polling
// (platform_impl/windows/mod.rs:157). In optimized (release) builds, the first
// poll fires a false Released ~immediately after press, collapsing PTT to a
// 60ms session. GetAsyncKeyState is fundamentally unreliable for
// RegisterHotKey-registered keys (the hotkey system consumes the keydown
// event, so the async key state may report "up" while the key is physically
// held). This affects BOTH the main key AND the modifier.
//
// Fix: WH_KEYBOARD_LL — a low-level keyboard hook that sees ALL key-up events
// at the OS level, completely bypassing GetAsyncKeyState. The hook is installed
// once on a dedicated thread (message-loop-pumping), "armed" on WM_HOTKEY
// (press), and fires the release when either the modifier or main key goes up.

#[cfg(target_os = "windows")]
mod ll_hook {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use std::sync::mpsc::Sender;

    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW, WH_KEYBOARD_LL,
        WM_KEYUP, WM_SYSKEYUP,
    };

    use crate::coordinator::Command;
    use crate::settings::RecognitionMode;

    /// True when a PTT session is active (press detected, waiting for release).
    static ARMED: AtomicBool = AtomicBool::new(false);
    /// VK code of the modifier to watch for key-up (left variant; 0 = none).
    /// WH_KEYBOARD_LL reports VK_LMENU (0xA4), NOT VK_MENU (0x12).
    static MOD_VK: AtomicI32 = AtomicI32::new(0);
    /// VK code of the modifier to watch for key-up (right variant; 0 = none).
    static MOD_VK2: AtomicI32 = AtomicI32::new(0);
    /// VK code of the main key to watch for key-up (0 = none).
    static MAIN_VK: AtomicI32 = AtomicI32::new(0);
    /// RecognitionMode at press time (0=PTT, 1=Toggle, 2=Command).
    static MODE_I: AtomicI32 = AtomicI32::new(0);
    /// Channel to the coordinator. Set once at startup.
    static TX: OnceLock<Sender<Command>> = OnceLock::new();
    /// Ensure the hook thread is spawned only once.
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    fn mode_to_i(m: RecognitionMode) -> i32 {
        match m {
            RecognitionMode::PushToTalk => 0,
            RecognitionMode::Toggle => 1,
            RecognitionMode::Command => 2,
        }
    }

    fn i_to_mode(i: i32) -> RecognitionMode {
        match i {
            1 => RecognitionMode::Toggle,
            2 => RecognitionMode::Command,
            _ => RecognitionMode::PushToTalk,
        }
    }

    /// Install the hook (once) and update the watched VKs. Called from
    /// `register_one` on Windows.
    pub fn setup(tx: Sender<Command>, mod_vk: i32, mod_vk2: i32, main_vk: i32) {
        MOD_VK.store(mod_vk, Ordering::SeqCst);
        MOD_VK2.store(mod_vk2, Ordering::SeqCst);
        MAIN_VK.store(main_vk, Ordering::SeqCst);
        let _ = TX.set(tx);
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return; // hook thread already running; VKs updated above.
        }
        std::thread::Builder::new()
            .name("molvi-kbhook".into())
            .spawn(|| {
                unsafe extern "C" {
                    static __ImageBase: u8;
                }
                let hinst = HINSTANCE(unsafe { &__ImageBase } as *const _ as *mut _);
                let hook =
                    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), Some(hinst), 0) };
                match hook {
                    Ok(_) => tracing::info!("WH_KEYBOARD_LL hook installed"),
                    Err(e) => tracing::error!("WH_KEYBOARD_LL hook failed: {e}"),
                }
                // Message loop — the LL hook callback is invoked by the system
                // during GetMessageW. No DispatchMessageW (the original working
                // pattern; DispatchMessageW may interfere with hook messages).
                let mut msg = MSG::default();
                loop {
                    let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                    if ret.0 <= 0 {
                        tracing::info!("hook thread: GetMessageW returned {}, exiting", ret.0);
                        break;
                    }
                }
                tracing::warn!("hook thread exited unexpectedly");
            })
            .ok();
    }

    /// Arm: the PTT combo was pressed (WM_HOTKEY). Watch for key-up.
    pub fn arm(mode: RecognitionMode) {
        MODE_I.store(mode_to_i(mode), Ordering::SeqCst);
        ARMED.store(true, Ordering::SeqCst);
    }

    /// The hook callback. Must be instant (Windows removes slow hooks).
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let armed = ARMED.load(Ordering::Relaxed);
            let w = wparam.0;
            let is_up = w == WM_KEYUP as usize || w == WM_SYSKEYUP as usize;
            if armed && is_up {
                let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                let vk = kb.vkCode as i32;
                let mv = MOD_VK.load(Ordering::Relaxed);
                let mv2 = MOD_VK2.load(Ordering::Relaxed);
                let kv = MAIN_VK.load(Ordering::Relaxed);
                tracing::info!(
                    "LL hook: key-up vk=0x{:x} (mod=0x{:x}/0x{:x} main=0x{:x})",
                    vk,
                    mv,
                    mv2,
                    kv
                );
                if (mv != 0 && vk == mv) || (mv2 != 0 && vk == mv2) || (kv != 0 && vk == kv) {
                    tracing::info!("LL hook: release detected, disarming");
                    ARMED.store(false, Ordering::Relaxed);
                    if let Some(tx) = TX.get() {
                        let mode = i_to_mode(MODE_I.load(Ordering::Relaxed));
                        let _ = tx.send(Command::Input {
                            is_pressed: false,
                            mode,
                        });
                    }
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

/// Windows: modifier VK codes parsed from the binding, returning the LEFT and
/// RIGHT variants for the LL hook. WH_KEYBOARD_LL reports VK_LMENU (0xA4), NOT
/// VK_MENU (0x12) — using VK_MENU here was the root cause of missed releases.
/// Returns (left_vk, right_vk); both 0 if no modifier.
#[cfg(target_os = "windows")]
fn modifier_vks(binding: &str) -> (i32, i32) {
    let l = binding.to_ascii_lowercase();
    if l.contains("alt") {
        (0xA4, 0xA5) // VK_LMENU, VK_RMENU
    } else if l.contains("ctrl") || l.contains("control") {
        (0xA2, 0xA3) // VK_LCONTROL, VK_RCONTROL
    } else if l.contains("shift") {
        (0xA0, 0xA1) // VK_LSHIFT, VK_RSHIFT
    } else if l.contains("super") || l.contains("meta") || l.contains("win") {
        (0x5B, 0x5C) // VK_LWIN, VK_RWIN
    } else {
        (0, 0)
    }
}

/// Windows: VK code of the hotkey's main key, parsed from the binding.
#[cfg(target_os = "windows")]
fn hotkey_vk(binding: &str) -> Option<i32> {
    let key = binding.rsplit('+').next()?.trim();
    Some(match key {
        "`" => 0xC0,
        "-" => 0xBD,
        "=" => 0xBB,
        "[" => 0xDB,
        "]" => 0xDD,
        "\\" => 0xDC,
        ";" => 0xBA,
        "'" => 0xDE,
        "/" => 0xBF,
        "." => 0xBE,
        "," => 0xBC,
        "Space" => 0x20,
        _ if key.len() == 1 => {
            let c = key.chars().next()?;
            if c.is_ascii_alphabetic() {
                c.to_ascii_uppercase() as i32
            } else if c.is_ascii_digit() {
                c as i32
            } else {
                return None;
            }
        }
        _ if key.starts_with('F') => {
            let n = key[1..].parse::<u32>().ok()?;
            if (1..=12).contains(&n) {
                0x6F + n as i32
            } else {
                return None;
            }
        }
        _ => return None,
    })
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
    if mirror_on
        && let Some(mirror) = altgr_mirror_of(binding)
        && let Err(e) = register_one(app, &mirror, cmd_tx)
    {
        tracing::warn!("AltGr mirror '{mirror}' failed (primary still active): {e}");
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

    #[cfg(target_os = "windows")]
    let (mod_l, mod_r) = modifier_vks(binding);

    #[cfg(target_os = "windows")]
    let main_vk = hotkey_vk(binding);

    #[cfg(target_os = "windows")]
    let hook_capable = mod_l != 0 || main_vk.is_some();

    #[cfg(target_os = "windows")]
    if hook_capable {
        ll_hook::setup(cmd_tx.clone(), mod_l, mod_r, main_vk.unwrap_or(0));
    }

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            let is_pressed = matches!(event.state, ShortcutState::Pressed);
            let mode = app
                .state::<AppState>()
                .settings
                .lock()
                .unwrap()
                .recognition_mode;

            #[cfg(target_os = "windows")]
            {
                if is_pressed {
                    if let Err(e) = cmd_tx.send(Command::Input {
                        is_pressed: true,
                        mode,
                    }) {
                        tracing::error!("coordinator channel closed: {e}");
                    }
                    if hook_capable {
                        ll_hook::arm(mode);
                    }
                } else if !hook_capable {
                    // Unmapped keys: no VK for the hook → trust the plugin.
                    if let Err(e) = cmd_tx.send(Command::Input {
                        is_pressed: false,
                        mode,
                    }) {
                        tracing::error!("coordinator channel closed: {e}");
                    }
                }
                // else: mapped-key release — the LL hook handles it.
            }

            #[cfg(not(target_os = "windows"))]
            {
                if let Err(e) = cmd_tx.send(Command::Input { is_pressed, mode }) {
                    tracing::error!("coordinator channel closed: {e}");
                }
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
