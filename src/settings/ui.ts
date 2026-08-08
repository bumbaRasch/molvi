// Component kit — spec §7.3 + Decision Log D21 (vanilla TS, no framework).
// Each helper returns an HTMLElement; stateful controls also return get/set.
// Self-labeled controls (Toggle/Select/TextInput/Slider/Textarea) render their
// own <label>; drop them straight into a SettingsGroup.

import { t } from "../i18n";

interface ToggleCtrl { wrap: HTMLElement; set: (v: boolean) => void; get: () => boolean; }
export interface SelectCtrl { wrap: HTMLElement; set: (v: string) => void; get: () => string; }
interface TextCtrl { wrap: HTMLElement; set: (v: string) => void; get: () => string; }
interface SliderCtrl { wrap: HTMLElement; set: (v: number) => void; get: () => number; }

export function Toggle(label: string, checked: boolean, onChange: (v: boolean) => void, tip?: string): ToggleCtrl {
  // <label> wraps input+text → clicking the text toggles the checkbox (a11y).
  const wrap = document.createElement("label");
  wrap.className = "toggle-wrap";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  input.addEventListener("change", () => onChange(input.checked));
  const text = document.createElement("span");
  text.textContent = label;
  if (tip) text.append(InfoTip(tip));
  wrap.append(input, text);
  return { wrap, set: (v: boolean) => { input.checked = v; }, get: () => input.checked };
}

export function Select(
  label: string,
  options: { value: string; label: string }[],
  value: string,
  onChange: (v: string) => void,
): SelectCtrl {
  const wrap = document.createElement("label");
  wrap.className = "field-wrap";
  const lab = document.createElement("span");
  lab.className = "field-label";
  lab.textContent = label;
  const sel = document.createElement("select");
  for (const o of options) {
    const opt = document.createElement("option");
    opt.value = o.value;
    opt.textContent = o.label;
    sel.appendChild(opt);
  }
  sel.value = value;
  sel.addEventListener("change", () => onChange(sel.value));
  wrap.append(lab, sel);
  return { wrap, set: (v: string) => { sel.value = v; }, get: () => sel.value };
}

export function TextInput(
  label: string,
  value: string,
  onChange: (v: string) => void,
  opts?: { placeholder?: string; type?: string },
): TextCtrl {
  const wrap = document.createElement("label");
  wrap.className = "field-wrap";
  const lab = document.createElement("span");
  lab.className = "field-label";
  lab.textContent = label;
  const input = document.createElement("input");
  input.type = opts?.type ?? "text";
  input.value = value;
  if (opts?.placeholder) input.placeholder = opts.placeholder;
  input.addEventListener("input", () => onChange(input.value));
  wrap.append(lab, input);
  return { wrap, set: (v: string) => { input.value = v; }, get: () => input.value };
}

export function Textarea(label: string, value: string, onChange: (v: string) => void): TextCtrl {
  // Multi-line sibling of TextInput (R11: the kit had no Textarea; added here
  // ≤ the kit's style so the Text section's Prompt field matches the controls).
  const wrap = document.createElement("label");
  wrap.className = "field-wrap";
  const lab = document.createElement("span");
  lab.className = "field-label";
  lab.textContent = label;
  const ta = document.createElement("textarea");
  ta.rows = 4;
  ta.value = value;
  ta.addEventListener("input", () => onChange(ta.value));
  wrap.append(lab, ta);
  return { wrap, set: (v: string) => { ta.value = v; }, get: () => ta.value };
}

