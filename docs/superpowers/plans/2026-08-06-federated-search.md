# Federated Settings Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A sidebar-top autocomplete search box that surfaces matching settings sections (instant, in-memory) plus inline history and dictionary matches (debounced IPC), navigable by mouse and keyboard — making molvi's 9-section settings panel navigable in one place.

**Architecture:** One new frontend-only module (`src/settings/federated-search.ts`) mounted at the top of `#sidebar`. Two-tier responsiveness: section matches filter synchronously on every keystroke (in-memory index, zero IPC); history + dictionary matches follow a 150 ms debounce + existing IPC. WAI-ARIA combobox + listbox pattern (DOM focus on the input, AT focus via `aria-activedescendant`). Strictly additive — no Rust, no new dependencies, no changes to existing section files.

**Tech Stack:** TypeScript 7 (native) + Vite 8, vanilla TS (no framework), Tauri 2 IPC (`invoke`), WAI-ARIA APG combobox pattern. No JS test runner (gate = `tsc` + `vite build` + human GUI smoke).

**Spec:** [`docs/superpowers/specs/2026-08-06-federated-search-design.md`](../specs/2026-08-06-federated-search-design.md)

---

## Global Constraints

Copied verbatim from the spec — every task inherits these:

- **App identity:** molvi, Windows 11 push-to-talk dictation. Identifier `com.molvi.app`.
- **Privacy (§10.1):** NEVER log transcript text / dictionary entries / history rows — any level. The search dropdown *displays* the user's own data in their own UI (display ≠ logging); the only `console.*` calls log the error object on IPC failure (metadata-only). No new `log_privacy.rs` substrate (frontend-only, no Rust logging).
- **Blaze (one-way ratchet):** no regression to the default RU/PTT/Smart dictation path. This feature is off the inference hot path; it mounts once at settings-window init and does nothing unless focused. The search itself must feel instant (in-memory section tier + bounded IPC content tier).
- **No new dependencies:** `Cargo.toml`, `Cargo.lock`, `package.json` are **untouched**. Frontend-only; no Rust.
- **No backward compatibility needed:** clean breaks OK; `#[serde(default)]` regenerates.
- **Ponytail FULL:** smallest diff, stdlib/native first, `// ponytail:` for deliberate shortcuts, comments explain WHY never WHAT.
- **i18n:** `en` canonical; all 36 locales get the same key set before merge (set-equality invariant). New `search.*` cluster.
- **WAI-ARIA conformance:** the input is `role="combobox"` + `aria-haspopup="listbox"` + `aria-autocomplete="list"` + `aria-expanded` + `aria-controls` + `aria-activedescendant`; the popup is `role="listbox"`, items `role="option"`; DOM focus stays on the input, AT focus moves via `aria-activedescendant`; arrow keys **clamp at the ends (no wrap)**. Verified 2026-08-06 against `w3.org/WAI/ARIA/apg/patterns/combobox`.
- **Tauri IPC:** args are camelCase by default (verified in the Session-8 handoff against the Tauri 2 doc). Here all args are single words (`search`, `limit`, `offset`) → identical in camelCase.
- **Gates (per task):** `npx tsc --noEmit` exit 0 + `npm run build` exit 0. No `cargo` (Rust untouched). Human GUI smoke is the behavioral gate (no JS test runner). `molvi.exe` may be running (binary lock) — irrelevant here since no Rust is compiled.

### Verified source facts (from the current codebase — do NOT re-derive)

