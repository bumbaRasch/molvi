// Federated settings search (Task 11). Autocomplete dropdown at the top of the
// sidebar: instant in-memory section matches (synchronous tier) + debounced
// history/dictionary matches (content tier, Task 3). WAI-ARIA combobox pattern
// (role=combobox + listbox + aria-activedescendant; DOM focus stays on input).

import { invoke } from "@tauri-apps/api/core";
import { ICONS } from "./icons";
import type { DictEntry, HistoryRow, SettingsStore } from "./types";
import { getCurrentLang, t } from "../i18n";

// Section ids mirror the sidebar SECTIONS in main.ts (single source of truth =
// that array; duplicated here only to keep this module self-contained — a drift
// would show immediately at smoke). Keywords are language-neutral technical
// terms; titles come from nav.<id> (localized).
const SECTIONS = [
  "recognition", "microphone", "text", "dictionary",
  "history", "hotkey", "overlay", "updates", "about",
] as const;

const KEYWORDS: Record<string, string[]> = {
  recognition: ["vad", "rtf", "model", "engine", "nemotron", "gigaam", "language", "energy", "chunk", "threshold"],
  microphone: ["mic", "device", "noise", "input", "level", "preview"],
  text: ["paste", "clipboard", "replace", "type", "polish", "smart", "raw", "mode"],
  dictionary: ["entry", "replacement", "word", "correct", "expand"],
  history: ["history", "log", "record", "retention", "entries", "days", "erase"],
  hotkey: ["hotkey", "ptt", "push-to-talk", "alt", "shortcut", "altgr"],
  overlay: ["overlay", "caption", "bubble", "edit", "paste-failed"],
  updates: ["update", "version", "check", "download"],
  about: ["about", "credits", "version", "links"],
};

// A renderable result row. `section` is the navigation destination; `icon` is an
// ICONS key (sections only); `text` is the main label; `sub` is a muted detail
// (timestamp for history, omitted otherwise).
interface SearchItem {
  section: string;
  icon?: string;
  text: string;
  sub?: string;
}
interface SearchGroup {
  kind: "sections" | "history" | "dictionary";
  title: string;
  items: SearchItem[];
}

// Module-level singletons: exactly one search box per settings window.
let index: { id: string; title: string }[] = [];
let input: HTMLInputElement;
let listbox: HTMLElement;
let open = false;
let activeIndex = -1;

let dictCache: DictEntry[] | null = null; // fetched once per open (Blaze)
let queryId = 0; // race guard: drop stale content resolves
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let boundStore: SettingsStore; // set in mountFederatedSearch for the content tier

function buildIndex(): void {
  index = SECTIONS.map((id) => ({ id, title: t(`nav.${id}`) }));
}

function matchSections(q: string): SearchItem[] {
  return index
    .filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        (KEYWORDS[s.id] ?? []).some((k) => k.includes(q)),
    )
    .map((s) => ({ section: s.id, icon: s.id, text: s.title }));
}

// Stable renderer: takes fully-assembled groups, handles the empty case, and
// assigns sequential option ids (search-opt-N) for aria-activedescendant (Task 2).
function renderDropdown(groups: SearchGroup[]): void {
  clearActive(); // a new result set invalidates the prior active option
  listbox.replaceChildren();
  const nonEmpty = groups.filter((g) => g.items.length > 0);
  if (nonEmpty.length === 0) {
    const empty = document.createElement("div");
    empty.className = "search-no-results";
    empty.textContent = t("search.no_results");
    listbox.append(empty);
    return;
  }
  let i = 0;
  for (const g of nonEmpty) {
    const grp = document.createElement("section");
    grp.className = "search-group";
    grp.dataset.kind = g.kind;
    const head = document.createElement("div");
    head.className = "search-group-title";
    head.textContent = g.title;
    grp.append(head);
    for (const it of g.items) {
      const row = document.createElement("div");
      row.className = "search-item";
      row.id = `search-opt-${i++}`;
      row.setAttribute("role", "option");
      row.dataset.section = it.section;
      if (it.icon) {
        const svg = ICONS[it.icon as keyof typeof ICONS];
        row.innerHTML = `${svg}<span class="search-item-text">${it.text}</span>`;
      } else {
        const span = document.createElement("span");
        span.className = "search-item-text";
        span.textContent = it.text;
        row.append(span);
      }
      if (it.sub) {
        const sub = document.createElement("span");
        sub.className = "search-item-sub";
        sub.textContent = it.sub;
        row.append(sub);
      }
      grp.append(row);
    }
    listbox.append(grp);
  }
}

