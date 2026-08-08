# Federated Settings Search — Design Spec

**Date:** 2026-08-06
**Branch:** `phase3` (Task 11 in the Phase-3 plan)
**Status:** Design — pending implementation plan
**Author:** controller + human (brainstormed)

## 1. Overview

molvi's settings panel has grown to 9 sidebar sections (recognition,
microphone, text, dictionary, history, hotkey, overlay, updates, about), each
holding dense, sometimes technical controls (VAD sliders, paste modes, model
picker, retention rules). There is no global search — a user who wants "where
do I change the noise threshold" or "what did I dictate about invoices
yesterday" must know which section holds it, then use that section's own
in-section search (only history has one; dictionary has none).

### Problem

Settings discoverability degrades as the panel grows. Competitor dictation
apps (Superwhisper, Dragon) either stay shallow or add a command-palette /
search surface. molvi has neither. The in-section search boxes are
siloed: history's search does not reach dictionary, and nothing reaches
across sections.

### Goal

A single search box at the **top of the sidebar** that, as the user types,
surfaces a **federated autocomplete dropdown**: matching **sections** (jump
to), plus inline **history** and **dictionary** matches — one surface to find
anything in settings. Instant for sections, bounded-IPC for content.

### Success criteria

1. A search box is always visible at the top of the sidebar, above the
   section buttons.
2. Typing shows, in one dropdown below the box: matching sections (synchronous,
   instant), then up to 5 history matches, then up to 5 dictionary matches.
3. Clicking (or Enter on) any result navigates to its section and closes the
   dropdown.
4. Section matches appear **synchronously on every keystroke** (in-memory
   index — zero IPC, zero perceived lag). History/dictionary matches follow
   the 150 ms debounce + IPC.
5. Full keyboard navigation (↑/↓/Enter/Esc) + ARIA roles — usable without a
   mouse.
6. No new Rust, no new dependencies, no changes to existing section files
   (strictly additive). No new privacy logging surface.
7. Blaze: no regression to the default RU/PTT/Smart dictation path (this is a
   settings-UI feature, entirely off the inference hot path) and the search
   itself is snappy enough to feel instant vs competitor settings search.

## 2. Non-goals (YAGNI — explicitly out of scope)

- **Snippets / profiles federated results.** Neither has an IPC `*_list`
  command (snippets has a backend store wired into the Smart step but **no
  IPC and no UI**; profiles live in `settings.json`, resolved server-side, no
  IPC). Adding them = building snippet/profile IPC + UI = separate work
  (closer to Task 12 / a dedicated task). Federated search covers sections +
  history + dictionary only.
- **Inline `?` help per `SettingsGroup`.** The Phase-3 plan's Task 11 Step 3
  mandated a `?` button + `Alert` hint on every group. This is **redundant**:
  Session-6 added `InfoTip` (ⓘ) with a hover/focus bubble to every group title
  and field label already (`SettingsGroup(title, children, tip?)` 3rd arg +
  `Toggle`/`Slider` `tip` arg). Dropped.
- **Seeding the destination section's search input** (click a history result →
  land in History with the query pre-typed). History's `searchInput` is a
  closure local (not exported); dictionary has no filter input at all (that's
  Task 12). Seeding would couple this feature into section internals + add a
  dictionary filter — scope creep. The dropdown already previews each match;
  landing on the section to act is acceptable for v1. See §10 deferred.
- **Fuzzy matching / scoring / ranking.** Plain case-insensitive substring
  match. No fuse.js, no Levenshtein. (ponytail — substring is correct on edge
  cases for the scale here: ≤9 sections + bounded content rows.)
- **Search across onboarding / overlay windows.** This is a settings-window
  feature only.
- **Persistence of recent searches.** State-free; the box clears on close.

## 3. Scope (locked, from brainstorm)

| Source | In scope | Mechanism |
|---|---|---|
| Sections (9) | ✅ | In-memory `{id, title, keywords}` index, synchronous substring filter |
| History | ✅ | `history_query({search, limit:5, offset:0})` (existing IPC; gated on `settings.history.enabled`) |
| Dictionary | ✅ | `dictionary_list()` → client-side filter (cached per open) |
| Snippets | ❌ | No IPC/UI (deferred) |
| Profiles | ❌ | No IPC; lives in settings.json (deferred) |
| `?`-help | ❌ | InfoTip already everywhere (redundant) |