- **Existing IPC:** `invoke<HistoryRow[]>("history_query", { search, limit, offset })` (`src-tauri/src/ipc.rs:221`); returns `[]` when history disabled (R6 — no error). `invoke<DictEntry[]>("dictionary_list")` (`ipc.rs:147`) returns all entries.
- **`selectSection(id: string)`** is exported from `src/settings/main.ts:50`.
- **`Store<State>` / `SettingsStore`** (`src/settings/types.ts:137`); `store.get().settings!.history.enabled` gates history.
- **`HistoryRow`** = `{ id, created_at (unix ms), text, lang, engine, post_mode }` (`types.ts:118`). **`DictEntry`** = `{ entry, replacement, created_at }` (`types.ts:111`).
- **`ICONS`** = 9 section SVGs keyed by id (`src/settings/icons.ts`); no search glyph (plain input).
- **CSS tokens** (`src/settings.css:3-26`): `--bg #FFFFFF`, `--canvas`, `--border #E5E7EB`, `--text`, `--muted #4B5563`, `--accent #0E7C86`, `--accent-hover #0a6670`, `--on-accent #FFFFFF`, `--radius-control 6px`, `--radius-card 8px`, `--ease`. There is **no** `--surface` (use `--bg`) and **no** `--accent-bright` in `settings.css`. `input[type="text"]` inherits a global baseline + `:focus-visible { outline: 2px solid var(--accent) }`.
- **`#sidebar`** = flex column, `width: 200px` (`settings.css:59`). The dropdown matches this width (content truncates with ellipsis).
- **`rerender()`** in `main.ts:119` runs on UI-lang change (lang `<select>` + `ui-lang-changed` event) — the hook for index rebuild.
- **i18n:** `t(key)` 3-level fallback (current→en→raw key); `getCurrentLang()`; `setCurrentLang()`; `LANGUAGES`. Locale files at `src/i18n/locales/<lang>.ts`, registry `src/i18n/locales.ts`. `en` canonical.

---

## File Structure

| File | Responsibility | Touched by |
|---|---|---|
| `src/settings/federated-search.ts` | **NEW.** The whole feature: section index, two-tier query, WAI-ARIA combobox widget, content federation, lang refresh. | Tasks 1, 2, 3 |
| `src/settings/main.ts` | Mount the search at top of `#sidebar`; call `refreshSearchLang()` in `rerender()`. | Task 1 |
| `src/settings.css` | `.search-*` rules (logical props, AA accent, RTL-safe, reduced-motion). | Task 1 |
| `src/i18n/locales/en.ts` | `search.*` canonical EN keys (5). | Task 1 |
| `src/i18n/locales/<other 35>.ts` | `search.*` propagated + translated (×35). | Task 4 |

`federated-search.ts` exports two symbols (final shape):
- `mountFederatedSearch(store: SettingsStore, navigate: (id: string) => void): HTMLElement`
- `refreshSearchLang(): void`

`navigate` = `selectSection` passed by `main.ts` (avoids a circular `main ↔ federated-search` import).

---

## Task 1: Core autocomplete widget — sections tier (instant) + click navigation + open/close

**Role:** The self-contained MVP. A search box at the top of the sidebar that, as you type, instantly shows matching sections (in-memory index, zero IPC); click navigates and closes. Static WAI-ARIA combobox markup is in place; `aria-expanded` toggles. (Keyboard arrow navigation + `aria-activedescendant` are Task 2; history/dictionary federation is Task 3.)

**Files:**
- Create: `src/settings/federated-search.ts`
- Modify: `src/settings/main.ts` (mount + `rerender()` hook)
- Modify: `src/settings.css` (append `.search-*` rules)
- Modify: `src/i18n/locales/en.ts` (add `search.*` canonical EN keys)

**Interfaces:**
- Consumes: `t`, `ICONS`, `SettingsStore` type, `selectSection` (passed as `navigate` by `main.ts`).
- Produces: `mountFederatedSearch(store, navigate): HTMLElement`, `refreshSearchLang(): void`. Later tasks widen the internal input handler (Task 3 adds the content fetch) and add keyboard handling (Task 2) — both modify this file.

- [ ] **Step 1: Add canonical EN i18n keys**

