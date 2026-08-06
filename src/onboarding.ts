// Onboarding first-run dialog (Task 10). 3-step skippable state machine:
//   1. Welcome + indeterminate model download + privacy anchor → auto-advance
//      on `engine-ready`; `engine-error` swaps the primary for [Open settings].
//   2. Hotkey capture (reuses the DRY helper) + mic-test breathing dot.
//   3. Real first-word: onboarding-practice routes the post-processed result
//      back here via `practice-result` (no paste) + teal check + all-set copy.
// Per-step Skip always visible (top-right); Esc = Skip; Enter = primary.
// 36-language via the shared i18n dictionary. Privacy §10.1: the practice text
// crosses the IPC bus only — never logged.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { asLang, setCurrentLang, t, LANGUAGES } from "./i18n";
import { captureHotkey } from "./settings/hotkey-capture";
import type { Settings } from "./settings/types";

// ── State ──
let currentStep: 1 | 2 | 3 = 1;
let settings: Settings | null = null;
let engineReady = false;
let micLevelUnlisten: UnlistenFn | null = null;
let streamUnlisten: UnlistenFn | null = null;
let resultUnlisten: UnlistenFn | null = null;

// ── i18n ──
function applyTranslations(): void {
  // Top row (always visible) + bottom language-picker aria-label.
  document.querySelector<HTMLElement>(".step-indicator")!.textContent = t("onboarding.step").replace("{n}", String(currentStep));
  document.getElementById("skip")!.textContent = t("onboarding.skip");
  document.getElementById("ui-lang")!.setAttribute("aria-label", t("nav.language"));

  // Step 1.
  document.getElementById("welcome-title")!.textContent = t("onboarding.welcome");
  document.getElementById("privacy-lead")!.textContent = t("onboarding.privacy_lead");
  document.getElementById("downloading")!.textContent = t("onboarding.downloading");
  document.getElementById("model-explainer")!.textContent = t("onboarding.model_explainer");
  document.getElementById("continue-1")!.textContent = t("onboarding.continue");
  document.getElementById("open-settings-error")!.textContent = t("onboarding.open_settings");

  // Step 2.
  document.getElementById("hotkey-title")!.textContent = t("onboarding.hotkey_title");
  document.getElementById("hotkey-hint")!.textContent = t("onboarding.hotkey_hint");
  document.getElementById("hotkey-capture")!.textContent = t("onboarding.hotkey_capture");
  document.getElementById("mic-hint")!.textContent = t("onboarding.mic_hint");
  document.getElementById("mic-heard")!.textContent = t("onboarding.mic_heard");
  document.getElementById("continue-2")!.textContent = t("onboarding.continue");

  // Step 3.
  document.getElementById("first-word-title")!.textContent = t("onboarding.first_word_title");
  document.getElementById("first-word-hint")!.textContent = t("onboarding.first_word_hint");
  document.getElementById("preparing")!.textContent = t("onboarding.preparing");
  document.getElementById("open-settings-done")!.textContent = t("onboarding.open_settings");
  document.getElementById("done")!.textContent = t("onboarding.done");
}

// ── Step transitions ──
function showStep(n: 1 | 2 | 3): void {
  currentStep = n;
  document.querySelectorAll<HTMLElement>(".step").forEach((el) => el.classList.add("hidden"));
  document.querySelector<HTMLElement>(`section.step[data-step="${n}"]`)!.classList.remove("hidden");
  document.querySelectorAll<HTMLElement>(".seg").forEach((el) => {
    const i = Number(el.dataset.i);
    el.classList.toggle("done", i < n);
    el.classList.toggle("active", i === n);
  });
  document.querySelector<HTMLElement>(".step-indicator")!.textContent = t("onboarding.step").replace("{n}", String(n));
  // Step enter/leave side-effects (mic-preview + listeners). Errors are
  // swallowed so a transient invoke failure doesn't block navigation.
  if (n === 2) void enterStep2();
  else void leaveStep2();
  if (n === 3) void enterStep3();
  else void leaveStep3();
  // Move focus to the new step's primary so Enter continues to work.
  document.querySelector<HTMLButtonElement>(`section.step[data-step="${n}"] .btn.primary`)?.focus();
}