## 4. Architecture (strictly additive)

```
src/settings/
  federated-search.ts    — NEW: mountFederatedSearch(store): { el, cleanup }
  main.ts                — MODIFY: mount search at top of #sidebar; re-translate in rerender()
src/settings.css         — MODIFY: .search-box / .search-dropdown / groups / items / keyboard-active
src/i18n/locales/*.ts    — MODIFY: +search.* keys × 36
```

**No Rust changes.** No new `#[tauri::command]`, no `Cargo.toml`/`Cargo.lock`
touch, no `log_privacy.rs` change. The feature consumes two existing IPC
commands (`history_query`, `dictionary_list`) and one existing export
(`selectSection` from `main.ts`).

### Module contract

`federated-search.ts` exports two symbols:

- `mountFederatedSearch(store: SettingsStore, navigate: (id: string) => void): HTMLElement`
  — builds the `<form class="search-box">` (input + dropdown), wires all
  listeners on its own elements, and returns the element. `main.ts` appends it
  to the top of `#sidebar` once at init, passing `selectSection` as `navigate`
  (a callback avoids a circular `main ↔ federated-search` module import and is
  cleaner than an event bus). There is **no `cleanup` return**: the search
  lives for the whole settings-window lifetime, all its listeners are attached
  to its own DOM (GC'd with the window), and it is never unmounted — so there
  is nothing to tear down (ponytail: no parity stub for a lifecycle phase that
  doesn't exist).
- `refreshSearchLang(): void` — rebuilds the in-memory section index (titles
  are localized) and re-translates the placeholder; if the dropdown is open,
  re-runs the filter. `main.ts`'s `rerender()` calls this alongside its
  sidebar-label re-translation on `ui_lang` change.

The search reads the current lang via `getCurrentLang()` at query time
(timestamps, matching) — no stale-lang risk.

## 5. Component: autocomplete dropdown

### 5.1 DOM shape

```
<form class="search-box" role="search">
  <input type="text" role="combobox" aria-haspopup="listbox"
         aria-expanded="false" aria-controls="search-listbox"
         aria-autocomplete="list"
         aria-label="<t('search.placeholder')>" placeholder="…" />
  <div class="search-dropdown" id="search-listbox" role="listbox" hidden>
    <section class="search-group" data-kind="sections">…</section>
    <section class="search-group" data-kind="history">…</section>
    <section class="search-group" data-kind="dictionary">…</section>
    <!-- OR a single .search-no-results row when all empty -->
  </div>
</form>
```

A group whose result set is empty renders nothing (collapsed); only non-empty
groups appear. When **all** groups are empty for a non-empty query, the
dropdown shows one `<div class="search-no-results">t("search.no_results")</div>`.

### 5.2 Item rendering

- **Section item**: icon (`ICONS[id]`) + `t("nav.id")` title. `role="option"`.
- **History item**: `r.text.slice(0, 80)` + ` · ` +
  `new Date(r.created_at).toLocaleString(getCurrentLang())`. `role="option"`.
  (Truncation + CSS ellipsis; matches the existing history-row pattern at
  `history.ts:200-201`.)
- **Dictionary item**: `${entry} → ${replacement}`. `role="option"`.

Every item carries `data-section="<id>"` (the destination section) so the
activate handler is uniform: `selectSection(item.dataset.section)` + close.

### 5.3 Dropdown positioning

`position: absolute` anchored under the input (`inset-inline-start: 0`,
`inset-block-start: 100%`). `z-index` above sidebar content. `max-height`
+ `overflow-y: auto` for the rare long list. Logical properties throughout
(RTL-safe; the dropdown mirrors under `dir="rtl"` automatically).

## 6. Data flow + query lifecycle

### 6.1 Input handling (two-tier)

On every `input` event:

1. **Immediate (synchronous tier)** — filter the in-memory section index by
   the trimmed, lowercased query; re-render the `sections` group at once.
   This is the "feels instant" surface — zero IPC, sub-millisecond.
2. **Debounced content tier** — clear any pending timer; set a 150 ms timer.
   When it fires:
   - Assign a new `queryId` (incrementing counter).
   - Fire the content fetches (§6.2).
   - On resolve, ignore if `queryId` is stale (a newer keystroke superseded
     it).

Empty query → hide dropdown, cancel pending timer, clear content groups.

### 6.2 Content fetch (debounced)

- **History** (only if `store.get().settings!.history.enabled`):
  `invoke<HistoryRow[]>("history_query", { search: q, limit: 5, offset: 0 })`.
  Server-side SQL `LIKE` already bounds the result. Returns `[]` when disabled
  (R6 — no error), but the client-side gate avoids the round-trip entirely.
- **Dictionary**:
  `invoke<DictEntry[]>("dictionary_list")` → **cached per open** (§6.3) →
  client-side filter: `entry` OR `replacement` contains `q` (case-insensitive)
  → top 5.

`Promise.all` both; render their groups on resolve (subject to the
`queryId` race guard). Either failing → its group renders empty (a failed
fetch must not blank the whole dropdown); the error is logged `console.error`
(metadata-only — error object, never transcript/dict content beyond what the
user already sees).

### 6.3 Dictionary cache (Blaze)

`dictionary_list` returns the **entire** dictionary. Re-fetching it on every
150 ms-debounced query is wasteful for large dicts. Cache strategy:

- On the **first query of the current open** (dropdown transitions hidden→
  shown with a non-empty query), fetch `dictionary_list` once, store in a
  local `dictCache: DictEntry[] | null`.
- Subsequent keystrokes while the dropdown stays open **reuse** `dictCache`
  (client-side filter only — no IPC).
- **Invalidate** when the dropdown closes (hide/Esc/blur/select →
  `dictCache = null`). Next open re-fetches (cheap, one IPC; picks up any
  edits made in the Dictionary section meanwhile).

History is not cached — `history_query` is server-side filtered + bounded to
5 rows, so each query is cheap and reflects live state.

### 6.4 Open / close lifecycle

- **Open**: dropdown becomes visible on focus (if query non-empty) or on the
  first keystroke.
- **Close** on any of: `Esc` (keydown on input), `blur` (150 ms delayed —
  so a click on an item registers before the close), item activation
  (select), or empty query.
- On close: cancel pending debounce timer, invalidate `dictCache`, clear
  `aria-expanded`, set `hidden` on the listbox, reset `aria-activedescendant`.

### 6.5 Section index build + lang refresh

```
type SectionIndex = { id: string; title: string; keywords: string[] }[];

function buildIndex(): SectionIndex {
  return SECTIONS.map((s) => ({
    id: s.id,
    title: t(`nav.${s.id}`),
    keywords: KEYWORDS[s.id] ?? [],
  }));
}
```

`KEYWORDS: Record<string, string[]>` is a static, mostly language-neutral
table of technical terms per section (lowercased):

| Section | Sample keywords |
|---|---|
| recognition | vad, rtf, model, engine, nemotron, gigaam, language, energy, chunk, threshold |
| microphone | mic, device, noise, input, level, preview |
| text | paste, clipboard, replace, type, polish, smart, raw, mode |
| dictionary | entry, replacement, word, correct, expand |
| history | history, log, record, retention, entries, days, erase |
| hotkey | hotkey, ptt, push-to-talk, alt, shortcut, altgr |
| overlay | overlay, caption, bubble, edit, paste-failed |
| updates | update, version, check, download |
| about | about, credits, version, links |

The index is rebuilt on `ui_lang` change (title is localized). `main.ts`
exports a `refreshSearchLang()` that `rerender()` calls alongside sidebar
re-translation; it triggers `index = buildIndex()` + a re-filter if the
dropdown is open. (Keywords stay language-neutral — a Russian user typing
"vad" or "модель" both match; "vad" matches the keyword, "модель" matches
the localized title `recognition` → "Распознавание" wouldn't, but the
keyword list + title together cover the common cases.)

## 7. Matching semantics

- Query: `q.trim().toLowerCase()`. If empty → dropdown hidden.
- **Section match**: `q` is a substring of `title.toLowerCase()` OR of any
  keyword (which are already lowercased). A section matches if any one
  succeeds.
- **History match**: server-side SQL `LIKE %q%` on `text` (existing
  `history_query` semantics — no client change).
- **Dictionary match**: `q` is a substring of `entry.toLowerCase()` OR
  `replacement.toLowerCase()`.
- No scoring/ranking within a group; groups render in a fixed order
  (sections, then history, then dictionary). Within a group, natural order
  (index order / server order / cache order). Top-N per group (sections: all
  ≤9; history: 5; dictionary: 5).

## 8. Keyboard navigation + a11y

- **`ArrowDown` / `ArrowUp`**: move `activeIndex` across the **flat** option
  list (all rendered options across groups, in DOM order). Wrap at the ends.
  Update `aria-activedescendant` to the active option's id; scroll into view
  if needed. **Clamp at the ends — no wrap** (WAI-ARIA APG: "does nothing" at
  first/last option). **`activeIndex` resets to -1 (no active) on every
  dropdown re-render** so Enter never activates a stale option from a prior
  result set.
- **`Enter`**: if an option is active, activate it (navigate); else no-op.
- **`Esc`**: close the dropdown (focus stays in the input; query retained).
- **`Home`/`End`**: YAGNI — skip (standard `type=search` provides native
  caret Home/End; the dropdown doesn't need its own).
- Input: `role="combobox"` + `aria-haspopup="listbox"` (**WAI-ARIA APG
  combobox pattern** — not `searchbox`; the popup association requires the
  combobox role; verified 2026-08-06 against w3.org/WAI/ARIA/apg/patterns/
  combobox), `aria-expanded`, `aria-controls="search-listbox"`,
  `aria-autocomplete="list"`, `aria-activedescendant`. **DOM focus stays on
  the input; AT focus moves within the listbox via `aria-activedescendant`**
  (APG: "Managing Focus in Composites Using aria-activedescendant").
- Listbox: `role="listbox"`. Items: `role="option"`, `aria-selected` on the
  active.
- Visible focus cue on items + input via `:focus-visible` using `--accent`
  (#0E7C86, AA-safe), not `--accent-bright`.
- Mouse hover sets active too (so click + keyboard share one active state).

## 9. i18n

New keys (canonical EN; all 36 locales; placed in a new `search.*` cluster,
alphabetical, after the `recognition.*` cluster and before `command.*` —
adjacent to the feature's home):

| Key | EN |
|---|---|
| `search.placeholder` | `Search settings…` |
| `search.sections` | `Sections` |
| `search.history` | `History` |
| `search.dictionary` | `Dictionary` |
| `search.no_results` | `No results` |

5 keys × 36 locales = 180 additions. Set-equality invariant (en canonical;
every locale's key set === en's). `{…}` interpolation tokens: none in v1.
RTL (ar, he): keep ASCII group-label text as-is (they're navigational
labels; the existing `nav.*` labels already translate, and `search.*` labels
translate per-locale normally).

## 10. Privacy (§10.1)

- **No new logging.** The dropdown renders the user's own history text +
  dictionary entries in their own settings UI — that is **display**, not
  logging; §10.1 governs logs, not on-screen rendering.
- `history_query` + `dictionary_list` are existing IPC; no new substrate.
- The only `console.*` calls are `console.error` on IPC failure, logging the
  **error object only** (matches the existing pattern in `history.ts:169` /
  `dictionary.ts:41`). Never logs transcript/dict content (the error doesn't
  carry it).
- No new `tests/log_privacy.rs` substrate is required (this is a
  frontend-only feature with no Rust logging changes).

## 11. Performance / Blaze

This feature is entirely off the inference hot path (it's a settings-window
UI control). The Blaze mandate applies to **search UX snappiness** and to
**not regressing** the default dictation path:

- **Default RU/PTT/Smart path: byte-for-byte untouched.** No Rust, no engine,
  no pipeline, no settings-fields change. The search box mounts once at
  settings-window init; it does nothing unless the user focuses it.
- **Instant section tier**: in-memory index, synchronous substring filter —
  sub-millisecond, zero IPC. The user sees section matches on every keystroke
  with no perceivable delay (better than competitor settings search that
  re-renders a full page or round-trips every key).
- **Bounded content tier**: history is server-side SQL-filtered to ≤5 rows;
  dictionary is cached per-open (one IPC, then client-side filter). 150 ms
  debounce coalesces rapid typing.
- **Bounded DOM**: ≤ 9 (sections) + 5 (history) + 5 (dictionary) ≈ 19 option
  nodes max, re-rendered via `replaceChildren` (no incremental append → no
  leak). Dropdown is absolutely positioned → no sidebar reflow while typing.
- **Race-safe**: stale content resolves are dropped (`queryId` guard) → no
  flicker from out-of-order IPC.
- No `setInterval` / polling anywhere. Event-driven (input/focus/blur/keydown
  only).

## 12. CSS (settings.css additions)

Logical properties throughout; AA palette; RTL-safe. Approximate rules:

- `.search-box` — form wrapper; `margin-block-end` to space it from the
  section buttons.
- `input[type="search"]` — full width; existing input styling baseline;
  `:focus-visible` ring in `--accent`.
- `.search-dropdown` — `position: absolute`; `inset-inline-start: 0`;
  `inset-block-start: 100%`; `min-inline-size: 100%`; `max-block-size: 60vh`;
  `overflow-y: auto`; `background: var(--bg)`; border + shadow;
  `z-index` above sidebar; `border-radius`.
- `.search-group` + `.search-group-title` — group label (small, muted).
- `.search-item` — row; `:hover` + `.active` (keyboard) share the same
  `--accent` tint background; cursor pointer.
- `.search-no-results` — muted, centered.
- `prefers-reduced-motion`: no transitions on show/hide (the dropdown appears
  instantly).

## 13. Files touched

| File | Change |
|---|---|
| `src/settings/federated-search.ts` | NEW — the whole feature |
| `src/settings/main.ts` | Mount `el` at top of `#sidebar`; export `refreshSearchLang()` + call from `rerender()` |
| `src/settings.css` | `.search-*` rules |
| `src/i18n/locales/*.ts` (×36) | `+search.*` cluster (5 keys) |

No Rust, no `Cargo.toml`/`Cargo.lock`, no `package.json`, no capabilities
file (no new events/commands).

## 14. Gates + smoke

- `npx tsc --noEmit` exit 0.
- `npm run build` exit 0.
- (No `cargo` — Rust untouched.)
- i18n set-equality: `search.*` keys present in all 36 locales; `en`
  canonical.
- **Manual GUI smoke** (the real gate — no JS test runner):
  1. Section search: type "vad" → Recognition appears; "paste" → Text; "mic"
     → Microphone; "update" → Updates. Click → navigates.
  2. History search (history enabled): type a known phrase → inline history
     matches with timestamp; click → History section opens.
  3. Dictionary search: type a known entry/word → inline dictionary matches;
     click → Dictionary section opens.
  4. No results: type gibberish → single "No results" row.
  5. Keyboard: ↑/↓ moves active, Enter navigates, Esc closes, Tab away
     closes (blur).
  6. RTL: switch UI to `ar` / `he` → dropdown anchors inline-start, items
     read RTL, no overflow.
  7. Lang switch: search in RU ("микрофон") → matches via localized title;
     switch to EN → index rebuilds, "microphone" matches.
  8. History disabled: no history group renders (gate holds).
  9. Default dictation path intact: hotkey dictation still pastes (the search
     feature is inert when the settings window isn't focused).

## 15. Deferred / future

- **History-seed on navigate** (§2): click a history result → navigate +
  pre-fill History's search via a loose event bus (`main.ts` already uses
  `navigate-history`; a sibling `seed-history-search` event + a ~3-line
  listener in `history.ts` would do it). Deferred — keep v1 non-invasive.
- **Snippets/profiles federation**: blocked on those features getting IPC +
  UI (Task 12+ / dedicated task).
- **Dictionary in-section filter**: dictionary has no filter input today;
  adding one is Task 12 ("Dictionary live filter"). When it exists, the
  history-seed pattern generalizes to dict too.
- **Command palette (`Ctrl+K`)**: the Phase-3 plan's deferred Task S2; this
  dropdown is the foundation (a palette is the same component triggered by a
  hotkey + spanning actions, not just settings nav).