Append to the `search.*` cluster in `src/i18n/locales/en.ts` (create the cluster after the `recognition.*` cluster, before `command.*`; keys alphabetical within it). If a `search` object does not yet exist, create it:

```ts
  search: {
    placeholder: "Search settings…",
    sections: "Sections",
    history: "History",
    dictionary: "Dictionary",
    no_results: "No results",
  },
```

(`search.history` / `search.dictionary` are used by Task 3; defining them now avoids a second `en.ts` edit. Sibling locales are Task 4.)

- [ ] **Step 2: Create `src/settings/federated-search.ts` — section index + stable renderer + widget shell**

```ts
// Federated settings search (Task 11). Autocomplete dropdown at the top of the
// sidebar: instant in-memory section matches (synchronous tier) + debounced
// history/dictionary matches (content tier, Task 3). WAI-ARIA combobox pattern
// (role=combobox + listbox + aria-activedescendant; DOM focus stays on input).

import { ICONS } from "./icons";
import type { SettingsStore } from "./types";
import { t } from "../i18n";

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
  open = false;
  listbox.hidden = true;
  listbox.replaceChildren();
  input.setAttribute("aria-expanded", "false");
}

// Synchronous section tier: renders sections-only immediately on each keystroke.
// (Task 3 widens onInput to also schedule the debounced content fetch.)
function renderSectionsTier(q: string): void {
  renderDropdown([
    { kind: "sections", title: t("search.sections"), items: matchSections(q) },
  ]);
}

// Shared by both `input` and `focus` listeners (re-focus on a non-empty box must
// re-render — close() empties the listbox, so a bare show() would display nothing).
function onInput(): void {
  const q = input.value.trim().toLowerCase();
  if (!q) {
    close();
    return;
  }
  renderSectionsTier(q); // instant tier (Task 3 appends the debounced content tier here)
  show();
}

export function mountFederatedSearch(
  store: SettingsStore,
  navigate: (id: string) => void,
): HTMLElement {
  void store; // read in Task 3 (history.enabled gate)
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
```

- [ ] **Step 3: Mount the search at the top of `#sidebar` + wire `refreshSearchLang`**

In `src/settings/main.ts`:

Add the import (next to the other `./` imports, e.g. after `./store`):
```ts
import { mountFederatedSearch, refreshSearchLang } from "./federated-search";
```

In `buildSidebar()` (around line 85), prepend the search as the first child of `#sidebar` (before the section-button loop, so it lands at the top):
```ts
function buildSidebar(): void {
  const nav = document.getElementById("sidebar")!;
  nav.prepend(mountFederatedSearch(store, selectSection)); // Task 11: federated search at the top
  for (const s of SECTIONS) {
    // ...existing loop unchanged...
  }
  // ...existing lang-row code unchanged...
}
```

In `rerender()` (around line 119), call `refreshSearchLang()` so the index + placeholder re-localize on language switch:
```ts
function rerender(): void {
  refreshSearchLang(); // Task 11: rebuild section index + re-translate placeholder
  document.querySelectorAll<HTMLElement>("#sidebar .item").forEach((el) => {
    // ...existing unchanged...
  });
  // ...rest unchanged...
}
```

- [ ] **Step 4: Append the `.search-*` CSS rules**

Append to `src/settings.css` (uses only existing tokens — `--bg`, `--border`, `--muted`, `--accent`, `--on-accent`, `--radius-control`; logical properties for RTL):