// ── Step 1: download + engine-ready/error ──
async function onEngineReady(): Promise<void> {
  engineReady = true;
  document.getElementById("preparing")!.classList.add("hidden");
  if (currentStep === 1) showStep(2);
}

function onEngineError(): void {
  if (currentStep !== 1) return;
  const engineError = document.getElementById("engine-error")!;
  engineError.textContent = t("onboarding.engine_error");
  engineError.classList.remove("hidden");
  // Continue can't proceed (engine failed); swap primary for Open settings.
  document.getElementById("continue-1")!.classList.add("hidden");
  document.getElementById("open-settings-error")!.classList.remove("hidden");
}

// ── Step 2: hotkey capture + mic test ──
const captureHandle = captureHotkey({
  onCombo: (combo) => {
    document.getElementById("hotkey-captured")!.textContent = combo;
    void persist((s) => { s.hotkey = combo; });
  },
});

// Persist a one-field settings patch (hotkey, ui_lang, …). Clone → patch →
// set_settings (which live-applies hotkey/tray/dir on the Rust side) → keep the
// local snapshot in sync. Mirrors persist.ts's patcher at the IPC level.
async function persist(patch: (s: Settings) => void): Promise<void> {
  if (!settings) return;
  const next = structuredClone(settings);
  patch(next);
  try {
    await invoke("set_settings", { settings: next });
    settings = next;
  } catch (e) {
    console.error("set_settings failed", e);
  }
}

async function enterStep2(): Promise<void> {
  await invoke("set_mic_preview", { enabled: true }).catch((e) => console.error("set_mic_preview", e));
  micLevelUnlisten = await listen<{ level: number }>("mic-level", (e) => {
    // Rust sends RMS×1000 (0..~100 for normal speech). Normalize to 0..1 for
    // the dot scale; ~15+ = "Hearing you." threshold.
    const level = e.payload.level;
    const norm = Math.min(1, level / 80);
    document.documentElement.style.setProperty("--mic", norm.toFixed(3));
    document.getElementById("mic-heard")!.classList.toggle("hidden", level < 15);
  });
}

async function leaveStep2(): Promise<void> {
  if (currentStep !== 2 && micLevelUnlisten) {
    micLevelUnlisten();
    micLevelUnlisten = null;
    await invoke("set_mic_preview", { enabled: false }).catch((e) => console.error("set_mic_preview", e));
  }
}

// ── Step 3: onboarding-practice first-word ──
const CHECK_SVG = `<svg viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="5 12 10 17 19 8"></polyline></svg>`;

async function enterStep3(): Promise<void> {
  // Engine-ready gate: if the bg thread is still spawning (rare on step 3 —
  // step 1 already waited), show "Preparing…". The listener clears it.
  if (!engineReady) document.getElementById("preparing")!.classList.remove("hidden");
  await invoke("set_onboarding_practice", { enabled: true }).catch((e) => console.error("set_onboarding_practice", e));
  await invoke("set_mic_preview", { enabled: true }).catch((e) => console.error("set_mic_preview", e));
  streamUnlisten = await listen<{ text: string }>("stream-text", (e) => {
    document.getElementById("caption")!.textContent = e.payload.text;
  });
  resultUnlisten = await listen<{ text: string }>("practice-result", (e) => {
    onPracticeResult(e.payload.text);
  });
}

async function leaveStep3(): Promise<void> {
  if (currentStep !== 3) {
    if (streamUnlisten) { streamUnlisten(); streamUnlisten = null; }
    if (resultUnlisten) { resultUnlisten(); resultUnlisten = null; }
    await invoke("set_onboarding_practice", { enabled: false }).catch((e) => console.error("set_onboarding_practice", e));
    await invoke("set_mic_preview", { enabled: false }).catch((e) => console.error("set_mic_preview", e));
  }
}