export function Slider(
  label: string,
  value: number,
  min: number,
  max: number,
  step: number,
  onChange: (v: number) => void,
  tip?: string,
): SliderCtrl {
  const wrap = document.createElement("label");
  wrap.className = "field-wrap";
  const lab = document.createElement("span");
  lab.className = "field-label";
  lab.textContent = label;
  if (tip) lab.append(InfoTip(tip));
  // <output> is the MDN-recommended element for a range input's live value
  // (result of the user's drag). Format to the slider's step precision so the
  // energy-threshold slider (step 0.001) is actually readable.
  const decimals = step < 1 ? (String(step).split(".")[1] ?? "").length : 0;
  const fmt = (v: number): string => v.toFixed(decimals);
  const out = document.createElement("output");
  out.className = "slider-value";
  out.textContent = fmt(value);
  const input = document.createElement("input");
  input.type = "range";
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.value = String(value);
  input.addEventListener("input", () => {
    const v = parseFloat(input.value);
    out.textContent = fmt(v);
    onChange(v);
  });
  const row = document.createElement("div");
  row.className = "slider-row";
  row.append(input, out);
  wrap.append(lab, row);
  return { wrap, set: (v: number) => { input.value = String(v); out.textContent = fmt(v); }, get: () => parseFloat(input.value) };
}

export function SettingsGroup(title: string, children: HTMLElement[], tip?: string): HTMLElement {
  const group = document.createElement("section");
  group.className = "settings-group";
  const h = document.createElement("h3");
  h.textContent = title;
  // Optional ⓘ on the section title — the single mechanism for section-header
  // tooltips, so callers don't hunt the h3 in each section builder.
  if (tip) h.append(InfoTip(tip));
  group.append(h, ...children);
  return group;
}

export function Button(
  label: string,
  onClick: () => void,
  opts?: { variant?: "primary" | "destructive" },
): HTMLButtonElement {
  const btn = document.createElement("button");
  btn.type = "button"; // best-practice: prevents accidental form submit if ever inside a <form>
  btn.className = "btn" + (opts?.variant ? ` ${opts.variant}` : "");
  btn.textContent = label;
  btn.addEventListener("click", onClick);
  return btn;
}

export function Alert(kind: "info" | "warning" | "error", text: string): HTMLElement {
  const el = document.createElement("div");
  el.className = `alert ${kind}`;
  el.textContent = text;
  return el;
}

// ⓘ icon with a CSS-driven hover/focus tooltip bubble. Always-visible (unlike
// a conditional Alert), so it suits option caveats that live on the option
// they describe. Accessible: the icon is role="img" with aria-label (AT path);
// focusable (tabindex 0) → keyboard reveals the bubble. The bubble is
// role="tooltip" + aria-hidden (visual-only; text is real DOM, wraps, copies).
export function InfoTip(text: string): HTMLElement {
  const tip = document.createElement("span");
  tip.className = "info-tip";
  tip.tabIndex = 0;
  tip.setAttribute("role", "img");
  tip.setAttribute("aria-label", text);
  tip.textContent = "\u24D8"; // ⓘ CIRCLED LATIN SMALL LETTER I
  // Click/Enter/Space on the ⓘ must NOT bubble to a parent <summary> (would
  // toggle <details>) or <label> (would focus/activate the control) — the tip
  // is hover/focus-revealed, not a trigger.
  tip.addEventListener("click", (e) => e.stopPropagation());
  tip.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") e.stopPropagation();
  });
  const bubble = document.createElement("span");
  bubble.className = "info-tip-bubble";
  bubble.setAttribute("role", "tooltip");
  bubble.setAttribute("aria-hidden", "true");
  bubble.textContent = text;
  tip.append(bubble);
  return tip;
}

// ── Toaster: transient auto-dismissing notifications (spec §7.3) ──────────
// Toast-B rewires persist.ts + sections to call toast(); Toast-A ships only
// the primitive. Throttle lives at the call site (Toast-B), not here.

type ToastKind = "success" | "info" | "warning" | "error";

const TOAST_MS: Record<ToastKind, number> = {
  success: 4000,
  info: 4000,
  warning: 6000,
  error: 8000,
};
const TOAST_CAP = 3;
const TOAST_EXIT_MS = 200; // exit transition (180ms) + small buffer
// Maps a toast card → its dismiss() so cap-evict can route through it (clears
// the dismiss timer + runs the .leaving exit) instead of a raw remove().
const dismissers = new WeakMap<HTMLElement, () => void>();

function ensureToaster(): HTMLElement {
    const existing = document.getElementById("toaster");
    if (existing) return existing;
    const el = document.createElement("div");
    el.id = "toaster";
    el.className = "toaster";
    document.body.appendChild(el);
    return el;
}