```css
/* ── Federated settings search (Task 11) ──────────────────────────────── */

.search-box {
  position: relative;          /* anchor for the absolute dropdown */
  margin-block-end: 8px;
}

/* input[type="text"] inherits the global input baseline (border/padding/radius)
   + :focus-visible outline via --accent. Only width needed here. */
.search-input {
  inline-size: 100%;
}

.search-dropdown {
  position: absolute;
  inset-inline-start: 0;
  inset-block-start: 100%;     /* directly below the input */
  inline-size: 100%;           /* match the 200px sidebar */
  max-block-size: 60vh;
  overflow-y: auto;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius-control);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.12);
  z-index: 10;                 /* above sidebar items */
  padding-block: 4px;
}

.search-group {
  padding-block: 2px;
}

.search-group-title {
  padding: 4px 10px;
  font-size: 12px;
  color: var(--muted);
}

.search-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  margin-inline: 4px;
  border-radius: var(--radius-control);
  cursor: pointer;
}

.search-item svg {
  flex: 0 0 auto;
  color: var(--muted);
}

.search-item-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.search-item-sub {
  margin-inline-start: auto;
  font-size: 12px;
  color: var(--muted);
  flex: 0 0 auto;
}

/* Mouse hover AND keyboard .active share the same accent treatment. */
.search-item:hover,
.search-item.active {
  background: var(--accent);
  color: var(--on-accent);
}

.search-item:hover svg,
.search-item.active svg,
.search-item:hover .search-item-sub,
.search-item.active .search-item-sub {
  color: var(--on-accent);
}

.search-no-results {
  padding: 10px;
  color: var(--muted);
  font-size: 13px;
}

@media (prefers-reduced-motion: reduce) {
  .search-dropdown,
  .search-item {
    transition: none;
  }
}
```

- [ ] **Step 5: Run the gates**

Run: `npx tsc --noEmit`
Expected: exit 0.

Run: `npm run build`
Expected: exit 0 (69 modules → still 69; the new module is imported by `main.ts`).

- [ ] **Step 6: Commit**

```bash
git add src/settings/federated-search.ts src/settings/main.ts src/settings.css src/i18n/locales/en.ts
git commit -m "feat(settings): federated search — sections autocomplete tier + open/close"
```

**Smoke (behavioral gate — human, after the task lands):** open Settings; the search box sits at the top of the sidebar. Type "vad" → Recognition appears; "paste" → Text; "mic" → Microphone; "update" → Updates. Click a match → that section opens, the box clears. Type gibberish → "No results" (EN only until Task 4). Esc is not wired yet (Task 2). Click outside → dropdown closes.

---

## Task 2: Keyboard navigation + dynamic ARIA (`aria-activedescendant`)

**Role:** Make the dropdown fully keyboard-usable per the WAI-ARIA combobox APG: ↓/↑ move the active option (clamped, no wrap), Enter activates it, Esc closes. `aria-activedescendant` tracks the active option (DOM focus stays on the input). Mouse hover also sets active so click + keyboard share one state.

**Files:**
- Modify: `src/settings/federated-search.ts` (add active state + keydown handler + reset on re-render)

**Interfaces:**
- Consumes: Task 1's `renderDropdown`, `listbox`, `input`, `close`.
- Produces: no new exports.

- [ ] **Step 1: Add active-index state + helpers**

In `src/settings/federated-search.ts`, add a module-level `activeIndex` near the other singletons (after `let open = false;`):

```ts
let activeIndex = -1;
```

Add helpers after `close()`:

```ts
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
  opts.forEach((el, idx) => el.classList.toggle("active", idx === activeIndex));
  const active = opts[activeIndex];
  input.setAttribute("aria-activedescendant", active.id);
  active.scrollIntoView({ block: "nearest" });
}

function clearActive(): void {
  activeIndex = -1;
  flatOptions().forEach((el) => el.classList.remove("active"));
  input.removeAttribute("aria-activedescendant");
}
```

- [ ] **Step 2: Reset active state on every re-render**

In `renderDropdown()`, reset active at the very top (first statement inside the function, before `listbox.replaceChildren()`):

```ts
function renderDropdown(groups: SearchGroup[]): void {
  clearActive(); // a new result set invalidates the prior active option
  listbox.replaceChildren();
  // ...rest unchanged...
}
```

- [ ] **Step 3: Wire the keydown handler on the input**