function onPracticeResult(text: string): void {
  document.getElementById("caption")!.textContent = text;
  const dot = document.getElementById("dot-practice")!;
  dot.classList.remove("breathing");
  dot.classList.add("check");
  dot.innerHTML = CHECK_SVG;
  document.getElementById("preparing")!.classList.add("hidden");
  const allSet = document.getElementById("all-set")!;
  allSet.textContent = t("onboarding.all_set").replace("{hotkey}", settings?.hotkey ?? "");
  allSet.classList.remove("hidden");
}

// ── Complete (Done/Skip/Open-settings) ──
async function complete(): Promise<void> {
  captureHandle.cleanup();
  // Force-leave mic/practice regardless of current step (belt-and-suspenders;
  // complete_onboarding also resets server-side).
  if (micLevelUnlisten) { micLevelUnlisten(); micLevelUnlisten = null; }
  if (streamUnlisten) { streamUnlisten(); streamUnlisten = null; }
  if (resultUnlisten) { resultUnlisten(); resultUnlisten = null; }
  await invoke("set_onboarding_practice", { enabled: false }).catch(() => undefined);
  await invoke("set_mic_preview", { enabled: false }).catch(() => undefined);
  await invoke("complete_onboarding").catch((e) => console.error("complete_onboarding", e));
}

// ── Init ──
async function init(): Promise<void> {
  settings = await invoke<Settings>("get_settings");
  setCurrentLang(asLang(settings.ui_lang));
  applyTranslations();

  // Wire engine-ready/error first — the bg thread may fire either at any time.
  void listen("engine-ready", () => void onEngineReady());
  void listen("engine-error", () => onEngineError());

  // Initial step indicator + rail.
  document.querySelectorAll<HTMLElement>(".seg").forEach((el) => {
    const i = Number(el.dataset.i);
    el.classList.toggle("done", i < 1);
    el.classList.toggle("active", i === 1);
  });

  // UI-language picker (bottom-left). Endonyms are language-neutral; the
  // aria-label localizes via applyTranslations. Change → setCurrentLang (flips
  // RTL dir) + persist ui_lang + re-translate the whole dialog.
  const uiLang = document.getElementById("ui-lang") as HTMLSelectElement;
  for (const { code, label } of LANGUAGES) {
    const opt = document.createElement("option");
    opt.value = code;
    opt.textContent = label;
    uiLang.append(opt);
  }
  uiLang.value = asLang(settings.ui_lang);
  uiLang.addEventListener("change", () => {
    const code = asLang(uiLang.value);
    setCurrentLang(code);
    void persist((s) => { s.ui_lang = code; });
    applyTranslations();
  });

  // Buttons.
  document.getElementById("continue-1")!.addEventListener("click", () => showStep(2));
  document.getElementById("continue-2")!.addEventListener("click", () => showStep(3));
  const hotkeyBtn = document.getElementById("hotkey-capture")!;
  hotkeyBtn.addEventListener("click", () => {
    captureHandle.cancel(); // re-arm: cancel any in-flight, then start fresh
    captureHandle.start();
    // Blur so Enter (primary action) doesn't re-trigger the button while armed.
    hotkeyBtn.blur();
  });
  document.getElementById("done")!.addEventListener("click", () => void complete());
  document.getElementById("skip")!.addEventListener("click", () => void complete());
  document.getElementById("open-settings-error")!.addEventListener("click", () => void complete());
  document.getElementById("open-settings-done")!.addEventListener("click", () => void complete());

  // Global Esc = Skip (the capture helper intercepts Esc while armed, so this
  // only fires when capture is idle — exactly the desired UX). Enter on a
  // focused .btn.primary already triggers it natively (browser default).
  document.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape") {
      ev.preventDefault();
      void complete();
    }
  });
}

init().catch((e) => console.error("onboarding init failed", e));
