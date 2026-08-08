// The single debounced (300ms trailing) set_settings path. Dictionary /
// check_update / apply_update / list_audio_devices are immediate invokes.

import { invoke } from "@tauri-apps/api/core";

import { t } from "../i18n";
import { toast } from "./ui";
import type { Settings, SettingsStore } from "./types";

let timer: ReturnType<typeof setTimeout> | null = null;
let pending: Settings | null = null;

// Success-toast throttle: dragging a slider spams debounced saves → cap at one
// "Saved" toast per 2s. Errors are NOT throttled (immediate feedback wanted).
let lastSuccessAt = 0;

function persist(store: SettingsStore, next: Settings): void {
  store.set({ settings: next });
  pending = next;
  if (timer) clearTimeout(timer);
  timer = setTimeout(flush, 300);
}

async function flush(): Promise<void> {
  timer = null;
  const p = pending;
  pending = null;
  if (!p) return;
  try {
    await invoke("set_settings", { settings: p });
    if (Date.now() - lastSuccessAt >= 2000) {
      lastSuccessAt = Date.now();
      toast("success", t("common.saved"));
    }
  } catch (e) {
    // metadata-only: MolviError carries settings/db error strings, never text.
    console.error("set_settings failed", e);
    toast("error", t("common.save_failed").replace("{msg}", errText(e)));
  }
}

// Force the pending debounced save (if any) to fire now and await it. Cancels
// the trailing timer so the change is saved exactly once (no double-save).
// Used by discrete actions that need the save to land before a follow-up invoke
// (e.g. history.enabled toggle-ON: Rust opens the History store in set_settings,
// so history_query before the save lands returns []).
export async function flushPending(): Promise<void> {
  if (timer) { clearTimeout(timer); timer = null; }
  await flush();
}

// Extract a short message from a thrown/rejected value. Shared by persist's
// save path and the sections' immediate-invokes (microphone/updates).
export function errText(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  // ponytail: Tauri may reject with a serde object; surface a short repr.
  try {
    return JSON.stringify(e);
  } catch {
    return t("common.unknown_error");
  }
}

// Clone current settings, mutate the clone, persist. The mutation helper every
// section uses. structuredClone keeps nested objects (vad/overlay/…) independent
// of the live store reference so edits don't leak before notify.
export function patcher(
  store: SettingsStore,
): (fn: (s: Settings) => void) => void {
  return (fn): void => {
    const cur = store.get().settings;
    if (!cur) return;
    const next = structuredClone(cur) as Settings;
    fn(next);
    persist(store, next);
  };
}