In `mountFederatedSearch()`, register the keydown listener alongside the other `input.addEventListener` calls:

```ts
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
```

- [ ] **Step 4: Mouse hover sets active too (shared state)**

In `mountFederatedSearch()`, add a `mousemove`/`mouseenter` delegation on the listbox so hovering an option syncs `activeIndex` (the click handler from Task 1 already navigates; this makes hover visually match keyboard). Add inside the existing `listbox.addEventListener("click", …)` block area:

```ts
  listbox.addEventListener("mouseover", (e) => {
    const item = (e.target as HTMLElement).closest<HTMLElement>(".search-item");
    if (!item) return;
    const opts = flatOptions();
    const idx = opts.indexOf(item);
    if (idx >= 0 && idx !== activeIndex) setActive(idx);
  });
```

- [ ] **Step 5: Run the gates**

Run: `npx tsc --noEmit` → exit 0.
Run: `npm run build` → exit 0.

- [ ] **Step 6: Commit**

```bash
git add src/settings/federated-search.ts
git commit -m "feat(settings): federated search keyboard nav + aria-activedescendant"
```

**Smoke:** focus the search box, type "se"; press ↓ → first option highlights (accent); ↓/↑ move it; Enter → navigates; Esc → closes. Hover an option → it highlights the same way. After results change (type more), the highlight resets (no stale option).

---

## Task 3: Federated content tier — history + dictionary matches

**Role:** Beyond sections, surface inline matches from the user's history and dictionary. History is server-side SQL-filtered (`history_query`, ≤5 rows, gated on `settings.history.enabled`); dictionary is fetched once per open and client-filtered. A `queryId` race guard drops stale results; the content tier runs on a 150 ms debounce after the instant section tier.

**Files:**
- Modify: `src/settings/federated-search.ts` (add the content fetch + widen the input handler + render history/dictionary groups)

**Interfaces:**
- Consumes: `invoke` (`@tauri-apps/api/core`), `getCurrentLang` (`../i18n`), `HistoryRow`/`DictEntry` types, `store` (for `history.enabled`).
- Produces: no new exports.

- [ ] **Step 1: Add imports + content-tier state**

At the top of `src/settings/federated-search.ts`, widen the imports:

```ts
import { invoke } from "@tauri-apps/api/core";
import { ICONS } from "./icons";
import type { DictEntry, HistoryRow, SettingsStore } from "./types";
import { getCurrentLang, t } from "../i18n";
```

Add module-level state near the other singletons (after `let activeIndex = -1;`):

```ts
let dictCache: DictEntry[] | null = null; // fetched once per open (Blaze)
let queryId = 0;                           // race guard: drop stale content resolves
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
let boundStore: SettingsStore;              // set in mountFederatedSearch for the content tier
```

- [ ] **Step 2: Capture the store in `mountFederatedSearch`**

In `mountFederatedSearch`, replace the `void store;` line with:

```ts
  boundStore = store; // content tier reads settings.history.enabled
```

- [ ] **Step 3: Add the content-fetch + group builders**

Add after `renderSectionsTier`:

```ts
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
```

- [ ] **Step 4: Widen `onInput()` with the debounced content tier + invalidate cache on close**

