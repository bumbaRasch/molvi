// DRY hotkey-capture helper (Task 10 D4). Lifted verbatim from
// sections/hotkey.ts so onboarding Step 2 reuses the exact same capture path.
// NO new IPC command — capture is pure frontend keydown; the persisted combo
// flows through the existing `set_settings` (which already calls
// `hotkey::rebind` on the Rust side).

// R6: serialize to the vocabulary Rust's binding.parse expects
// ("Alt+`", "Ctrl+Space", "Alt+Shift+F2", "Ctrl+Alt+`").
function serialize(ev: KeyboardEvent): string {
  const mods: string[] = [];
  if (ev.ctrlKey) mods.push("Ctrl");
  if (ev.altKey) mods.push("Alt");
  if (ev.shiftKey) mods.push("Shift");
  if (ev.metaKey) mods.push("Super");
  let key = ev.key;
  if (key === " ") key = "Space";
  else if (key.length === 1) key = key.toLowerCase();
  // F1-F12, digits, punctuation, backtick: pass through as-is.
  return [...mods, key].join("+");
}

export interface CaptureOpts {
  /** Fired with the captured combo (never a bare modifier). */
  onCombo: (combo: string) => void;
  /** Fired when the user cancels (Esc) or `cancel()` is called while armed. */
  onCancel?: () => void;
  /** Optional state hook so callers can update button labels on arm/disarm. */
  onStateChange?: (armed: boolean) => void;
}

export interface CaptureHandle {
  /** Arm: listen for the next non-modifier keypress. Idempotent. */
  start: () => void;
  /** Disarm: cancel the in-flight capture (Esc equivalent). Idempotent. */
  cancel: () => void;
  /** Remove the keydown listener entirely — call on section/step leave. */
  cleanup: () => void;
}

// ponytail: capturing stays an internal flag (idle listener with a guard), not
// an add/remove dance — mirrors the pre-extract hotkey.ts shape (proven). The
// listener is always registered while the handle is alive; only `armed` flips.
export function captureHotkey(opts: CaptureOpts): CaptureHandle {
  let armed = false;
  const onKey = (ev: KeyboardEvent): void => {
    if (!armed) return;
    ev.preventDefault();
    ev.stopPropagation();
    if (ev.key === "Escape") {
      cancel();
      return;
    }
    // Ignore bare modifier presses — wait for the actual key.
    if (ev.key === "Control" || ev.key === "Alt" || ev.key === "Shift" || ev.key === "Meta") {
      return;
    }
    const combo = serialize(ev);
    opts.onCombo(combo);
    // Disarm after a successful capture so a second combo requires a re-arm.
    armed = false;
    opts.onStateChange?.(false);
  };
  function start(): void {
    armed = true;
    opts.onStateChange?.(true);
  }
  function cancel(): void {
    if (!armed) return;
    armed = false;
    opts.onStateChange?.(false);
    opts.onCancel?.();
  }
  window.addEventListener("keydown", onKey, true);
  return {
    start,
    cancel,
    cleanup: () => window.removeEventListener("keydown", onKey, true),
  };
}
