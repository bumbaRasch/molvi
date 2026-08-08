import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { asLang, setCurrentLang, t } from "./i18n";

const caption = document.getElementById("caption")!;
const dot = document.getElementById("dot")!;
const timer = document.getElementById("timer")!;
const cancel = document.getElementById("cancel")!;
const actions = document.getElementById("actions")!;

let startedAt = 0;
let timerId: number | null = null;

invoke<{ ui_lang: string }>("get_settings")
  .then((s) => setCurrentLang(asLang(s.ui_lang)))
  .catch((e: unknown) => console.error("overlay get_settings failed", e));

function tick(): void {
  const s = Math.floor((Date.now() - startedAt) / 1000);
  timer.textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// Clear dot phase classes (breathing/ring/check); used on transitions + hide.
function clearDot(): void {
  dot.classList.remove("breathing", "ring", "check");
}

// Show/hide the actions row. `els` = elements to render; pass [] to hide.
function setActions(els: HTMLElement[]): void {
  actions.replaceChildren(...els);
  actions.classList.toggle("hidden", els.length === 0);
}

function stopTimer(): void {
  if (timerId !== null) {
    clearInterval(timerId);
    timerId = null;
  }
}

// Exit inline-edit mode: drop contenteditable + clear the actions row.
function exitEdit(): void {
  caption.contentEditable = "false";
  setActions([]);
}

// ── Edit button (Polished edit-window) ──
// The overlay is focusable:false during recording to keep the paste target
// focused. Edit needs keyboard input: request_edit flips focusable+focus on
// the Rust side, then we make the caption editable and focus it so keystrokes
// land in it. Enter/Esc resolve via confirm_paste/cancel_paste.
async function onEditClick(): Promise<void> {
  try {
    await invoke("request_edit");
  } catch (e) {
    console.error("request_edit:", e);
    return;
  }
  caption.contentEditable = "true";
  caption.focus();
  const hint = document.createElement("span");
  hint.className = "edit-hint";
  hint.textContent = t("ovl.edit_hint");
  setActions([hint]);
}

function onCaptionKey(e: KeyboardEvent): void {
  if (caption.contentEditable !== "true") return;
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    const text = caption.textContent;
    caption.contentEditable = "false";
    setActions([]);
    invoke("confirm_paste", { text }).catch((err: unknown) =>
      console.error("confirm_paste:", err),
    );
  } else if (e.key === "Escape") {
    e.preventDefault();
    caption.contentEditable = "false";
    setActions([]);
    invoke("cancel_paste").catch((err: unknown) =>
      console.error("cancel_paste:", err),
    );
  }
}

// ── Paste-failed recovery ──
function showPasteFailedActions(): void {
  const paste = document.createElement("button");
  paste.className = "btn";
  paste.type = "button";
  paste.textContent = t("ovl.paste_anyway");
  paste.addEventListener("click", () => {
    invoke("paste_anyway").catch((e: unknown) => console.error("paste_anyway:", e));
  });
  const history = document.createElement("button");
  history.className = "btn";
  history.type = "button";
  history.textContent = t("ovl.open_history");
  history.addEventListener("click", () => {
    invoke("open_history").catch((e: unknown) => console.error("open_history:", e));
  });
  setActions([paste, history]);
}

// Order matters: wire listeners before the first show-overlay could arrive.
// Unlisten handles are dropped — this window lives for the whole app process.
void (async (): Promise<void> => {
  await listen("show-overlay", () => {
    caption.textContent = t("ovl.recording");
    startedAt = Date.now();
    if (timerId !== null) clearInterval(timerId);
    timerId = window.setInterval(tick, 250);
    clearDot();
    dot.classList.add("breathing");
    setActions([]);
  });

  await listen<{ text: string }>("stream-text", (e) => {
    caption.textContent = e.payload.text;
  });

  await listen<{ phase: string }>("phase", (e) => {
    const phase = e.payload.phase;
    if (phase === "listening") {
      clearDot();
      dot.classList.add("breathing");
    } else if (phase === "working" || phase === "polishing") {
      clearDot();
      dot.classList.add("ring");
      // Don't clobber a streamed partial — only show the polishing label when
      // the caption has no streamed text yet.
      if (!caption.textContent) caption.textContent = t("ovl.polishing");
    } else if (phase === "success") {
      clearDot();
      dot.classList.add("check");
    }
  });

  // Polished edit-window: the finalize side-thread emits the post-processed
  // text + reveals the Edit button. Smart/Raw never fire this (Decision A).
  await listen<{ text: string }>("edit-ready", (e) => {
    caption.textContent = e.payload.text;
    const edit = document.createElement("button");
    edit.className = "btn";
    edit.type = "button";
    edit.textContent = t("ovl.edit");
    edit.addEventListener("click", () => void onEditClick());
    setActions([edit]);
  });

  await listen<{ level: number }>("mic-level", (e) => {
    // 0..100 from Rust; drive the waveform height placeholder.
    const level = e.payload.level;
    document.documentElement.style.setProperty("--mic", `${Math.min(100, level)}%`);
  });

  // Paste failed (focus mismatch / no target): the finalize side-thread stored
  // the failed text for "Paste anyway" re-paste. Show recovery buttons INSTEAD
  // of the old static caption.
  await listen("paste-failed", () => {
    stopTimer();
    clearDot();
    caption.textContent = t("ovl.paste_failed");
    showPasteFailedActions();
  });

  await listen("hide-overlay", () => {
    stopTimer();
    clearDot();
    exitEdit();
    // Privacy: clear the last transcript from the hidden DOM (§10.1).
    caption.textContent = "";
  });
})();

cancel.addEventListener("click", () => {
  invoke("cancel_operation").catch((e: unknown) => console.error("cancel_operation:", e));
});

caption.addEventListener("keydown", onCaptionKey);