function show(): void {
  if (open) return;
  open = true;
  listbox.hidden = false;
  input.setAttribute("aria-expanded", "true");
}

function close(): void {
  if (!open) return;
  clearActive(); // reset aria-activedescendant + active styling (carried-forward from Task 2 review)
  open = false;
  listbox.hidden = true;
  listbox.replaceChildren();
  input.setAttribute("aria-expanded", "false");
  dictCache = null; // Task 3: invalidate per-open dictionary cache
  if (debounceTimer) {
    clearTimeout(debounceTimer);
    debounceTimer = null;
  }
}

// All currently-rendered options in flat DOM order (across groups).
function flatOptions(): HTMLElement[] {
  return Array.from(listbox.querySelectorAll<HTMLElement>(".search-item"));
}

// APG combobox: clamp at the ends (no wrap). DOM focus stays on the input;
// AT focus follows via aria-activedescendant.
function setActive(next: number): void {
  const opts = flatOptions();
  if (opts.length === 0) {
    activeIndex = -1;
    input.removeAttribute("aria-activedescendant");
    return;
  }
  activeIndex = Math.max(0, Math.min(next, opts.length - 1));
  opts.forEach((el, idx) => {
    el.classList.toggle("active", idx === activeIndex);
    el.toggleAttribute("aria-selected", idx === activeIndex);
  });
  const active = opts[activeIndex];
  input.setAttribute("aria-activedescendant", active.id);
  active.scrollIntoView({ block: "nearest" });
}

function clearActive(): void {
  activeIndex = -1;
  flatOptions().forEach((el) => {
    el.classList.remove("active");
    el.removeAttribute("aria-selected");
  });
  input.removeAttribute("aria-activedescendant");
}

// Synchronous section tier: renders sections-only immediately on each keystroke.
// (Task 3 widens onInput to also schedule the debounced content fetch.)
function renderSectionsTier(q: string): void {
  renderDropdown([
    { kind: "sections", title: t("search.sections"), items: matchSections(q) },
  ]);
}

function matchDictionary(entries: DictEntry[], q: string): SearchItem[] {
  return entries
    .filter(
      (d) =>
        d.entry.toLowerCase().includes(q) ||
        d.replacement.toLowerCase().includes(q),
    )
    .slice(0, 5)
    .map((d) => ({ section: "dictionary", text: `${d.entry} → ${d.replacement}` }));
}

// Debounced content tier: history (server-filtered, gated on enabled) +
// dictionary (cached per-open, client-filtered). Drops stale resolves via queryId.
async function fetchContent(q: string): Promise<void> {
  const myQuery = ++queryId;
  const enabled = boundStore.get().settings?.history.enabled ?? false;

  const tasks: Promise<SearchGroup | null>[] = [
    enabled
      ? invoke<HistoryRow[]>("history_query", { search: q, limit: 5, offset: 0 })
          .then((rows) => ({
            kind: "history" as const,
            title: t("search.history"),
            items: rows.map((r) => ({
              section: "history",
              text: r.text.slice(0, 80),
              sub: new Date(r.created_at).toLocaleString(getCurrentLang()),
            })),
          }))
          .catch(() => null) // a failed source renders empty, never blanks the dropdown
      : Promise.resolve(null),
    (dictCache
      ? Promise.resolve(dictCache)
      : invoke<DictEntry[]>("dictionary_list").then((entries) => {
          dictCache = entries; // cache for the lifetime of this open
          return entries;
        })
    )
      .then((entries) => ({
        kind: "dictionary" as const,
        title: t("search.dictionary"),
        items: matchDictionary(entries, q),
      }))
      .catch(() => null),
  ];

  const [historyGroup, dictGroup] = await Promise.all(tasks);
  if (myQuery !== queryId) return; // a newer keystroke superseded this one
  const groups: SearchGroup[] = [
    { kind: "sections", title: t("search.sections"), items: matchSections(q) },
  ];
  if (historyGroup && historyGroup.items.length) groups.push(historyGroup);
  if (dictGroup && dictGroup.items.length) groups.push(dictGroup);
  renderDropdown(groups);
}