Replace the Task 1 module-level `onInput()` function with this widened version (Task 1's `input` + `focus` listeners already call `onInput`, so this covers re-focus too):

```ts
function onInput(): void {
  const q = input.value.trim().toLowerCase();
  if (!q) {
    if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
    close();
    return;
  }
  renderSectionsTier(q); // instant tier (synchronous)
  show();
  // content tier (debounced 150ms)
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => void fetchContent(q), 150);
}
```

In `close()`, invalidate the dictionary cache so the next open re-fetches (picks up edits made in the Dictionary section meanwhile). Add at the end of `close()`:

```ts
function close(): void {
  if (!open) return;
  open = false;
  listbox.hidden = true;
  listbox.replaceChildren();
  input.setAttribute("aria-expanded", "false");
  dictCache = null; // Task 3: invalidate per-open dictionary cache
  if (debounceTimer) { clearTimeout(debounceTimer); debounceTimer = null; }
}
```

- [ ] **Step 5: Run the gates**

Run: `npx tsc --noEmit` → exit 0.
Run: `npm run build` → exit 0.

- [ ] **Step 6: Commit**

```bash
git add src/settings/federated-search.ts
git commit -m "feat(settings): federated search — history + dictionary content tier"
```

**Smoke (history must be enabled for the history group; have a few dict entries + a few dictated phrases):** type a known word — sections appear instantly, then after ~150 ms a "History" group (rows: truncated text · timestamp) and/or a "Dictionary" group ("entry → replacement") appear below. Click a history row → History section opens. Click a dict row → Dictionary section opens. Type fast — no flicker from stale results (race guard). Disable history (toggle off) → no history group renders. The dictionary is fetched once per open (watch the network/IPC — repeated keystrokes in one open do not re-fetch).

---

## Task 4: i18n propagation (×35 non-EN locales) + RTL/reduced-motion verification

**Role:** The 5 `search.*` keys exist only in `en` after Task 1. Propagate + translate them to the other 35 locales so the set-equality invariant holds (en canonical; every locale's key set === en's). Then verify RTL (ar/he) renders correctly and reduced-motion disables transitions (already in Task 1's CSS — visual confirm only).

**Files:**
- Modify: `src/i18n/locales/<ar,bg,cs,da,de,el,es,et,fi,fr,he,hi,hr,hu,it,ja,ko,lt,lv,mt,nb,nl,pl,pt,ro,ru,sk,sl,sr,sv,th,tr,uk,vi,zh,zh-TW>.ts` (35 files — all except `en.ts`)

**Interfaces:**
- Consumes: the 5 EN keys from Task 1 (`search.placeholder`, `search.sections`, `search.history`, `search.dictionary`, `search.no_results`).
- Produces: set-equality across all 36 locales.

- [ ] **Step 1: Add the `search` cluster to all 35 non-EN locale files**

In each of the 35 locale files, add the `search` object in the SAME position as in `en.ts` (after the `recognition.*` cluster, before `command.*`; keys alphabetical). Translate the values per-locale; keep `search.sections`/`search.history`/`search.dictionary` as the locale's words for Sections/History/Dictionary (reuse the existing `nav.history` / `nav.dictionary` translations for consistency where they exist). Example (RU):

```ts
  search: {
    placeholder: "Поиск настроек…",
    sections: "Разделы",
    history: "История",
    dictionary: "Словарь",
    no_results: "Нет результатов",
  },
```

Conventions (from the model-picker Task 14.5, carried forward):
- The `…` ellipsis in `placeholder` is preserved in all locales (or the locale's convention — RU/DE/FR/etc. use `…`).
- RTL (ar, he): the cluster is a plain object; values translate normally; the `…` and ASCII placeholders stay ASCII (browser bidi handles them).
- CJK (ja, zh, zh-TW): terminal punctuation `。` where the EN ends with `.`/`…` — actually `placeholder`/`no_results` have no terminal period; keep them clean. ja/zh use native punctuation only where EN has a sentence (none here).
- th: no terminal punctuation (Thai convention).

- [ ] **Step 2: Verify set-equality (36 locales, en canonical)**

`search:` is a new cluster and `no_results:` is a search-unique key. Both must be present in all 36 locale files:

```powershell
# expect 36 for each (one match per file via -List)
(Select-String -Path src/i18n/locales/*.ts -Pattern "search: \{" -List).Count
(Select-String -Path src/i18n/locales/*.ts -Pattern "no_results:" -List).Count
```

Expected: both print `36`. (Counting `history:`/`dictionary:` directly would over-match the existing top-level clusters of those names — `no_results` is unique to `search`, so it's the reliable probe.)

- [ ] **Step 3: Run the gates**

Run: `npx tsc --noEmit` → exit 0.
Run: `npm run build` → exit 0.

- [ ] **Step 4: Commit**

```bash
git add src/i18n/locales/*.ts
git commit -m "i18n: add search.* keys (x36) for federated settings search"
```

**Smoke (the full-feature gate):**
1. EN: search "vad" → Recognition; "paste" → Text + (if dict has a "paste"-related entry) a Dictionary row; a known history phrase → a History row. Keyboard: ↓/↑/Enter/Esc. No-results for gibberish. Click outside closes.
2. Switch UI to RU → placeholder becomes "Поиск настроек…", group titles localize ("Разделы"/"История"/"Словарь"), "Нет результатов" for gibberish. Type "микрофон" → Microphone matches (localized title).
3. RTL: switch to `ar` and `he` → the dropdown anchors to the inline-start (right edge), items read RTL, no overflow, group titles + option text render RTL correctly. Keyboard arrows still work.
4. Reduced-motion: enable OS reduced-motion → the dropdown appears instantly (no transition).
5. Default dictation path intact: the hotkey still pastes (the search feature is inert unless the settings window is focused).

---

## Self-Review (controller, after writing — recorded for the implementer)

**Spec coverage:**
- §1 Overview / §3 Scope (sections+history+dict) → Tasks 1+3. ✓
- §4 Architecture (additive, no Rust) → File Structure + every task's gates (tsc/build only). ✓
- §5 Component (DOM shape, item rendering, positioning) → Task 1 renderDropdown + CSS. ✓
- §6.1 Two-tier (instant sections + debounced content) → Task 1 renderSectionsTier + Task 3 fetchContent. ✓
- §6.2 Content fetch (history gated, dictionary client-filter) → Task 3. ✓
- §6.3 Dictionary cache per-open → Task 3 `dictCache` + invalidate in `close()`. ✓
- §6.4 Open/close lifecycle (focus/blur-delay/Esc/select/empty) → Task 1 + Task 2 Esc + Task 3 empty cancels debounce. ✓
- §6.5 Section index + lang refresh → Task 1 `buildIndex`/`KEYWORDS` + `refreshSearchLang`. ✓
- §7 Matching semantics (substring, no scoring, top-N) → Task 1 `matchSections` + Task 3 `matchDictionary`. ✓
- §8 Keyboard + ARIA (combobox/listbox/option/activedescendant, clamp-no-wrap, activeIndex reset) → Task 2. ✓
- §9 i18n (5 keys × 36) → Task 1 (en) + Task 4 (×35). ✓
- §10 Privacy (no new logging, console.error metadata-only) → Task 3 `.catch(() => null)`; no `console.*` of content. ✓
- §11 Blaze (default path untouched, instant tier, bounded DOM, race-safe, no polling) → Task 1 synchronous tier + Task 3 race guard + absolute dropdown. ✓
- §12 CSS (logical props, --bg not --surface, --accent, reduced-motion) → Task 1 CSS. ✓

**Placeholder scan:** none — every step has exact code.

**Type consistency:** `SearchItem`/`SearchGroup` defined in Task 1, used unchanged in Task 3. `mountFederatedSearch(store, navigate)` signature consistent across Task 1 (defined) + Task 3 (reads `boundStore`). `renderDropdown(groups)` stable from Task 1; Task 2 adds `clearActive()` at its top; Task 3 calls it with assembled groups. `flatOptions()`/`setActive()`/`clearActive()` defined Task 2, used Task 2 + (implicitly) via renderDropdown's `clearActive`.

**One known cross-task edit:** Task 2 modifies `renderDropdown` (adds `clearActive()` as its first line) and Task 3 modifies `close()` (adds cache invalidation) and the input handler. Each task's step shows the exact change with context. The implementer sees only their task brief.
