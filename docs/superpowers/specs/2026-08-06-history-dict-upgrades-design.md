# Task 12 — History + Dictionary upgrades (design spec)

**Date:** 2026-08-06
**Branch:** `phase3` (HEAD `f5702b2` at design time)
**Predecessors:** Tasks 1-11 + 14 shipped + review-clean + human-smoke-verified.
**Plan:** `docs/superpowers/plans/2026-08-06-history-dict-upgrades.md` (written after this spec).
**Source plan (high-level):** `docs/superpowers/plans/2026-08-05-molvi-phase3.md` §Task 12 (8 steps, no verbatim code — this spec + the implementation plan fill the gap).
**UX research:** `docs/phase-3-ux-research.md` §6.

## 1. Goal + scope

Make History + Dictionary first-class (UX §6). Six features; the plan's Step 1
(`ru-RU` locale bug) is already DONE (commit `06edb28`, Session-7 audit item 2)
and is skipped.

| # | Feature | Surface |
|---|---------|---------|
| 1 | History row expand (lazy, per-row) + lang/date filter chips | `history.ts` + Rust IPC widen |
| 2 | History keyboard nav (roving-tabindex composite) | `history.ts` |
| 3 | History bulk select + bulk delete | `history.ts` + new Rust IPC |
| 4 | Toaster action button (primitive) | `ui.ts` + CSS |
| 5 | Dictionary live filter + undo-delete (5s) | `dictionary.ts` (uses #4) |
| 6 | Dictionary import preview (N new, M conflicts) | `dictionary.ts` + Rust IPC split |

**Out of scope (decided):**
- **History single-row undo-delete.** Architecturally unclean for low-value
  ephemeral data. History rows are keyed by an autoincrement PK and ordered by
  `created_at`; "undo" means either a soft-delete schema migration
  (`deleted_at` column + `WHERE deleted_at IS NULL` filter) or a restore-IPC
  that re-inserts with a NEW id (breaks the original timestamp/position;
  `re_paste` by old id would 404). Both are over-engineering for data that
  auto-regenerates on every dictation and is auto-pruned by retention. Dict
  undo is a clean inverse (entries keyed by `entry` text → remove + re-add =
  identical). History single-row delete stays **instant** (deliberate row-by-
  row action); history BULK delete uses the existing `twoStepConfirm` (already
  the pattern for clear/erase).

## 2. Hard constraints (unchanged from Phase-3)

- **Blaze ratchet:** default RU/PTT/Smart path byte-untouched. History/dict are
  opt-in settings sections — no hot-path code. All new Rust is off the
  dictation path.
- **Privacy §10.1:** NEVER log transcript/dict/history content — any level.
  `console.error` logs the error object only (metadata). The UI DISPLAYS the
  user's own data — display ≠ logging.
- **NO new dependencies.** Rust = stdlib + existing `rusqlite`. Frontend =
  vanilla TS (no framework, no lib).
- **Backward compat NOT needed** (clean breaks; `#[serde(default)]` regenerates).
- **Docs-first:** WAI-ARIA APG (listbox + keyboard-interface) verified live
  2026-08-06 before this spec. Tauri 2 IPC camelCase rule (Session-8 verified).
- **Ponytail FULL:** smallest diff, stdlib/native first, `// ponytail:` for
  shortcuts.

## 3. Architecture

### 3.1 Rust IPC changes (all in `src-tauri/src/`)

| IPC | File | Change |
|---|---|---|
| `history_query` | `ipc.rs` + `history.rs` | widen: `+lang: Option<String>, +since: Option<i64>`. `history.rs::query` builds a dynamic `WHERE` from present params (see §3.2). |
| `history_bulk_delete` | `ipc.rs` + `history.rs` | **NEW**: `(ids: Vec<i64>) -> ()`. One tx, `DELETE FROM history WHERE id IN (?,…)`. |
| `history_distinct_langs` | `ipc.rs` + `history.rs` | **NEW**: `() -> Vec<String>`. `SELECT DISTINCT lang FROM history WHERE lang IS NOT NULL ORDER BY lang`. Disabled → `vec![]`. |
| `dictionary_import` | `ipc.rs` + `dictionary.rs` | **SPLIT → 2 IPCs**: `dictionary_import_preview() -> Option<ImportPreview>` (pick + parse + count, READ-ONLY) + `dictionary_import_apply(path: String) -> ()` (parse + apply). `dictionary.rs` extracts `parse_csv(path) -> Vec<(String,String)>` + `parse_json(path) -> Vec<(String,String)>`; existing `import_csv`/`import_json` become `parse_*` + `apply_vec`. |

**New type:** `ImportPreview { path: String, total: u32, new: u32, conflicts: u32 }`
(`#[derive(Debug, serde::Serialize)]`). `total` = parsed entries; `new` = keys
not in existing dict; `conflicts` = keys already present (will overwrite).

**Naming/camelCase** (Tauri 2 default — verified Session-8): all new params are
single words (`lang`, `since`, `ids`, `path`) → JS↔Rust identical, no
`rename_all` needed.

**`lib.rs invoke_handler`** registers the 3 new commands (+2 split for import).
AppState unchanged (history/dictionary stores already present).

### 3.2 `history.rs::query` dynamic WHERE

Replaces the current 2-branch (search/no-search) with a composed query:

```rust
pub fn query(
    &self,
    search: Option<&str>,
    lang: Option<&str>,
    since: Option<i64>,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistoryRow>> {
    let mut clauses: Vec<&str> = Vec::new();
    let mut params: Vec<&dyn rusqlite::ToSql> = Vec::new();
    if let Some(s) = search {
        // existing LIKE-escape logic, unchanged
        clauses.push("text LIKE ? ESCAPE '\\'");
        params.push(&escaped_pat); // built as today
    }
    if let Some(l) = lang {
        clauses.push("lang = ?");
        params.push(&l);
    }
    if let Some(c) = since {
        clauses.push("created_at >= ?");
        params.push(&c);
    }
    let where_sql = if clauses.is_empty() { String::new() }
        else { format!("WHERE {}", clauses.join(" AND ")) };
    let sql = format!(
        "SELECT id, created_at, text, lang, engine, post_mode FROM history \
         {where_sql} ORDER BY created_at DESC LIMIT ? OFFSET ?"
    );
    params.push(&limit as i64);
    params.push(&offset as i64);
    // prepare + query_map with params_from_iter(params)
}
```

Existing LIKE-escape (backslash + `%` + `_`) preserved verbatim. Existing
`NOCASE` ASCII-only caveat (Cyrillic case-sensitive) unchanged — documented,
deferred to a possible ICU extension.

> **Impl note (lifetime):** the sketch above uses `Vec<&dyn ToSql>` for clarity,
> but the escaped LIKE pattern + the lang clone are owned `String`s whose
> borrows don't outlive the function call site they're created in. The real
> impl owns the bound values in a `Vec<String>` and collects params as
> `Vec<Box<dyn ToSql>>` (boxed so `&str` + `&i64` unify), then
> `params_from_iter(params.iter().map(|b| b.as_ref()))`. See plan §12.1 Step 3b
> for the lifetime-correct verbatim code.

### 3.3 Frontend module changes

| File | Change |
|---|---|
| `src/settings/ui.ts` | `toast()` opts widens: `+action?: { label: string; onClick: () => void }`. |
| `src/settings/sections/history.ts` | Row DOM restructure (roving tabindex + expandable + checkbox); filter chips bar; keyboard handler; bulk toolbar. |
| `src/settings/sections/dictionary.ts` | Live-filter input; undo-delete on remove; 2-step import (preview → confirm). |
| `src/settings/types.ts` | `+ImportPreview` mirror (IPC row, NOT a Settings field → R4 invariant untouched, mirrors `ModelStatus`/`DictEntry`/`HistoryRow`). |
| `src/settings.css` | `.hist-row[.expanded/.focused]`, `.hist-main`, `.hist-select`, `.filter-chips`, `.chip[.active]`, `.bulk-bar`, `.toast-action`, `.dic-filter`, `.import-preview` (logical properties; `--accent` AA-safe `#0E7C86`; `--bg` not `--surface`). |
| `src/i18n/locales/*.ts` | +17 keys ×36 (see §7). |

## 4. History section

### 4.1 Row DOM (roving-tabindex composite — NOT listbox)

**APG grounding** (verified 2026-08-06, w3.org/WAI/ARIA/apg): the **listbox
pattern explicitly excludes interactive elements inside options** — "it does
not provide an accessible way to present a list of interactive elements, such
as links, buttons, or checkboxes. To present a list of interactive elements,
see the Grid Pattern." History rows contain a checkbox + repaste/delete
buttons → listbox is wrong. The APG-blessed alternative is the **roving-
tabindex composite** (APG keyboard-interface §"Managing Focus Within
Components Using a Roving tabindex"): rows are focusable via roving `tabindex`,
real `<button>`s inside are naturally Tab-reachable, the user agent auto-
scrolls the focused row into view (a benefit over `aria-activedescendant`).

```html
<div role="group" aria-label="History entries" class="hist-list">
  <div class="hist-row" tabindex="-1" data-row-id="…" role="group"
       aria-label="Entry from 2026-08-06 14:32">
    <input type="checkbox" class="hist-select"
           aria-label="Select row">           <!-- bulk select; Space toggles -->
    <div class="hist-main">                     <!-- click = expand; NOT a button -->
      <div class="hist-meta">2026-08-06 14:32 · ru · smart</div>
      <div class="hist-text">truncated 80 chars…</div>   <!-- expanded: full text -->
    </div>
    <div class="hist-actions">
      <button type="button">Repaste</button>
      <button type="button">Delete</button>
    </div>
  </div>
  …
</div>
```

- The **row** is the focusable element (`tabindex="0"` on the active row,
  `tabindex="-1"` on others — the roving-tabindex swap).
- `.hist-main` is a `<div>`, **not** a `<button>` — avoids Enter/Space conflict
  with the row's keydown handler (a button fires click on Enter/Space natively;
  the row handles those keys). Click on `.hist-main` (mousedown) = expand.
- The row's `aria-label` carries the date so AT announces the row context
  without reading the transcript text aloud by default (privacy-adjacent:
  screen-reader users hear content they navigate into, but the row label is the
  metadata, not the transcript).

### 4.2 Row expand (lazy, per-row)

Click on `.hist-main` OR `Enter` on the focused row toggles `expanded`:
- Collapsed: `.hist-text` shows `text.slice(0,80)` + CSS ellipsis.
- Expanded: full `text` (no slice), `aria-expanded="true"` on `.hist-main`.

**No IPC** — the full text is already in the `HistoryRow` from `history_query`;
the 80-char cap was a DOM heuristic, not a data limit.

### 4.3 Filter chips bar (above the list)

```
[All languages] [ru] [en] [de] …    [Today] [7d] [30d] [All time]
```

- **Lang chips**: populated from `history_distinct_langs()` (one IPC call when
  the enabled-section mounts; re-called on section re-mount). Single-select;
  active chip → `lang` param on `history_query`. First chip "All languages"
  clears the filter (`lang=None`). Hidden when only one lang exists (or none).
- **Date chips**: `[Today | 7d | 30d | All time]`. Maps to
  `since = Date.now() - N*86_400_000` (Today=0 days, 7d, 30d) or `None` (All).
  Single-select; default All.
- Both compose with the existing text-search box. Any filter change →
  `offset = 0` + re-query.
- **Filtered-empty state:** when a query/filters yield zero rows (but history is
  not actually empty), show a muted `common.no_matches` line (shared with dict;
  distinct from `history.empty` which means "history is empty").
- Chips are `<button type="button">` with `aria-pressed="true|false"` (APG
  toggle-button pattern).

### 4.4 Keyboard nav (on the list, when a row has focus)

| Key | Action |
|---|---|
| `↑` / `k` | focus previous row (roving-tabindex swap + `.focus()`) |
| `↓` / `j` | focus next row |
| `Home` | first row (APG "strongly recommended for lists >5") |
| `End` | last row |
| `Enter` | expand/collapse the focused row |
| `Delete` | delete the focused row (instant — no undo per §1) |
| `Space` | toggle the focused row's select-checkbox |
| `Tab` / `Shift+Tab` | move into/out of the row's action buttons (natural DOM order) |

**Focus persistence (APG):** after delete, focus the **next** surviving row
(or previous if last was deleted; or the empty-state message if none remain) —
never let focus fall to `body`.

All nav keys `preventDefault` when they act (so `↑`/`↓` don't scroll the page,
`Space` doesn't page-scroll, etc.). `j`/`k` are the vim-style supplement
(matches the "pro tool" feel, UX §6).

### 4.5 Bulk select + bulk delete

- Checkbox per row (`.hist-select`). `Space` on the focused row toggles it.
- **Shift+click** range select: anchor = last checkbox clicked; shift-click a
  second checkbox → select all rows between (inclusive) in DOM order.
  `// ponytail:` limited to loaded rows (pagination boundary — can't shift-
  select across pages). Shift-clicking first→last loaded row covers the
  "select this whole page" case; a dedicated "select all" button is YAGNI
  (marginally fewer clicks than shift-range, but adds the loaded-vs-whole-DB
  ambiguity that needs a tooltip to explain — net cognitive cost).
- A bulk-action bar appears when ≥1 selected: `"{n} selected" [Delete selected]
  [Clear selection]`. Delete → existing `twoStepConfirm` → `history_bulk_delete
  (ids)` → remove rows from DOM + clear selection + re-focus next row.

## 5. Dictionary section

### 5.1 Live filter

One `TextInput` at the top of the dictionary group:
- Filters the rendered list client-side on `entry` OR `replacement` substring
  (case-insensitive via `.toLowerCase()`).
- The `dictionary_list` result is cached in `renderList`'s closure — re-filter
  on each keystroke, no IPC. Mirrors the federated-search dict cache pattern.
- Empty filter → show all. No-match → a muted `common.no_matches` line
  ("No matches." — shared with history's filtered-empty state; one key, two
  sites). The unfiltered empty-dict state keeps `common.empty_dict`.

### 5.2 Undo-delete (5s, via the toaster action)

On `remove(entry)`:
1. Snapshot `{entry, replacement}` from the loaded list BEFORE the IPC.
2. Call `dictionary_remove`.
3. Remove the row from the rendered list.
4. Toast `warning`, `action: { label: t("dictionary.undo"), onClick: re_add }`,
   `durationMs: 5000`.
5. `re_add`: call `dictionary_add(entry, replacement)` + re-render. Clean
   inverse (dict keyed by `entry`).
6. If the 5s timer expires without click → toast auto-dismisses; delete is
   permanent.

The action button is focusable → existing toaster `focusin` pauses the dismiss
timer → the 5s window extends while the user Tabs to the action.

### 5.3 Import preview (2-IPC)

Import button → `dictionary_import_preview()`:
- Returns `Option<ImportPreview>` — `None` if the user cancelled the picker.
- Else show an inline confirm panel:
  `"{total} entries: {new} new, {conflicts} will overwrite existing."`
  + `[Import] [Cancel]`.
- Import → `dictionary_import_apply(path)` (the path returned from preview;
  round-tripped through JS — safe: single-user, local, just-picked, ms apart.
  `// ponytail:` file could change between preview+apply — non-issue in
  practice).
- Success toast `dictionary.imported` (existing key) + re-render.

`conflicts` = entries in the file whose key already exists (will be
overwritten). `new` = keys not present. Counted in Rust during preview (parse +
`d.list()` + set membership).

## 6. Toaster action button (primitive)

`toast(kind, message, opts?)` — `opts` widens to
`{ durationMs?: number; action?: { label: string; onClick: () => void } }`:
- Renders `<button class="toast-action" type="button">label</button>` after the
  message span, before the `×` close.
- Clicking the action calls `onClick()` then dismisses (same path as `×`:
  `pause()` + `dismiss()`).
- The action button is focusable → existing `focusin`/`focusout` pause-on-focus
  keeps the toast alive while Tabbing to + reading the action. No new pause
  logic needed.
- `aria-label` = the label text (the button text IS the accessible name; the
  `aria-label` is belt-and-suspenders for the toast `role=status`/`alert`).
- CSS: `.toast-action` uses `--accent` (AA-safe `#0E7C86`), sits inline-end of
  the message (flex row), `text-align: start`. Reduced-motion: no animation
  (button appears with the toast card).

## 7. i18n (new keys ×36 locales)

EN canonical (17 new keys). Namespace is `dictionary.*` (NOT `dict.*`) — matches
the 10 existing `dictionary.*` keys + the `nav.dictionary`/`search.dictionary`
reuse; a parallel short namespace would be confusing.

```
// common.* (shared, no-match state for history + dict filters)
"common.no_matches": "No matches.",
// history.filters.*
"history.filter_lang": "Language",
"history.lang_all": "All languages",
"history.filter_date": "Date",
"history.date_today": "Today",
"history.date_7d": "Last 7 days",
"history.date_30d": "Last 30 days",
"history.date_all": "All time",
// history.bulk.*
"history.bulk_selected": "{n} selected",
"history.bulk_delete": "Delete selected",
"history.bulk_clear": "Clear selection",
"history.select_row": "Select row",          // checkbox aria-label
// dictionary.*
"dictionary.filter": "Filter",
"dictionary.filter_ph": "Filter entries…",
"dictionary.undo": "Undo",
"dictionary.removed": "Entry removed",
"dictionary.preview_text": "{total} entries: {new} new, {conflicts} will overwrite.",
```

The preview-confirm Import button reuses the existing `common.import` (same
action label — no new key for it).

Tokens `{n}`/`{total}`/`{new}`/`{conflicts}` ASCII-verbatim in ALL locales incl
RTL (ar/he) + CJK (ja/zh/ko). Terminal punctuation per-locale (ja/zh 。, hi ।,
th none). Set-equality verified ×36 before the task closes.

## 8. Task split (for the implementation plan)

| Task | Scope | Gates |
|---|---|---|
| **12.1** | Rust IPC: widen `history_query` (+lang/since, dynamic WHERE) + `history_bulk_delete` + `history_distinct_langs` + split `dictionary_import` → preview+apply + `dictionary.rs` parse helper + `ImportPreview` type + `lib.rs` registration. | cargo fmt + `cargo clippy --all-targets -- -D warnings` + `cargo test --lib` + `cargo check --all-targets` (binary-lock safe) |
| **12.2** | Toaster action primitive (`ui.ts` + CSS `.toast-action`). | tsc + build |
| **12.3** | History: row DOM restructure (roving tabindex + expand) + filter chips (lang + date) + IPC wiring. | tsc + build |
| **12.4** | History: keyboard nav handler + bulk select + bulk delete (uses 12.1 IPC + existing `twoStepConfirm`). | tsc + build |
| **12.5** | Dictionary: live filter + undo-delete (uses 12.2) + import preview (uses 12.1). | tsc + build |
| **12.6** | i18n ×36 (propagate the §7 keys) + final whole-branch review. | tsc + build + key-set parity ×36 |

6 tasks. **Human review pause** after 12.3 (history UX is the riskiest surface)
and after 12.5 (dict complete; everything functional in EN before i18n).

Dependency order: 12.1 unblocks 12.3 + 12.5; 12.2 unblocks 12.5; 12.3 + 12.4
share the row DOM (sequential); 12.6 last.

## 9. Privacy

- All new Rust logging is metadata-only (counts, ids, param presence) — NEVER
  transcript/dict/history content. The `history_bulk_delete` log =
  `"history: bulk deleted {} ids"`; `dictionary_import_preview` log =
  `"dictionary: preview total={} new={} conflicts={}"`. Mirrors the existing
  `ipc.rs` logging discipline.
- `ImportPreview` carries counts only — no entry text crosses the IPC boundary
  beyond what `dictionary_list` already returns.
- `console.error` at the 3 new IPC call sites logs the error object (metadata),
  consistent with existing `history_query`/`dictionary_*` handlers.
- No new `tests/log_privacy.rs` substrate needed (no new pipeline path; the
  new IPCs are direct DB ops behind the existing AppState locks, same shape as
  the existing `history_delete`/`dictionary_remove` that are already covered).

## 10. Open items / deferred

- **History undo-delete** — decided out (§1). If ever revisited: the clean path
  is soft-delete (`deleted_at` column + `WHERE deleted_at IS NULL`), NOT a
  restore-IPC.
- **Full-DB "select all"** — there is no select-all button (dropped per §4.5
  ponytail: shift-click range covers loaded-rows; a button adds loaded-vs-DB
  ambiguity). A future "select all N rows across pagination" = a separate flow
  (needs a count IPC + id-batch fetch). YAGNI for v1.
- **NOCASE Unicode CI search** — pre-existing Phase-2 caveat (history.rs:173
  `// ponytail:`). Not touched by Task 12.
- **Filter chip for engine/post_mode** — lang + date are the UX-research asks
  (§6). Engine/post_mode chips are a trivial future add once the chip bar
  exists (same pattern). Deferred.