interface ToastAction { label: string; onClick: () => void; }

export function toast(
  kind: ToastKind,
  message: string,
  opts?: { durationMs?: number; action?: ToastAction },
): void {
  const host = ensureToaster();

  const card = document.createElement("div");
  card.className = `toast ${kind}`;
  card.setAttribute("role", kind === "error" ? "alert" : "status");
  card.setAttribute("aria-atomic", "true");

  const msg = document.createElement("span");
  msg.textContent = message;

  const close = document.createElement("button");
  close.type = "button";
  close.className = "toast-close";
  close.setAttribute("aria-label", t("toast.close"));
  close.textContent = "\u00d7";

  // Optional inline action button (e.g. "Undo" on dict delete). Sits between
  // the message and the × close. Focusable -> existing focusin pause keeps the
  // toast alive while the user Tabs to + clicks the action.
  let actionBtn: HTMLButtonElement | null = null;
  if (opts?.action) {
    actionBtn = document.createElement("button");
    actionBtn.type = "button";
    actionBtn.className = "toast-action";
    actionBtn.textContent = opts.action.label;
    actionBtn.addEventListener("click", () => {
      opts!.action!.onClick();
      dismiss();
    });
  }

  card.append(msg, ...(actionBtn ? [actionBtn] : []), close);
  host.prepend(card); // newest on top

  // stack cap: drop oldest (last child) beyond TOAST_CAP. Each toast prepends
  // exactly one card → at most one over cap, so a single eviction suffices
  // (a `while` on childElementCount would spin: dismiss() defers removal for
  // the exit anim, so count never drops in-loop). Route through the evicted
  // card's own dismiss() to clear its timer + run .leaving; an already-leaving
  // card (dismiss is an idempotent no-op) is removed outright to hold the cap.
  if (host.childElementCount > TOAST_CAP) {
    const last = host.lastElementChild as HTMLElement | null;
    if (last) {
      if (last.classList.contains("leaving")) {
        last.remove();
        dismissers.delete(last);
      } else {
        // Every prior card has a registered dismisser (dismissers.set runs
        // synchronously before this toast() returns; the WeakMap entry is
        // deleted only in the .leaving exit-removal path, handled above).
        dismissers.get(last)!();
      }
    }
  }

  let timer = window.setTimeout(dismiss, opts?.durationMs ?? TOAST_MS[kind]);

  function dismiss(): void {
    if (card.classList.contains("leaving")) return; // idempotent
    window.clearTimeout(timer); // clear dismiss timer (natural + cap-evict path)
    card.classList.add("leaving");
    // ponytail: setTimeout removal, NOT transitionend — reduced-motion zeroes
    // transitions (settings.css), so transitionend would never fire and hang.
    window.setTimeout(() => { card.remove(); dismissers.delete(card); }, TOAST_EXIT_MS);
  }
  dismissers.set(card, dismiss);

  const pause = (): void => window.clearTimeout(timer);
  const resume = (): void => {
    timer = window.setTimeout(dismiss, opts?.durationMs ?? TOAST_MS[kind]);
  };

  // Joint hover+focus: only resume when BOTH are gone. Without this, a
  // mouseleave while the close button is still focused (or a focusout while
  // still hovered) would restart the dismiss timer and fire it under the
  // pointer/keyboard focus.
  let hovered = false;
  let focused = false;

  close.addEventListener("click", () => {
    pause();
    dismiss();
  });
  card.addEventListener("mouseenter", () => { hovered = true; pause(); });
  card.addEventListener("mouseleave", () => { hovered = false; if (!focused) resume(); });
  card.addEventListener("focusin", () => { focused = true; pause(); });
  card.addEventListener("focusout", () => { focused = false; if (!hovered) resume(); });

  // enter: render at base (opacity 0), then flip to .shown next frame so the
  // transition runs. double-rAF is the reliable first-paint idiom.
  requestAnimationFrame(() => requestAnimationFrame(() => card.classList.add("shown")));
}
