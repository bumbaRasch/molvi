import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { LANGUAGES, asLang, getCurrentLang, setCurrentLang, t } from "../i18n";
import { ICONS } from "./icons";
import { patcher } from "./persist";
import { Store } from "./store";
import { mountFederatedSearch, refreshSearchLang } from "./federated-search";
import type { Section, SectionBuilder, Settings, State } from "./types";
import { mountToaster } from "./ui";
import { buildAbout } from "./sections/about";
import { buildDictionary } from "./sections/dictionary";
import { buildHistory } from "./sections/history";
import { buildHotkey } from "./sections/hotkey";
import { buildMicrophone } from "./sections/microphone";
import { buildOverlay } from "./sections/overlay";
import { buildRecognition } from "./sections/recognition";
import { buildText } from "./sections/text";
import { buildUpdates } from "./sections/updates";

export const store = new Store<State>({ settings: null });
const patch = patcher(store);

const SECTIONS = [
  { id: "recognition", icon: ICONS.recognition },
  { id: "microphone",  icon: ICONS.microphone  },
  { id: "text",        icon: ICONS.text        },
  { id: "dictionary",  icon: ICONS.dictionary  },
  { id: "history",     icon: ICONS.history     },
  { id: "hotkey",      icon: ICONS.hotkey      },
  { id: "overlay",     icon: ICONS.overlay     },
  { id: "updates",     icon: ICONS.updates     },
  { id: "about",       icon: ICONS.about       },
] as const;

const BUILDERS: Record<string, SectionBuilder> = {
  recognition: buildRecognition,
  microphone: buildMicrophone,
  text: buildText,
  dictionary: buildDictionary,
  history: buildHistory,
  hotkey: buildHotkey,
  overlay: buildOverlay,
  updates: buildUpdates,
  about: buildAbout,
};

let current: Section | null = null;
let currentId = "recognition";

export function selectSection(id: string): void {
  currentId = id;
  // Toggle .selected on sidebar items; render the section content.
  document.querySelectorAll("#sidebar .item").forEach((el) => {
    el.classList.toggle("selected", el.getAttribute("data-id") === id);
  });

  // R5: call the previous section's cleanup (e.g. Microphone mic-level unlisten).
  current?.cleanup?.();
  current = null;

  const content = document.getElementById("content")!;

  // No H2 section title — the sidebar highlights the active section (.selected)
  // and each section's own SettingsGroup carries its title.
  const builder = BUILDERS[id];
  const section: Section = !store.get().settings
    ? { el: loadErrorEl() }
    : builder
      ? builder(store)
      : { el: document.createElement("div") };
  current = section;
  content.replaceChildren(section.el);
}

function loadErrorEl(): HTMLElement {
  const p = document.createElement("p");
  p.className = "muted";
  p.textContent = t("common.load_error");
  return p;
}

const langLabel = document.createElement("span");
const langSelect = document.createElement("select");

function buildSidebar(): void {
  const nav = document.getElementById("sidebar")!;
  nav.prepend(mountFederatedSearch(store, selectSection)); // Task 11: federated search at the top
  for (const s of SECTIONS) {
    const btn = document.createElement("button");
    btn.className = "item";
    btn.dataset.id = s.id;
    btn.innerHTML = `${s.icon}<span>${t(`nav.${s.id}`)}</span>`;
    btn.addEventListener("click", () => selectSection(s.id));
    nav.appendChild(btn);
  }

  langLabel.textContent = t("nav.language");
  for (const { code, label } of LANGUAGES) {
    const opt = document.createElement("option");
    opt.value = code;
    opt.textContent = label;
    langSelect.append(opt);
  }
  langSelect.value = getCurrentLang();
  langSelect.addEventListener("change", () => {
    const code = asLang(langSelect.value);
    patch((n) => { n.ui_lang = code; });
    setCurrentLang(code);
    rerender();
  });

  const langRow = document.createElement("div");
  langRow.className = "lang-row";
  langRow.append(langLabel, langSelect);
  nav.append(langRow);
}

// Re-translate every sidebar label + the lang row, then re-render the active
// section via selectSection() (no duplicated section-building logic).
function rerender(): void {
  refreshSearchLang(); // Task 11: rebuild section index + re-translate placeholder
  document.querySelectorAll<HTMLElement>("#sidebar .item").forEach((el) => {
    const span = el.querySelector("span");
    if (span) span.textContent = t(`nav.${el.dataset.id}`);
  });
  langLabel.textContent = t("nav.language");
  langSelect.value = getCurrentLang();
  selectSection(currentId);
}

void (async (): Promise<void> => {
  try {
    const s = await invoke<Settings>("get_settings");
    store.set({ settings: s });
    setCurrentLang(asLang(s.ui_lang));
  } catch (e) {
    console.error("get_settings failed", e);  // metadata-only: error object only
  }
  buildSidebar();
  mountToaster();
  selectSection("recognition");
  void listen("navigate-history", () => selectSection("history"));
  // An external writer (onboarding) changed ui_lang — re-sync the store (a
  // later save would otherwise revert it) and re-localize the whole UI. The
  // settings webview loads at startup with the startup ui_lang and never
  // re-inits, so without this it stays stuck on the first-launch language.
  void listen("ui-lang-changed", async () => {
    try {
      const s = await invoke<Settings>("get_settings");
      store.set({ settings: s });
      setCurrentLang(asLang(s.ui_lang));
      rerender();
    } catch (e) {
      console.error("ui-lang-changed re-fetch failed", e);
    }
  });
})();
