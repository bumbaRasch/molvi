import { InfoTip, SettingsGroup, Textarea, TextInput, Toggle } from "../ui";
import type { PasteMode, PostMode, SectionBuilder, SmartSettings } from "../types";
import { patcher } from "../persist";
import { t } from "../../i18n";

const POST_OPTS: PostMode[] = ["raw", "smart", "polished"];
const PASTE_OPTS: PasteMode[] = ["clipboard", "type", "replace"];
// clipboard reuses the existing text.use_clipboard label; type/replace are new.
const PASTE_LABEL: Record<PasteMode, string> = {
  clipboard: "text.use_clipboard",
  type: "text.paste_type",
  replace: "text.paste_replace",
};

const SMART_FIELDS: (keyof SmartSettings)[] = [
  "apply_dictionary",
  "fix_case",
  "normalize_whitespace",
  "cleanup_repeated_marks",
  "merge_chunks",
  "remove_duplicate_words",
  "remove_fillers",
  "inter_chunk_punctuation",
];

export const buildText: SectionBuilder = (store) => {
  const patch = patcher(store);
  const s = store.get().settings!;

  // Paste mode radio group (Clipboard / Type / Replace). Mirrors the PostMode
  // radio group below. Replace = Ctrl+A then Ctrl+V (select-all before paste).
  // The Replace caveat lives on the Paste mode label (ⓘ) so it's visible at
  // the group level, not only when Replace happens to be selected.
  const pasteLabel = document.createElement("span");
  pasteLabel.className = "field-label";
  pasteLabel.textContent = t("text.paste_mode");
  pasteLabel.append(InfoTip(t("text.paste_replace_hint")));
  const pasteGroup = document.createElement("div");
  pasteGroup.className = "radio-group";
  pasteGroup.setAttribute("role", "radiogroup");
  pasteGroup.setAttribute("aria-label", t("text.paste_mode"));
  pasteGroup.append(pasteLabel);
  for (const o of PASTE_OPTS) {
    const lab = document.createElement("label");
    lab.className = "radio-opt";
    const inp = document.createElement("input");
    inp.type = "radio";
    inp.name = "paste-mode";
    inp.value = o;
    inp.checked = s.paste_mode === o;
    inp.addEventListener("change", () => {
      if (!inp.checked) return;
      patch((n) => { n.paste_mode = o; });
    });
    const sp = document.createElement("span");
    sp.textContent = t(PASTE_LABEL[o]);
    lab.append(inp, sp);
    pasteGroup.append(lab);
  }

  // PostMode radio group — mutual exclusivity is native (shared name).
  const radioLabel = document.createElement("span");
  radioLabel.className = "field-label";
  radioLabel.textContent = t("text.post_processing");
  radioLabel.append(InfoTip(t("text.post_processing_tip")));
  const radioGroup = document.createElement("div");
  radioGroup.className = "radio-group";
  radioGroup.setAttribute("role", "radiogroup");
  radioGroup.setAttribute("aria-label", t("text.post_processing"));
  radioGroup.append(radioLabel);
  for (const o of POST_OPTS) {
    const lab = document.createElement("label");
    lab.className = "radio-opt";
    const inp = document.createElement("input");
    inp.type = "radio";
    inp.name = "post-mode";
    inp.value = o;
    inp.checked = s.post_processing.mode === o;
    inp.addEventListener("change", () => {
      if (!inp.checked) return;
      patch((n) => { n.post_processing.mode = o; });
      syncReveal(o);
    });
    const sp = document.createElement("span");
    sp.textContent = t(`text.post_${o}`);
    lab.append(inp, sp);
    radioGroup.append(lab);
  }

  const endpoint = TextInput(
    t("text.endpoint"),
    s.post_processing.endpoint ?? "",
    (v) => patch((n) => { n.post_processing.endpoint = v || null; }),
    { placeholder: t("text.endpoint_ph") },
  );
  const apiKey = TextInput(
    t("text.api_key"),
    s.post_processing.api_key ?? "",
    (v) => patch((n) => { n.post_processing.api_key = v || null; }),
    { type: "password" },
  );
  const model = TextInput(
    t("text.model"),
    s.post_processing.model ?? "",
    (v) => patch((n) => { n.post_processing.model = v || null; }),
    { placeholder: t("text.model_ph") },
  );
  const prompt = Textarea(
    t("text.prompt"),
    s.post_processing.prompt ?? "",
    (v) => patch((n) => { n.post_processing.prompt = v || null; }),
  );
  const polishedWrap = document.createElement("div");
  polishedWrap.className = "reveal";
  polishedWrap.append(endpoint.wrap, apiKey.wrap, model.wrap, prompt.wrap);

  const smartToggles = SMART_FIELDS.map((f) =>
    Toggle(t(`text.smart_${f}`), s.post_processing.smart[f], (v) => patch((n) => { n.post_processing.smart[f] = v; })),
  );
  const smartGrid = document.createElement("div");
  smartGrid.className = "reveal-grid";
  smartGrid.append(...smartToggles.map((tg) => tg.wrap));
  // Visible header above the smart-toggles grid (was a bare grid). Title + grid
  // are wrapped so syncReveal hides the whole block, not just the toggles.
  const smartTitle = document.createElement("div");
  smartTitle.className = "field-label";
  smartTitle.textContent = t("text.smart_title");
  smartTitle.append(InfoTip(t("text.smart_title_tip")));
  const smartWrap = document.createElement("div");
  smartWrap.className = "smart-block";
  smartWrap.append(smartTitle, smartGrid);

  function syncReveal(mode: PostMode): void {
    smartWrap.classList.toggle("hidden", mode !== "smart");
    polishedWrap.classList.toggle("hidden", mode !== "polished");
  }
  syncReveal(s.post_processing.mode);

  const group = SettingsGroup(t("text.title"), [pasteGroup, radioGroup, polishedWrap, smartWrap], t("text.title_tip"));
  const root = document.createElement("div");
  root.append(group);
  return { el: root };
};
