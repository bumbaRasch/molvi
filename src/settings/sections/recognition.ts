import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, InfoTip, Select, SettingsGroup, Slider, toast } from "../ui";
import { patcher } from "../persist";
import { safeListeners } from "../listen-safe";
import { getCurrentLang, t } from "../../i18n";
import type { ModelStatus, RecognitionMode, SectionBuilder } from "../types";

// Multilingual Nemotron engine id (matches Rust engine dispatch). Drives the
// restart-needed check when the selection diverges from page-open.
const NEMOTRON_ID = "nemotron-3.5-asr-streaming-0.6b";
const GIGAAM_ID = "gigaam-v3-e2e-ctc";

// Localized "1.2 ГБ" / "80 МБ" via Intl unit formatting (stdlib, no deps; the
// unit token localizes too: ГБ/GB/Go). Locale follows the UI language.
function fmtBytes(bytes: number): string {
  const unit = bytes >= 1e9 ? "gigabyte" : bytes >= 1e6 ? "megabyte" : bytes >= 1e3 ? "kilobyte" : "byte";
  const div = bytes >= 1e9 ? 1e9 : bytes >= 1e6 ? 1e6 : bytes >= 1e3 ? 1e3 : 1;
  return new Intl.NumberFormat(getCurrentLang(), { style: "unit", unit, maximumFractionDigits: 1 }).format(bytes / div);
}

// 40 Nemotron 3.5 ASR locales (3 tiers from the model card). Endonyms cross-
// referenced from src/i18n/index.ts LANGUAGES; locale variants disambiguated
// by region. Static labels (proper nouns / native names) — NOT via t().
// `adaptation: true` = the 8 fine-tune-needed locales that trigger langWarnAlert.
const NEMOTRON_LANGS: { value: string; label: string; adaptation: boolean }[] = [
  // transcription-ready (19)
  { value: "en-US", label: "English (US)", adaptation: false },
  { value: "en-GB", label: "English (UK)", adaptation: false },
  { value: "es-US", label: "Español (EE. UU.)", adaptation: false },
  { value: "es-ES", label: "Español (España)", adaptation: false },
  { value: "fr-FR", label: "Français (France)", adaptation: false },
  { value: "fr-CA", label: "Français (Canada)", adaptation: false },
  { value: "it-IT", label: "Italiano", adaptation: false },
  { value: "pt-BR", label: "Português (Brasil)", adaptation: false },
  { value: "pt-PT", label: "Português (Portugal)", adaptation: false },
  { value: "nl-NL", label: "Nederlands", adaptation: false },
  { value: "de-DE", label: "Deutsch", adaptation: false },
  { value: "tr-TR", label: "Türkçe", adaptation: false },
  { value: "ru-RU", label: "Русский", adaptation: false },
  { value: "ar-AR", label: "العربية", adaptation: false },
  { value: "hi-IN", label: "हिन्दी", adaptation: false },
  { value: "ja-JP", label: "日本語", adaptation: false },
  { value: "ko-KR", label: "한국어", adaptation: false },
  { value: "vi-VN", label: "Tiếng Việt", adaptation: false },
  { value: "uk-UA", label: "Українська", adaptation: false },
  // broad-coverage (13)
  { value: "pl-PL", label: "Polski", adaptation: false },
  { value: "sv-SE", label: "Svenska", adaptation: false },
  { value: "cs-CZ", label: "Čeština", adaptation: false },
  { value: "nb-NO", label: "Bokmål", adaptation: false },
  { value: "da-DK", label: "Dansk", adaptation: false },
  { value: "bg-BG", label: "Български", adaptation: false },
  { value: "fi-FI", label: "Suomi", adaptation: false },
  { value: "hr-HR", label: "Hrvatski", adaptation: false },
  { value: "sk-SK", label: "Slovenčina", adaptation: false },
  { value: "zh-CN", label: "中文 (简体)", adaptation: false },
  { value: "hu-HU", label: "Magyar", adaptation: false },
  { value: "ro-RO", label: "Română", adaptation: false },
  { value: "et-EE", label: "Eesti", adaptation: false },
  // adaptation-ready (8, need fine-tune)
  { value: "el-GR", label: "Ελληνικά", adaptation: true },
  { value: "lt-LT", label: "Lietuvių", adaptation: true },
  { value: "lv-LV", label: "Latviešu", adaptation: true },
  { value: "mt-MT", label: "Malti", adaptation: true },
  { value: "sl-SI", label: "Slovenščina", adaptation: true },
  { value: "he-IL", label: "עברית", adaptation: true },
  { value: "th-TH", label: "ไทย", adaptation: true },
  { value: "nn-NO", label: "Nynorsk", adaptation: true },
];