// Shared by both `input` and `focus` listeners (re-focus on a non-empty box must
// re-render — close() empties the listbox, so a bare show() would display nothing).
function onInput(): void {
  const q = input.value.trim().toLowerCase();
  if (!q) {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
    close();
    return;
  }
  renderSectionsTier(q); // instant tier (synchronous)
  show();
  // content tier (debounced 150ms)
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => void fetchContent(q), 150);
}

export function mountFederatedSearch(
  store: SettingsStore,
  navigate: (id: string) => void,
): HTMLElement {
  boundStore = store; // content tier reads settings.history.enabled
  buildIndex();

  const form = document.createElement("form");
  form.className = "search-box";
  form.setAttribute("role", "search");
  form.addEventListener("submit", (e) => e.preventDefault()); // Enter = navigate, not submit

  input = document.createElement("input");
  input.type = "text";
  input.className = "search-input";
  input.setAttribute("role", "combobox");
  input.setAttribute("aria-haspopup", "listbox");
  input.setAttribute("aria-expanded", "false");
  input.setAttribute("aria-controls", "search-listbox");
  input.setAttribute("aria-autocomplete", "list");
  input.setAttribute("aria-label", t("search.placeholder"));
  input.placeholder = t("search.placeholder");

  listbox = document.createElement("div");
  listbox.className = "search-dropdown";
  listbox.id = "search-listbox";
  listbox.setAttribute("role", "listbox");
  listbox.hidden = true;

  form.append(input, listbox);

  input.addEventListener("input", onInput);
  input.addEventListener("focus", onInput);

  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      if (!open && input.value.trim()) {
        renderSectionsTier(input.value.trim().toLowerCase());
        show();
      }
      if (open) {
        e.preventDefault();
        setActive(activeIndex + 1);
      }
    } else if (e.key === "ArrowUp") {
      if (open) {
        e.preventDefault();
        setActive(activeIndex - 1);
      }
    } else if (e.key === "Enter") {
      if (open && activeIndex >= 0) {
        e.preventDefault();
        const opts = flatOptions();
        const sec = opts[activeIndex]?.dataset.section;
        if (sec) {
          navigate(sec);
          input.value = "";
          close();
        }
      }
    } else if (e.key === "Escape") {
      if (open) {
        e.preventDefault();
        close();
      }
    }
  });

  // Delay close so a click on an option registers before the blur fires.
  input.addEventListener("blur", () => {
    setTimeout(() => {
      if (open) close();
    }, 150);
  });

  // Keep focus on the input while clicking an option (APG: DOM focus stays on combobox).
  listbox.addEventListener("mousedown", (e) => e.preventDefault());

  listbox.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const item = target.closest<HTMLElement>(".search-item");
    if (!item) return;
    const sec = item.dataset.section;
    if (sec) {
      navigate(sec);
      input.value = "";
      close();
    }
  });

  listbox.addEventListener("mouseover", (e) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>(".search-item");
    if (!item) return;
    const opts = flatOptions();
    const idx = opts.indexOf(item);
    if (idx >= 0 && idx !== activeIndex) setActive(idx);
  });

  return form;
}

// Rebuild the localized section-title index on UI-lang change; re-render if open.
export function refreshSearchLang(): void {
  buildIndex();
  input?.setAttribute("aria-label", t("search.placeholder"));
  input.placeholder = t("search.placeholder");
  if (open) {
    const q = input.value.trim().toLowerCase();
    if (q) renderSectionsTier(q);
  }
}
