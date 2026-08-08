import { InfoTip, RadioGroup, SettingsGroup, Textarea, TextInput, Toggle } from "../ui";
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
  // radio group below. Replace = Ctrl+A then Ctrl+V (select-all before paste);
  // its caveat lives on the group label ⓘ.
  const pasteGroup = RadioGroup({
    label: "text.paste_mode",
    tip: "text.paste_replace_hint",
    name: "paste-mode",
    options: PASTE_OPTS,
    value: s.paste_mode,
    labelFor: (o) => PASTE_LABEL[o],
    onChange: (o) => patch((n) => { n.paste_mode = o; }),
  });

  // PostMode radio group — mutual exclusivity is native (shared name).
  const radioGroup = RadioGroup({
    label: "text.post_processing",
    tip: "text.post_processing_tip",
    name: "post-mode",
    options: POST_OPTS,
    value: s.post_processing.mode,
    labelFor: (o) => `text.post_${o}`,
    onChange: (o) => { patch((n) => { n.post_processing.mode = o; }); syncReveal(o); },
  });

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