const NEMOTRON_BY_VALUE = new Map(NEMOTRON_LANGS.map((l) => [l.value, l]));

// Nemotron accepts only "auto" or one of the 40 locales above. A stale value
// (e.g. Phase-1's bare "ru" persisted before Nemotron shipped) renders the
// Select empty. Coerce to "auto" + persist the correction so it sticks.
const KNOWN_LANGS = new Set<string>(["auto", ...NEMOTRON_LANGS.map((l) => l.value)]);
const validLang = (v: string): string => (KNOWN_LANGS.has(v) ? v : "auto");

// Recognition mode radio group (mirrors the paste-mode radio group in text.ts).
// `command` selects deterministic command-mode grammar parsing (spec §6.2);
// the main hotkey is reused (press-release = PTT semantics, DECISION 5).
const MODE_OPTS: RecognitionMode[] = ["push_to_talk", "toggle", "command"];
const MODE_LABEL: Record<RecognitionMode, string> = {
  push_to_talk: "recognition.mode_ptt",
  toggle: "recognition.mode_toggle",
  command: "recognition.mode_command",
};

export const buildRecognition: SectionBuilder = (store) => {
  const patch = patcher(store);
  const s = store.get().settings!;

  // The engine + target language load ONCE at startup; switching either in
  // Settings does NOT hot-reload (restart required). Capture both at page-
  // open so we can warn the user when their selection diverges from what's
  // actually running.
  const initialModel = s.model;
  const initialLang = s.language;

  // Coerce a stale/unknown language to "auto" once, both for display and
  // persistence — Nemotron would reject a bare "ru" anyway.
  const langValue = validLang(s.language);
  if (langValue !== s.language) {
    patch((n) => { n.language = langValue; });
  }

  // Adaptation-ready locale warning: shown only when the selected language is
  // one of the 8 fine-tune-needed Nemotron locales (and the engine is Nemotron).
  const langWarnAlert = Alert(
    "warning",
    t("recognition.lang_warn"),
  );

  // Nemotron language change needs a restart (no hot-reload). The model-restart
  // case is owned by the picker's `selected` card, so this alert covers ONLY the
  // Nemotron-language-divergence case (see `sync`).
  const restartAlert = Alert(
    "info",
    t("recognition.restart_notice"),
  );

  // Recognition mode is the highest-level choice → radio group at the top.
  // ⓘ on the group label expands the command-mode hint (5 langs + main-hotkey).
  const modeLabel = document.createElement("span");
  modeLabel.className = "field-label";
  modeLabel.textContent = t("recognition.mode_label");
  modeLabel.append(InfoTip(t("command.hint")));
  const modeGroup = document.createElement("div");
  modeGroup.className = "radio-group";
  modeGroup.setAttribute("role", "radiogroup");
  modeGroup.setAttribute("aria-label", t("recognition.mode_label"));
  modeGroup.append(modeLabel);
  for (const o of MODE_OPTS) {
    const lab = document.createElement("label");
    lab.className = "radio-opt";
    const inp = document.createElement("input");
    inp.type = "radio";
    inp.name = "recognition-mode";
    inp.value = o;
    inp.checked = s.recognition_mode === o;
    inp.addEventListener("change", () => {
      if (!inp.checked) return;
      patch((n) => { n.recognition_mode = o; });
    });
    const sp = document.createElement("span");
    sp.textContent = t(MODE_LABEL[o]);
    lab.append(inp, sp);
    modeGroup.append(lab);
  }

  const langSel = Select(
    t("recognition.lang_label"),
    [
      { value: "auto", label: t("recognition.lang_auto") },
      ...NEMOTRON_LANGS.map((l) => ({ value: l.value, label: l.adaptation ? `${l.label} (β)` : l.label })),
    ],
    langValue,
    (v) => {
      patch((n) => { n.language = v; });
      sync(currentModel, v);
    },
  );

  // Live current selection (mirrors the old engine Select.get()); updated on
  // each card select.
  let currentModel = s.model;

  // One source of truth for the conditional visibilities (init + changes). The
  // picker's `selected` card owns the model-restart UI (text + Restart button),
  // so this alert covers ONLY the Nemotron-language-divergence case — avoids a
  // duplicate restart prompt when the model itself changed.
  function sync(model: string, lang: string): void {
    const nem = model === NEMOTRON_ID;
    langSel.wrap.classList.toggle("hidden", !nem);
    const adapt = nem && NEMOTRON_BY_VALUE.get(lang)?.adaptation === true;
    langWarnAlert.classList.toggle("hidden", !adapt);
    restartAlert.classList.toggle("hidden", !(nem && lang !== initialLang));
  }
  sync(s.model, s.language);

  // ── Model picker (2 cards: Nemotron / GigaAM) ───────────────────────────
  const MODELS = [
    { id: NEMOTRON_ID, name: "Nemotron", desc: "models.nemotron_desc" },
    { id: GIGAAM_ID, name: "GigaAM", desc: "models.gigaam_desc" },
  ] as const;

  const pickerHost = document.createElement("div");
  pickerHost.className = "model-picker";

  let statuses: Record<string, ModelStatus> = {};
  // Live download state for the card currently downloading (undefined = none).
  let downloading: { id: string; pct: number; bytes: number; total: number } | undefined;

  function refreshStatuses(): void {
    void invoke<ModelStatus[]>("model_status").then((arr) => {
      statuses = Object.fromEntries(arr.map((m) => [m.model_id, m]));
      renderPicker();
    }).catch((e) => console.error("model_status failed", e)); // metadata-only
  }
  refreshStatuses();

  // Card-state logic (spec §3.1):
  //  active    = currentModel === id           (settings.model == this)
  //  loaded    = id === initialModel           (the engine running since page-open)
  //  selected  = active && !loaded             (cached, chosen, restart pending)
  // Invariant: settings.model is only set via the cached `ready` branch choose(),
  // so `selected` always implies cached. Download never changes currentModel.
  function renderPicker(): void {
    pickerHost.replaceChildren();
    for (const m of MODELS) {
      const cached = statuses[m.id]?.cached ?? false;
      const active = currentModel === m.id;
      const loaded = m.id === initialModel;
      const selected = active && !loaded;
      const dl = downloading && downloading.id === m.id ? downloading : undefined;

      const card = document.createElement("div");
      card.className = "model-card" + (active ? " selected" : "");
      const head = document.createElement("div");
      head.className = "model-card-head";
      const nameEl = document.createElement("span");
      nameEl.className = "model-name";
      nameEl.textContent = m.name;
      head.append(nameEl);

      const statusEl = document.createElement("div");
      statusEl.className = "model-card-status";
      if (dl) {
        const bar = document.createElement("div"); bar.className = "progress-bar";
        const fill = document.createElement("div"); fill.className = "progress-fill";
        fill.style.width = `${dl.pct}%`; bar.append(fill);
        const txt = document.createElement("span"); txt.className = "progress-text";
        txt.textContent = t("models.downloading")
          .replace("{bytes}", fmtBytes(dl.bytes))
          .replace("{total}", fmtBytes(dl.total))
          .replace("{pct}", String(dl.pct));
        const cancel = Button(t("models.cancel"), () => void invoke("cancel_model_download").then(() => {
          downloading = undefined;
          refreshStatuses();
        }));
        statusEl.append(bar, txt, cancel);
      } else if (selected) {
        const note = document.createElement("span");
        note.textContent = t("models.restart_to_activate");
        const restart = Button(t("models.restart_btn"), () => void invoke("restart_app"), { variant: "primary" });
        statusEl.append(note, restart);
      } else if (cached) {
        if (active) { // active == loaded (selected handled above)
          const a = document.createElement("span"); a.className = "badge-active";
          a.textContent = "✓ " + t("models.active");
          statusEl.append(a);
        } else {
          // ready (cached, not chosen): whole card selects — keyboard + click.
          card.classList.add("clickable");
          card.tabIndex = 0;
          const choose = (): void => {
            currentModel = m.id;
            patch((n) => { n.model = m.id; });
            sync(m.id, langSel.get());
            renderPicker();
          };
          card.addEventListener("click", choose);
          card.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); choose(); } });
        }
      } else {
        const size = statuses[m.id]?.size_bytes ?? 0;
        const dlBtn = Button(
          t("models.download").replace("{size}", fmtBytes(size)),
          () => void invoke("download_model", { modelId: m.id })
            .catch((e) => toast("error", t("models.download_failed") + (typeof e === "string" ? ": " + e : ""))),
        );
        dlBtn.classList.add("btn-block");
        statusEl.append(dlBtn);
      }
      head.append(statusEl);
      const desc = document.createElement("div"); desc.className = "model-desc";
      desc.textContent = t(m.desc);
      card.append(head, desc);
      pickerHost.append(card);
    }
  }
  // No standalone renderPicker() here: refreshStatuses() above fires the
  // model_status IPC and calls renderPicker() inside its .then() once statuses
  // populate. A synchronous render now would see statuses={} → every card
  // cached=false → the active/loaded engine would flash a "Download" button.
  // Empty-for-<1-frame (local IPC) beats wrong states; no skeleton (ponytail).

  // Event wiring (TOCTOU-safe, generalized from microphone.ts: cleanup may run
  // before a listen() resolves → the disposed flag drops the just-resolved fn
  // instead of leaking it). All listeners torn down on section teardown.
  const listeners = safeListeners();
  listeners.on<{ model: string; bytes: number; total: number; pct: number }>("model-download-progress", (p) => {
    downloading = { id: p.model, pct: p.pct, bytes: p.bytes, total: p.total };
    renderPicker();
  });
  listeners.on<{ model: string }>("model-download-complete", () => {
    downloading = undefined;
    toast("success", t("models.download_complete"));
    refreshStatuses(); // re-fetch cached flags → card becomes ready/active
  });
  listeners.on<{ model: string }>("model-download-failed", () => {
    downloading = undefined;
    toast("error", t("models.download_failed"));
    renderPicker();
  });

  // ponytail: slider ranges are sensible bounds around the VAD defaults; not
  // spec-locked — widen if a real acoustic profile needs it.
  const vadMin = Slider(t("recognition.vad_min"), s.vad.min_chunk_secs, 0.1, 5, 0.1, (v) => patch((n) => { n.vad.min_chunk_secs = v; }), t("recognition.vad_min_tip"));
  const vadMax = Slider(t("recognition.vad_max"), s.vad.max_chunk_secs, 5, 60, 1, (v) => patch((n) => { n.vad.max_chunk_secs = v; }), t("recognition.vad_max_tip"));
  const vadPad = Slider(t("recognition.vad_pad"), s.vad.padding_secs, 0, 1, 0.05, (v) => patch((n) => { n.vad.padding_secs = v; }), t("recognition.vad_pad_tip"));
  const vadEnergy = Slider(t("recognition.vad_energy"), s.vad.energy_threshold, 0, 0.1, 0.001, (v) => patch((n) => { n.vad.energy_threshold = v; }), t("recognition.vad_energy_tip"));

  // Collapsed Advanced via native <details> (zero JS).
  const advanced = document.createElement("details");
  advanced.className = "advanced";
  const summary = document.createElement("summary");
  summary.textContent = t("recognition.advanced");
  summary.append(InfoTip(t("recognition.advanced_tip")));
  advanced.append(summary, vadMin.wrap, vadMax.wrap, vadPad.wrap, vadEnergy.wrap);

  const group = SettingsGroup(t("recognition.title"), [modeGroup, pickerHost, langSel.wrap, langWarnAlert, restartAlert, advanced], t("recognition.title_tip"));

  const root = document.createElement("div");
  root.append(group);
  return {
    el: root,
    cleanup: () => listeners.cleanup(),
  };
};
