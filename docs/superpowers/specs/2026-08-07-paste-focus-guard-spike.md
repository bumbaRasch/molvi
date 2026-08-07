# Spike #3 — paste focus-guard portability (macOS / X11 / Wayland)

> Date: 2026-08-07 · Status: **research complete, feeds the port spec**
> Parent: `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md`
> (spike #3). Doc-only; no code.

## Question

molvi's §6.6 invariant — never misdeliver a paste into a stranger's window — is
enforced on Windows by `capture_target()` (save foreground HWND at hotkey-down)
+ `ensure_focus()` (verify/restore before enigo Ctrl+V). What is the **portable
shape** of this guard on macOS + Linux/X11 (+ Wayland)?

## Biggest semantic difference vs Windows

| | Windows (today) | macOS | Linux X11 | Wayland |
|---|---|---|---|---|
| `capture_target()` | `HWND` (`GetForegroundWindow`) | `pid_t` (`NSWorkspace.frontmostApplication.processIdentifier`) | X window id (`_NET_ACTIVE_WINDOW`) | **`None`** |
| restore possible? | yes (`SetForegroundWindow`) | **NO** (background/accessory app can't re-activate another app) | yes (`_NET_ACTIVE_WINDOW` client msg) | n/a |
| overlay non-focus mechanism | `focusable:false` | **`NSPanel` conversion (tauri-nspanel)** — `focusable:false` is BROKEN | WM hints (`focusable:false`/window-type) | compositor-dependent |
| `ensure_focus()` | verify + restore + fallback | **verify-only + fallback** | verify + restore + fallback | **clipboard-only + toast** |

**Key:** verify is universal; restore is conditional; the clipboard+toast
fallback (§6.6) is shared. macOS's "no restore" is the OS contract — mitigated
upstream by the non-activating `NSPanel` (focus never leaves the user's app, so
a mismatch only means the user ⌘-tabbed mid-dictation → safe to refuse).

## macOS findings

- **`focusable:false` is BROKEN** — tauri#14102 / tao#1210 (open, tao 0.34.2,
  macOS 15.6): the overlay *steals* focus. Tauri `setFocusable` docs confirm
  "on macOS, already focused windows cannot be unfocused via this method."
- **Fix (two parts, both required — verbatim, Unwait 2026-08-07):**
  (a) window can't become key, AND (b) app can't become active on click.
  Implementation = convert the webview window to an `NSPanel` via the
  **`tauri-nspanel` plugin**: `to_panel()`, `set_style_mask(StyleMask::nonactivating_panel)`,
  `set_hides_on_deactivate(false)` (a non-activating app is permanently
  deactivated, so the panel's default hide-on-deactivate would vanish it),
  `set_level(PanelLevel::Status)`, `full_screen_auxiliary()` (else it doesn't
  appear over full-screen windows). This is a **new macOS-only dependency**.
- **Frontmost app:** `NSWorkspace.shared.frontmostApplication` → `NSRunningApplication`
  `.processIdentifier` (`pid_t`). Needs objc2/objc2-app-kit (small FFI; arboard +
  enigo already pull objc2).
- **Re-activate another app: effectively impossible.** `NSRunningApplication.activate(options:)`
  "returns false if … not a type of application that can be activated"; the
  legacy Carbon `SetFrontProcess` is deprecated/fragile. → macOS guard =
  verify-only.
- **enigo keystroke target:** `CGEvent.post(CGEventTapLocation::HID)` → routed
  to the currently-active/key app's event stream (no window targeting). So if
  the non-activating overlay kept focus on the user's app, injected ⌘V lands
  there. **enigo macOS also requires Accessibility permission**
  (`AXIsProcessTrustedWithOptions`) — request at first run, separate from mic.

## Linux/X11 findings

- **Overlay:** more controllable than macOS; WM-dependent. An overlay typed
  `_NET_WM_WINDOW_TYPE_{NOTIFICATION,DOCK,TOOLTIP,SPLASH}` + skip-taskbar/pager
  typically does NOT take keyboard focus. tao honors `focusable`/`setSkipTaskbar`
  on X11 (unlike macOS). enigo injects via `xtest_fake_input` → the X-server
  keyboard-focus window.
- **Active window:** EWMH root property `_NET_ACTIVE_WINDOW` → window id; that
  window's `_NET_WM_PID` → owning PID (best-effort; not every window sets it).
  Via x11rb (no new dep — enigo pulls x11rb).
- **Restore:** send a `_NET_ACTIVE_WINDOW` client message to the root window
  (`message_type=_NET_ACTIVE_WINDOW, format=32, data=[2, CurrentTime, 0, 0, target]`)
  — the WM honors it. Lower-level `XSetInputFocus`/`XRaiseWindow` are WM-rude.

## Wayland (brief)

- **No foreground/active-window API** — compositors hide global state. →
  `capture_target()` structurally `None`; guard degrades to clipboard-only +
  toast (the §6.6 safe fallback). Hotkey is the harder blocker (spec blocker #1).

## Recommended portable type (no new abstraction)

Keep the existing `Option<isize>` handle (matches spec D2: inline cfg, no
`mod platform`, no trait). Opaque-per-platform: HWND / pid_t / X-window-id /
None.

```rust
// paste.rs — inline per D2; no mod platform, no trait, no runtime branch
pub fn capture_target() -> Option<isize> {
    #[cfg(target_os = "windows")] { /* GetForegroundWindow HWND.0 */ }
    #[cfg(target_os = "macos")]   { /* NSWorkspace.frontmostApplication.processIdentifier */ }
    #[cfg(target_os = "linux")]   { /* _NET_ACTIVE_WINDOW via x11rb; None on Wayland */ }
    #[cfg(not(any(/*…*/)))]       { None }
}

fn ensure_focus(t: isize) -> Result<()> {
    if foreground_is(t) { return Ok(()); }            // verify (all)
    #[cfg(target_os = "windows")] { /* SetForegroundWindow; re-check */ }
    #[cfg(target_os = "linux")]   { /* _NET_ACTIVE_WINDOW client msg; re-check */ }
    #[cfg(target_os = "macos")]   { /* NO restore → fall through to fallback */ }
    if foreground_is(t) { Ok(()) } else { /* leave on clipboard + Err (§6.6) */ }
}
```

## Port-decision inputs forced by this spike

1. **macOS needs `tauri-nspanel`** (new native dep) — without it the overlay
   steals focus and the §6.6 guard can't rely on "focus never changed."
2. **macOS paste modifier = `Key::Command` (⌘V), not `Key::Control`.** Linux/X11
   stays `Key::Control` (Ctrl+V). Windows stays `Key::Other(0x56)`.
3. **enigo macOS Accessibility permission** — first-run prompt, separate from mic.

## Sources

- enigo source (master): `src/macos/macos_impl.rs` (CGEvent `.post(HID)`,
  `AXIsProcessTrustedWithOptions`, `Key::Other`→raw keycode);
  `src/linux/x11rb.rs` (`xtest_fake_input` to `screen.root`);
  `src/linux/mod.rs` (wayland/x11/libei dispatch, opt-in features).
- Tauri docs (ctx7 `/websites/v2_tauri_app`): `setFocusable` (macOS no-op note),
  `setSkipTaskbar` (unsupported macOS), `accept_first_mouse` (v1.2).
- tauri#14102 / tao#1210 — `focusable:false` steals focus on macOS (open).
- Unwait, "Building a macOS overlay that never steals focus" (2026-08-07) —
  `canBecomeKeyWindow=false` + `NSWindowStyleMaskNonactivatingPanel`, both required.
- Apple: `NSRunningApplication.activate(options:)`, `NSWorkspace.frontmostApplication`,
  `LSUIElement`.
- EWMH: `_NET_ACTIVE_WINDOW` (root prop) + `_NET_WM_PID` via x11rb.
