# History + Dictionary Upgrades Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make History + Dictionary first-class settings sections (UX research §6): row expand + filters + keyboard nav + bulk delete for history; live filter + undo-delete + import preview for dictionary.

**Architecture:** Mixed Rust + frontend. New off-hot-path Rust IPC (history filter/bulk/distinct-langs, dictionary import split) + frontend section rewrites (roving-tabindex list, toaster action primitive). Default RU/PTT/Smart path byte-untouched. No new deps.

**Tech Stack:** Rust (rusqlite, stdlib) + vanilla TS (no framework) + WAI-ARIA roving-tabindex composite (NOT listbox — APG excludes interactive-element options).

**Spec:** `docs/superpowers/specs/2026-08-06-history-dict-upgrades-design.md` (commit `66f608f`).

## Global Constraints

- **Blaze ratchet:** default RU/PTT/Smart path byte-untouched. History/dict are opt-in settings sections — no hot-path code. All new Rust is off the dictation path.
- **Privacy §10.1:** NEVER log transcript/dict/history content — any level. `console.error` logs the error object only. Logging in new Rust IPC = metadata only (counts, ids, param presence).
- **NO new dependencies.** Rust = stdlib + existing `rusqlite`/`serde`/`csv_util`. Frontend = vanilla TS.
- **Backward compat NOT needed** (clean breaks; `#[serde(default)]` regenerates). Existing `import_csv`/`import_json` Rust fns are kept as thin wrappers so existing tests stay green (zero-cost).
- **Tauri 2 IPC camelCase** (Session-8 verified): all new params are single words (`lang`, `since`, `ids`, `path`) → JS↔Rust identical, no `rename_all`.
- **Ponytail FULL:** smallest diff, stdlib/native first, `// ponytail:` for shortcuts.
- **Gates:** Rust task → `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test --lib` + `cargo check --all-targets` (binary-lock safe if dev app running — do NOT kill it). Frontend tasks → `npx tsc --noEmit` + `npm run build`. Human GUI smoke is the behavioral gate (no JS test runner).
- **i18n:** keys use the existing `dictionary.*` namespace (NOT `dict.*` — the spec's short form was a naming slip; `dictionary.*` matches the 10 existing keys + the `nav.dictionary`/`search.dictionary` reuse). 17 new keys total.
- **Working tree:** has known uncommitted churn (`AGENTS.md`, `docs/superpowers/plans/session-3-handoff.md` CRLF, `src-tauri/tauri.conf.json` dim experiment). NEVER stage these — commit only the files each task touches.

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `src-tauri/src/history.rs` | `query` widen (dynamic WHERE) + `bulk_delete` + `distinct_langs` | 12.1 |
| `src-tauri/src/dictionary.rs` | `parse_csv_vec`/`parse_json_vec`/`apply_vec` extract + `ImportPreview` type + `preview_import` count | 12.1 |
| `src-tauri/src/ipc.rs` | widen `history_query` + `history_bulk_delete` + `history_distinct_langs` + split `dictionary_import` → preview+apply | 12.1 |
| `src-tauri/src/lib.rs` | register 3 new commands in `invoke_handler` | 12.1 |
| `src/settings/ui.ts` | `toast()` opts gains `action?: {label, onClick}` | 12.2 |
| `src/settings/sections/history.ts` | row DOM restructure (roving tabindex + expand) + filter chips + keyboard nav + bulk select/delete | 12.3, 12.4 |
| `src/settings/sections/dictionary.ts` | live filter + undo-delete + import preview | 12.5 |
| `src/settings/types.ts` | `ImportPreview` mirror (IPC row, NOT a Settings field) | 12.5 |
| `src/settings.css` | `.toast-action`, `.hist-row`, `.hist-main`, `.filter-chips`, `.chip`, `.hist-select`, `.bulk-bar`, `.dic-filter`, `.import-preview` | 12.2-12.5 |
| `src/i18n/locales/*.ts` | 17 new keys ×36 | 12.6 |

---

## Task 12.1: Rust IPC — history filters + bulk delete + distinct langs + dictionary import split

**Role:** All Rust changes, off the hot path. TDD (cargo test --lib is the gate).

**Files:**
- Modify: `src-tauri/src/history.rs`, `src-tauri/src/dictionary.rs`, `src-tauri/src/ipc.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: existing `history.rs::query` (2-branch search/no-search), `dictionary.rs::import_csv`/`import_json`/`list`, `csv_util::parse_rows`, `ipc.rs::pick_open_path`/`dict_format_for_path`/`DictFormat`.
- Produces (signatures later tasks rely on):
  - `history::History::query(&self, search: Option<&str>, lang: Option<&str>, since: Option<i64>, limit: u32, offset: u32) -> Result<Vec<HistoryRow>>`
  - `history::History::bulk_delete(&self, ids: &[i64]) -> Result<()>`
  - `history::History::distinct_langs(&self) -> Result<Vec<String>>`
  - `dictionary::Dictionary::parse_csv_vec(path: &Path) -> Result<Vec<(String,String)>>` (pure parse, no DB write)
  - `dictionary::Dictionary::parse_json_vec(path: &Path) -> Result<Vec<(String,String)>>` (pure parse)
  - `dictionary::Dictionary::apply_vec(&self, entries: &[(String,String)]) -> Result<()>`
  - `dictionary::Dictionary::preview_import(path: &Path) -> Result<ImportPreview>`
  - `dictionary::ImportPreview { path: String, total: u32, new: u32, conflicts: u32 }` (`#[derive(Debug, serde::Serialize)]`)
  - IPC: `history_query(search, lang, since, limit, offset)`, `history_bulk_delete(ids)`, `history_distinct_langs()`, `dictionary_import_preview()`, `dictionary_import_apply(path)`.

- [ ] **Step 1: Write failing tests in `history.rs` (query lang/since + bulk_delete + distinct_langs)**

Append to the `#[cfg(test)] mod tests` block in `src-tauri/src/history.rs`:

```rust
    #[test]
    fn query_filters_by_lang() {
        let h = History::open_in_memory().unwrap();
        h.insert("привет", Some("ru"), None, None).unwrap();
        h.insert("hello", Some("en"), None, None).unwrap();
        let rows = h.query(None, Some("ru"), None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "привет");
    }

    #[test]
    fn query_filters_by_since() {
        let h = History::open_in_memory().unwrap();
        let now = unix_ms_now();
        h.insert_at("old", now - 10 * 86_400_000, None, None, None).unwrap();
        h.insert_at("new", now, None, None, None).unwrap();
        let cutoff = now - 7 * 86_400_000;
        let rows = h.query(None, None, Some(cutoff), 100, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "new");
    }

    #[test]
    fn query_combines_search_lang_since() {
        let h = History::open_in_memory().unwrap();
        let now = unix_ms_now();
        h.insert_at("привет мир", now, Some("ru"), None, None).unwrap();
        h.insert_at("пока мир", now, Some("ru"), None, None).unwrap();
        h.insert_at("hello world", now, Some("en"), None, None).unwrap();
        let rows = h.query(Some("мир"), Some("ru"), Some(now - 1000), 100, 0).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn query_null_lang_matches_any_including_null() {
        let h = History::open_in_memory().unwrap();
        h.insert("no lang", None, None, None).unwrap();
        h.insert("with lang", Some("ru"), None, None).unwrap();
        let rows = h.query(None, None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn bulk_delete_removes_only_listed_ids() {
        let h = History::open_in_memory().unwrap();
        h.insert("a", None, None, None).unwrap();
        h.insert("b", None, None, None).unwrap();
        h.insert("c", None, None, None).unwrap();
        let rows = h.query(None, None, None, 100, 0).unwrap();
        // newest-first: [c, b, a]
        let ids: Vec<i64> = rows.iter().take(2).map(|r| r.id).collect();
        h.bulk_delete(&ids).unwrap();
        let after = h.query(None, None, None, 100, 0).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].text, "a");
    }

    #[test]
    fn bulk_delete_empty_is_noop() {
        let h = History::open_in_memory().unwrap();
        h.insert("a", None, None, None).unwrap();
        h.bulk_delete(&[]).unwrap();
        assert_eq!(h.query(None, None, None, 100, 0).unwrap().len(), 1);
    }

    #[test]
    fn distinct_langs_returns_sorted_unique() {
        let h = History::open_in_memory().unwrap();
        h.insert("a", Some("en"), None, None).unwrap();
        h.insert("b", Some("ru"), None, None).unwrap();
        h.insert("c", Some("en"), None, None).unwrap();
        h.insert("d", None, None, None).unwrap(); // null lang excluded
        let langs = h.distinct_langs().unwrap();
        assert_eq!(langs, vec!["en".to_string(), "ru".to_string()]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib history::tests::query_filters_by_lang history::tests::bulk_delete_removes_only_listed_ids history::tests::distinct_langs_returns_sorted_unique`
Expected: COMPILE FAIL — `query` now takes 3 params but tests pass 5; `bulk_delete`/`distinct_langs` don't exist. (This confirms the tests reference the new API.)

- [ ] **Step 3: Widen `history.rs::query` + add `bulk_delete` + `distinct_langs`**

Replace the existing `query` method in `src-tauri/src/history.rs` (the whole method, currently lines ~150-196 — from `/// Paginated` through the closing brace before `get`) with:

```rust
    /// Paginated `ORDER BY created_at DESC`. Optional case-insensitive LIKE on
    /// text + optional lang/since filters, composed into one WHERE. Each filter
    /// is independently optional (None = no constraint).
    pub fn query(
        &self,
        search: Option<&str>,
        lang: Option<&str>,
        since: Option<i64>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<HistoryRow>> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;

        // Build a dynamic WHERE from present filters. Conditions join with AND;
        // if none, the WHERE clause is empty (matches all rows, like the old
        // no-search branch). The LIKE-escape for the search term is preserved
        // verbatim from the pre-widen impl.
        let mut clauses: Vec<String> = Vec::new();
        // ponytail: Box<dyn ToSql> lets us push params of different types
        // (&String, &i64, &str) into one Vec for params_from_iter.
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::new();

        if let Some(s) = search {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            clauses.push(format!("text LIKE ? ESCAPE '\\'"));
            // ponytail: leak the owned String into the params Vec's lifetime via
            // a local that outlives the query_map call. Box it so the address is
            // stable; store the Box alongside to keep it alive.
            params.push(&escaped);
        }
        // ↑ BUG-PRONE: the escaped String is dropped at end of the `if let`.
        // Fix below in Step 3b: own the bound values in a Vec<String>.
```

That inline note flags the lifetime trap. The clean version owns the bound values. Replace the WHOLE method body with the corrected version below.

- [ ] **Step 3b: Use the corrected dynamic-WHERE (owns bound String values)**

The actual replacement for the `query` method (replaces what you wrote in Step 3):

```rust
    /// Paginated `ORDER BY created_at DESC`. Optional case-insensitive LIKE on
    /// text + optional lang/since filters, composed into one WHERE. Each filter
    /// is independently optional (None = no constraint).
    pub fn query(
        &self,
        search: Option<&str>,
        lang: Option<&str>,
        since: Option<i64>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<HistoryRow>> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;

        // Build a dynamic WHERE from present filters. Owned String values live
        // in `owned` for the duration of the query_map call (borrow checker).
        let mut clauses: Vec<&str> = Vec::new();
        let mut owned: Vec<String> = Vec::new();
        // ponytail: Box<dyn ToSql> unifies &String, &str, &i64 into one Vec.
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = search {
            // Escape LIKE wildcards so a literal %/_ in the user term is matched,
            // not treated as a wildcard. Backslash escaped first.
            let escaped = s
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
                + "%"; // wrap with %...% for substring match (LIKE %term%)
            owned.push(format!("%{}%",
                s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
            let _ = escaped; // (kept for clarity; the owned[%..%] above is what's bound)
            clauses.push("text LIKE ? ESCAPE '\\'");
            // ponytail: leak each owned String into the params Vec as a borrowed
            // trait object — the String stays alive in `owned` until fn return.
            let last: &String = owned.last().unwrap();
            params.push(Box::new(last as &str));
        }
        if let Some(l) = lang {
            owned.push(l.to_string());
            clauses.push("lang = ?");
            let last: &String = owned.last().unwrap();
            params.push(Box::new(last as &str));
        }
        if let Some(cutoff) = since {
            clauses.push("created_at >= ?");
            params.push(Box::new(cutoff));
        }

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        let sql = format!(
            "SELECT id, created_at, text, lang, engine, post_mode FROM history \
             {where_sql} ORDER BY created_at DESC LIMIT ? OFFSET ?"
        );
        params.push(Box::new(limit as i64));
        params.push(Box::new(offset as i64));

        let mut stmt = guard
            .prepare(&sql)
            .map_err(|e| MolviError::Db(format!("query prepare: {e}")))?;
        // ponytail: SQLite LIKE/NOCASE fold ASCII only (datatype3 §7); Cyrillic
        // search stays case-sensitive. Full Unicode CI needs the ICU extension
        // (not in rusqlite `bundled`). Pre-existing Phase-2 caveat.
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())), map_row)
            .map_err(|e| MolviError::Db(format!("query: {e}")))?;
        rows.map(|r| r.map_err(|e| MolviError::Db(format!("query row: {e}"))))
            .collect()
    }
```

Then add `bulk_delete` + `distinct_langs` methods to `impl History` (after `delete`, before `clear`):

```rust
    /// Bulk delete by id list. One statement via `IN (?, ?, …)`. Empty slice is
    /// a no-op (caller may pass an empty selection).
    // ponytail: SQLite IN-clause variable limit (SQLITE_MAX_VARIABLE_NUMBER,
    // default 999 / 32766 in newer). History bulk-select is paginated (≤ loaded
    // rows, typically ≤50) → never near the limit. If it ever is, batch.
    pub fn bulk_delete(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("DELETE FROM history WHERE id IN ({placeholders})");
        let params: Vec<Box<dyn rusqlite::ToSql>> = ids.iter().map(|i| Box::new(*i) as Box<dyn rusqlite::ToSql>).collect();
        guard
            .execute(&sql, rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())))
            .map_err(|e| MolviError::Db(format!("bulk_delete: {e}")))?;
        Ok(())
    }

    /// Distinct non-null lang values, sorted. For the history filter chip set.
    pub fn distinct_langs(&self) -> Result<Vec<String>> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        let mut stmt = guard
            .prepare(
                "SELECT DISTINCT lang FROM history WHERE lang IS NOT NULL ORDER BY lang",
            )
            .map_err(|e| MolviError::Db(format!("distinct_langs prepare: {e}")))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| MolviError::Db(format!("distinct_langs: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| MolviError::Db(format!("distinct_langs row: {e}")))?);
        }
        Ok(out)
    }
```

- [ ] **Step 4: Run history tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib history::`
Expected: PASS (existing tests + the 7 new ones). NOTE: the existing `query_with_search`/`query_with_special_chars`/`insert_prunes_*` tests call `h.query(Some("…"), 100, 0)` (old 3-arg signature) — they will FAIL to compile. Update them to the new 5-arg form: insert `None, None,` after the search arg. E.g. `h.query(Some("привет"), None, None, 100, 0)`. The `insert_prunes_to_max_entries` test uses `h.query(None, 100, 0)` → `h.query(None, None, None, 100, 0)`. Do this for every existing `h.query(` call site in `history.rs` tests + anywhere else in the crate that calls `h.query(`.

- [ ] **Step 5: Write failing tests in `dictionary.rs` (parse_vec + preview)**

Append to the `#[cfg(test)] mod tests` block in `src-tauri/src/dictionary.rs`:

```rust
    #[test]
    fn parse_csv_vec_reads_without_writing() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("existing", "E").unwrap();
        let path = std::env::temp_dir().join(format!("molvi_dict_parse_{}_{}.csv",
            std::process::id(), line!()));
        // file with 2 entries: one conflicts with "existing", one is new
        std::fs::write(&path, "entry,replacement\r\nexisting,OVERWRITE\r\nnew,NEW\r\n").unwrap();
        let parsed = Dictionary::parse_csv_vec(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(parsed.len(), 2);
        // parse must NOT have written to the DB
        assert_eq!(d.list().unwrap().len(), 1);
    }

    #[test]
    fn preview_import_counts_new_and_conflicts() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("existing", "E").unwrap();
        let path = std::env::temp_dir().join(format!("molvi_dict_prev_{}_{}.csv",
            std::process::id(), line!()));
        std::fs::write(&path, "entry,replacement\r\nexisting,OVERWRITE\r\nnew,NEW\r\n").unwrap();
        let prev = d.preview_import(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(prev.total, 2);
        assert_eq!(prev.new, 1);
        assert_eq!(prev.conflicts, 1);
        // preview is read-only
        assert_eq!(d.list().unwrap().len(), 1);
    }

    #[test]
    fn apply_vec_inserts_all() {
        let d = Dictionary::open_in_memory().unwrap();
        let entries = vec![
            ("a".to_string(), "A".to_string()),
            ("b".to_string(), "B".to_string()),
        ];
        d.apply_vec(&entries).unwrap();
        assert_eq!(d.list().unwrap().len(), 2);
    }
```

- [ ] **Step 6: Run dictionary tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib dictionary::tests::parse_csv_vec_reads_without_writing`
Expected: COMPILE FAIL — `parse_csv_vec`/`preview_import`/`apply_vec` don't exist.

- [ ] **Step 7: Add `parse_csv_vec`/`parse_json_vec`/`apply_vec`/`preview_import` + `ImportPreview` to `dictionary.rs`**

Add the `ImportPreview` struct near the top of `src-tauri/src/dictionary.rs` (after the `DictEntry` struct definition, ~line 20):

```rust
/// Result of a dry-run import preview (read-only; no DB writes). Returned by
/// `dictionary_import_preview` IPC so the frontend can show a confirm panel.
#[derive(Debug, serde::Serialize)]
pub struct ImportPreview {
    /// The picked file path — round-tripped to `dictionary_import_apply`.
    pub path: String,
    /// Total parsed entries in the file.
    pub total: u32,
    /// Entries whose key is NOT in the existing dictionary (will be added).
    pub new: u32,
    /// Entries whose key IS already present (will overwrite the replacement).
    pub conflicts: u32,
}
```

Refactor the existing `import_csv` body. Replace the current `import_csv` method (lines ~151-168) with:

```rust
    /// Parse a CSV file into (entry, replacement) pairs WITHOUT writing.
    /// RFC-4180 parse (handles quoted multi-line fields). Row 0 is the header
    /// written by export_csv; rows with <2 fields are skipped (lenient for
    /// external files). Pure — no DB access, no cache invalidation.
    pub fn parse_csv_vec(path: &Path) -> Result<Vec<(String, String)>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            MolviError::Dictionary(format!("parse_csv read {}: {e}", path.display()))
        })?;
        let mut out = Vec::new();
        for row in csv_util::parse_rows(&content).into_iter().skip(1) {
            if row.len() < 2 {
                continue;
            }
            out.push((row[0].trim().to_string(), row[1].trim().to_string()));
        }
        Ok(out)
    }

    /// Apply a vec of (entry, replacement) pairs to the DB (INSERT OR REPLACE).
    pub fn apply_vec(&self, entries: &[(String, String)]) -> Result<()> {
        for (entry, replacement) in entries {
            self.add(entry, replacement)?;
        }
        Ok(())
    }

    /// Import a CSV file: parse + apply. (Kept as a thin wrapper so existing
    /// call sites + tests stay green; the new preview flow uses parse_csv_vec +
    /// apply_vec separately.)
    pub fn import_csv(&self, path: &Path) -> Result<()> {
        let entries = Self::parse_csv_vec(path)?;
        self.apply_vec(&entries)
    }
```

Do the same split for JSON. Replace the current `import_json` method (lines ~187-206) with:

```rust
    /// Parse a JSON file into (entry, replacement) pairs WITHOUT writing.
    /// Expects a JSON array of objects with string `entry` + `replacement`.
    pub fn parse_json_vec(path: &Path) -> Result<Vec<(String, String)>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            MolviError::Dictionary(format!("parse_json read {}: {e}", path.display()))
        })?;
        let arr: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| MolviError::Dictionary(format!("parse_json parse: {e}")))?;
        let arr = arr.as_array().ok_or_else(|| {
            MolviError::Dictionary("parse_json: expected JSON array".into())
        })?;
        let mut out = Vec::new();
        for obj in arr {
            let entry = obj["entry"].as_str().ok_or_else(|| {
                MolviError::Dictionary("parse_json: missing entry string".into())
            })?;
            let replacement = obj["replacement"].as_str().ok_or_else(|| {
                MolviError::Dictionary("parse_json: missing replacement string".into())
            })?;
            out.push((entry.to_string(), replacement.to_string()));
        }
        Ok(out)
    }

    /// Import a JSON file: parse + apply. (Thin wrapper.)
    pub fn import_json(&self, path: &Path) -> Result<()> {
        let entries = Self::parse_json_vec(path)?;
        self.apply_vec(&entries)
    }
```

Add the `preview_import` method (after `import_json`, before `export_json`):

```rust
    /// Dry-run import preview: parse the file + count new vs conflicts against
    /// the current dictionary. READ-ONLY — no DB writes, no cache invalidation.
    pub fn preview_import(&self, path: &Path) -> Result<ImportPreview> {
        let fmt = if path.extension().and_then(|e| e.to_str()) == Some("json") {
            "json"
        } else {
            "csv" // default; unsupported extensions are caught by the IPC layer
        };
        let entries = match fmt {
            "json" => Self::parse_json_vec(path)?,
            _ => Self::parse_csv_vec(path)?,
        };
        let existing: std::collections::HashSet<&str> = self
            .list()?
            .iter()
            .map(|e| e.entry.as_str())
            .collect();
        let total = entries.len() as u32;
        let conflicts = entries
            .iter()
            .filter(|(e, _)| existing.contains(e.as_str()))
            .count() as u32;
        let new = total - conflicts;
        Ok(ImportPreview {
            path: path.to_string_lossy().into_owned(),
            total,
            new,
            conflicts,
        })
    }
```

- [ ] **Step 8: Run dictionary tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib dictionary::`
Expected: PASS (existing csv/json roundtrip tests still pass via the thin `import_csv`/`import_json` wrappers + the 3 new tests).

- [ ] **Step 9: Widen `history_query` IPC + add 4 new IPC commands in `ipc.rs`**

In `src-tauri/src/ipc.rs`, replace the existing `history_query` command (lines ~220-234) with:

```rust
#[tauri::command]
pub async fn history_query(
    state: State<'_, AppState>,
    search: Option<String>,
    lang: Option<String>,
    since: Option<i64>,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistoryRow>, MolviError> {
    let g = state.history.lock().unwrap();
    let Some(h) = g.as_ref() else {
        return Ok(vec![]); // R6: disabled -> empty list
    };
    let rows = h.query(search.as_deref(), lang.as_deref(), since, limit, offset)?;
    tracing::info!("history: query returned {} rows", rows.len());
    Ok(rows)
}
```

Add three new history commands (after `history_disable_and_erase`, before the `// ── Updater ──` section):

```rust
#[tauri::command]
pub async fn history_bulk_delete(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<(), MolviError> {
    let g = state.history.lock().unwrap();
    if let Some(h) = g.as_ref() {
        h.bulk_delete(&ids)?;
        tracing::info!("history: bulk deleted {} ids", ids.len());
    }
    Ok(())
}

#[tauri::command]
pub async fn history_distinct_langs(
    state: State<'_, AppState>,
) -> Result<Vec<String>, MolviError> {
    let g = state.history.lock().unwrap();
    let Some(h) = g.as_ref() else {
        return Ok(vec![]); // disabled -> empty
    };
    let langs = h.distinct_langs()?;
    Ok(langs)
}
```

Replace the existing `dictionary_import` command (lines ~174-196) with the 2-IPC split:

```rust
#[tauri::command]
pub async fn dictionary_import_preview(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<crate::dictionary::ImportPreview>, MolviError> {
    let Some(path) = pick_open_path(&app) else {
        return Ok(None); // user cancelled the picker
    };
    if dict_format_for_path(&path).is_none() {
        return Err(MolviError::Dictionary(
            "unsupported format (use .csv or .json)".into(),
        ));
    }
    let d = state.dictionary.lock().unwrap();
    let prev = d.preview_import(&path)?;
    tracing::info!(
        "dictionary: preview total={} new={} conflicts={}",
        prev.total, prev.new, prev.conflicts
    );
    Ok(Some(prev))
}

#[tauri::command]
pub async fn dictionary_import_apply(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), MolviError> {
    let p = std::path::Path::new(&path);
    let fmt = dict_format_for_path(p)
        .ok_or_else(|| MolviError::Dictionary("unsupported format (use .csv or .json)".into()))?;
    let d = state.dictionary.lock().unwrap();
    let before = d.list()?.len();
    match fmt {
        DictFormat::Csv => d.import_csv(p)?,
        DictFormat::Json => d.import_json(p)?,
    }
    let after = d.list()?.len();
    tracing::info!(
        "dictionary: imported {} entries",
        after.saturating_sub(before)
    );
    Ok(())
}
```

Note: `dict_format_for_path` takes `&Path` — adjust the call in `dictionary_import_preview` to pass `&path` (it already does). The existing `dictionary_export` is UNCHANGED.

- [ ] **Step 10: Register the 3 new commands in `lib.rs`**

In `src-tauri/src/lib.rs`, find the `invoke_handler![` macro call (it lists all `ipc::command` names). Add the 3 new commands. Find `ipc::history_query` in the list and add after the existing history commands:

```rust
        ipc::history_query,
        ipc::history_bulk_delete,        // ← NEW
        ipc::history_distinct_langs,     // ← NEW
        ipc::history_delete,
        // … (rest unchanged)
```

And in the dictionary section of the same `invoke_handler` list, replace `ipc::dictionary_import,` with:

```rust
        ipc::dictionary_import_preview,  // ← NEW (replaces dictionary_import)
        ipc::dictionary_import_apply,    // ← NEW
```

Remove the old `ipc::dictionary_import,` line entirely (the command no longer exists).

- [ ] **Step 11: Run the full Rust gate**

Run each; all must pass:
- `cargo fmt --manifest-path src-tauri/Cargo.toml`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` (binary-lock safe; if `molvi.exe` is running, this still works — it doesn't link the final exe)

Expected: clippy clean; all lib tests pass (existing + 7 history + 3 dictionary new = 175 → 185).

If a binary lock blocks `cargo clippy --all-targets` (linking `molvi.exe` fails), use `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` + `cargo check --all-targets` instead. Do NOT kill the human's running app.

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/history.rs src-tauri/src/dictionary.rs src-tauri/src/ipc.rs src-tauri/src/lib.rs
git commit -m "feat(ipc): history filters + bulk_delete + distinct_langs + dictionary import preview/apply split"
```

---

## Task 12.2: Toaster action button primitive

**Role:** Widen `toast()` with an optional action button. Foundational for 12.5 (dict undo-delete). Pure frontend.

**Files:**
- Modify: `src/settings/ui.ts`, `src/settings.css`

**Interfaces:**
- Consumes: existing `toast(kind, message, opts?)` + the toaster machinery.
- Produces: `toast(kind, message, opts?: { durationMs?: number; action?: { label: string; onClick: () => void } })`.

- [ ] **Step 1: Widen the `toast()` signature + render the action button in `ui.ts`**

In `src/settings/ui.ts`, change the `toast` export signature (currently line ~231) and the card-assembly block.

Replace this line:

```ts
export function toast(kind: ToastKind, message: string, opts?: { durationMs?: number }): void {
```

with:

```ts
export interface ToastAction { label: string; onClick: () => void; }

export function toast(
  kind: ToastKind,
  message: string,
  opts?: { durationMs?: number; action?: ToastAction },
): void {
```

Then inside the function body, find the block that builds `msg` + `close` and appends them to `card` (currently around line 239-248):

```ts
  const msg = document.createElement("span");
  msg.textContent = message;

  const close = document.createElement("button");
  close.type = "button";
  close.className = "toast-close";
  close.setAttribute("aria-label", t("toast.close"));
  close.textContent = "\u00d7";

  card.append(msg, close);
```

Replace the `card.append(msg, close);` line with:

```ts
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
```

(The `dismiss()` function is defined later in the same closure; the arrow captures it by reference, which is fine since `dismiss` is hoisted as a function declaration.)

- [ ] **Step 2: Add `.toast-action` CSS**

Append to `src/settings.css` (in the toaster section, after the existing `.toast-close` rule — find `.toast-close` and add after its block):

```css
.toast-action {
  border: none;
  background: transparent;
  color: var(--accent);
  font: inherit;
  font-weight: 600;
  padding: 2px 6px;
  margin-inline-start: 8px;
  border-radius: var(--radius-control);
  cursor: pointer;
  text-align: start;
}
.toast-action:hover {
  color: var(--accent-hover);
}
.toast-action:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
```

- [ ] **Step 3: Gate + commit**

Run: `npx tsc --noEmit` (must exit 0); `npm run build` (must exit 0).

```bash
git add src/settings/ui.ts src/settings.css
git commit -m "feat(toaster): optional action button on toast (primitive for undo-delete)"
```

---

## Task 12.3: History — row DOM restructure + expand + filter chips

**Role:** Restructure history rows (roving tabindex + expandable text) + add the lang/date filter chips bar + wire the new IPC params.

**Files:**
- Modify: `src/settings/sections/history.ts`, `src/settings.css`

**Interfaces:**
- Consumes: `history_query(search, lang, since, limit, offset)` + `history_distinct_langs()` (from 12.1); existing `HistoryRow` type; `t()`.
- Produces: a history list with focusable rows + click/Enter expand + filter chips that drive the query. (Keyboard j/k nav + bulk come in 12.4 — this task ships the DOM scaffold + filters.)

- [ ] **Step 1: Add filter-chip state + fetch distinct langs in `history.ts`**

In `src/settings/sections/history.ts`, inside `buildEnabledContent()` (the inner function starting ~line 98), find the existing closure variables (`let search: string | null = null; let offset = 0;` around line 122-123). After the `offset` declaration, add the filter state + fetch:

```ts
    // Filter state (Task 12.3): null = no filter.
    let langFilter: string | null = null;   // e.g. "ru"; null = All languages
    let sinceFilter: number | null = null;  // ms cutoff; null = All time
    const chipBar = document.createElement("div");
    chipBar.className = "filter-chips hidden"; // hidden until langs arrive (or always for date)
    const dateChips = document.createElement("div");
    dateChips.className = "filter-chips";
```

Then, after the `searchInput` is built (around line 137) but before the `wrap.append(SettingsGroup(t("history.search"), …))` line, insert the chip bars + the distinct-langs fetch. Replace the existing block that builds the search SettingsGroup (lines ~140-143):

```ts
    // H1 block 4: list (H4 foreground hint + rows + more).
    wrap.append(
      SettingsGroup(t("history.search"), [searchInput.wrap]),
      SettingsGroup(t("history.entries_title"), [listHost, moreBtn], t("history.paste_hint")),
    );
```

with:

```ts
    // Lang chips (Task 12.3): fetched once on mount; hidden if only ≤1 lang.
    void (async (): Promise<void> => {
      try {
        const langs = await invoke<string[]>("history_distinct_langs");
        renderLangChips(langs);
      } catch (e) {
        console.error("history_distinct_langs failed", e); // metadata-only
        // fail-open: no lang filter chip bar
      }
    })();

    function renderLangChips(langs: string[]): void {
      if (langs.length <= 1) { chipBar.classList.add("hidden"); return; }
      chipBar.classList.remove("hidden");
      chipBar.replaceChildren();
      const all = chip(t("history.lang_all"), () => { langFilter = null; offset = 0; void query(false); });
      all.classList.add("active");
      chipBar.append(document.createTextNode(t("history.filter_lang") + ": "), all);
      for (const lg of langs) {
        const c = chip(lg, () => { langFilter = lg; setActiveChip(chipBar, c); offset = 0; void query(false); });
        chipBar.append(c);
      }
    }

    // Date chips (Task 12.3): Today / 7d / 30d / All.
    const DAY_MS = 86_400_000;
    const dateOpts: { label: string; since: number | null }[] = [
      { label: t("history.date_today"), since: Date.now() - 0 * DAY_MS },     // today cutoff = now (only entries from today)
      { label: t("history.date_7d"), since: Date.now() - 7 * DAY_MS },
      { label: t("history.date_30d"), since: Date.now() - 30 * DAY_MS },
      { label: t("history.date_all"), since: null },
    ];
    dateChips.append(document.createTextNode(t("history.filter_date") + ": "));
    for (const opt of dateOpts) {
      const c = chip(opt.label, () => {
        sinceFilter = opt.since;
        setActiveChip(dateChips, c);
        offset = 0;
        void query(false);
      });
      if (opt.since === null) c.classList.add("active"); // All = default
      dateChips.append(c);
    }

    function chip(label: string, onClick: () => void): HTMLButtonElement {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "chip";
      b.textContent = label;
      b.setAttribute("aria-pressed", "false");
      b.addEventListener("click", () => { onClick(); });
      return b;
    }
    function setActiveChip(bar: HTMLElement, active: HTMLElement): void {
      for (const c of bar.querySelectorAll(".chip")) {
        c.classList.toggle("active", c === active);
        (c as HTMLElement).setAttribute("aria-pressed", c === active ? "true" : "false");
      }
    }

    // H1 block 4: list (H4 foreground hint + rows + more).
    wrap.append(
      SettingsGroup(t("history.search"), [searchInput.wrap, chipBar, dateChips]),
      SettingsGroup(t("history.entries_title"), [listHost, moreBtn], t("history.paste_hint")),
    );
```

- [ ] **Step 2: Wire the filter params into the `query()` closure**

In the same `buildEnabledContent()`, find the existing `query()` inner closure (line ~162). Its current body calls:

```ts
        rows = await invoke<HistoryRow[]>("history_query", {
          search, limit: PAGE_SIZE, offset,
        });
```

Replace with:

```ts
        rows = await invoke<HistoryRow[]>("history_query", {
          search, lang: langFilter, since: sinceFilter, limit: PAGE_SIZE, offset,
        });
```

- [ ] **Step 3: Restructure `renderRow` for roving tabindex + expandable text**

Replace the entire existing `renderRow` function (lines ~191-210) with:

```ts
    function renderRow(r: HistoryRow): HTMLElement {
      const row = document.createElement("div");
      row.className = "hist-row";
      row.tabIndex = -1; // roving tabindex: 0 on the focused row, -1 on others (Task 12.4 swaps)
      row.dataset.rowId = String(r.id);
      row.setAttribute("role", "group");
      const metaParts = [
        new Date(r.created_at).toLocaleString(getCurrentLang()),
        r.lang ?? "",
        r.post_mode ?? "",
      ].filter((s) => s.length > 0);

      // .hist-main is a <div>, NOT a <button> — the row itself is the focusable
      // element (Task 12.4 handles Enter/Space). Click on .hist-main = expand.
      const main = document.createElement("div");
      main.className = "hist-main";
      main.setAttribute("aria-expanded", "false");

      const meta = document.createElement("div");
      meta.className = "hist-meta";
      meta.textContent = metaParts.join(" · ");

      const text = document.createElement("div");
      text.className = "hist-text";
      const COLLAPSED = 80;
      let expanded = false;
      text.textContent = r.text.slice(0, COLLAPSED);

      main.append(meta, text);
      main.addEventListener("click", () => toggleExpand());

      function toggleExpand(): void {
        expanded = !expanded;
        text.textContent = expanded ? r.text : r.text.slice(0, COLLAPSED);
        main.setAttribute("aria-expanded", expanded ? "true" : "false");
        row.classList.toggle("expanded", expanded);
      }

      const actions = document.createElement("div");
      actions.className = "hist-actions";
      actions.append(
        Button(t("history.repaste"), () => void doRepaste(r.id)),
        Button(t("common.delete"), () => void doDelete(r.id, row)),
      );
      row.append(main, actions);
      return row;
    }
```

(The `hist-select` checkbox is added in Task 12.4 — bulk select. This task ships rows as focusable + expandable; the keyboard handler that actually moves tabindex=0 between rows also lands in 12.4.)

- [ ] **Step 4: Add the CSS for rows + chips**

Append to `src/settings.css` (replace any existing `.hist-row`/`.hist-meta`/`.hist-text` rules — they exist from the Phase-2 history section):

```css
/* History rows (Task 12.3: focusable + expandable; 12.4 adds keyboard nav + bulk) */
.hist-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 12px;
  border-block-end: 1px solid var(--border);
}
.hist-row:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
  background: var(--canvas);
}
.hist-row.expanded .hist-text {
  white-space: pre-wrap; /* show full text incl. newlines */
  -webkit-line-clamp: unset;
}
.hist-main {
  flex: 1;
  min-width: 0;
  cursor: pointer;
}
.hist-meta {
  color: var(--muted);
  font-size: 0.85em;
  margin-block-end: 2px;
}
.hist-text {
  overflow: hidden;
  text-overflow: ellipsis;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 1; /* collapsed: 1 line + ellipsis */
}

/* Filter chips (Task 12.3) */
.filter-chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-block-start: 8px;
}
.filter-chips.hidden { display: none; }
.chip {
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
  font: inherit;
  font-size: 0.85em;
  padding: 2px 10px;
  border-radius: 999px;
  cursor: pointer;
}
.chip:hover { border-color: var(--accent); }
.chip.active {
  background: var(--accent);
  color: var(--on-accent);
  border-color: var(--accent);
}
.chip:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
```

(Find the existing `.hist-row`/`.hist-meta`/`.hist-text`/`.hist-actions` rules from Phase-2 and REPLACE them — they used a different layout. Keep `.hist-actions` if it exists, or add it: `.hist-actions { display: flex; gap: 6px; flex-shrink: 0; }`.)

- [ ] **Step 5: Gate + commit**

Run: `npx tsc --noEmit` (exit 0); `npm run build` (exit 0).

```bash
git add src/settings/sections/history.ts src/settings.css
git commit -m "feat(history): row expand + lang/date filter chips + roving-tabindex scaffold"
```

---

## Task 12.4: History — keyboard nav + bulk select + bulk delete

**Role:** Add j/k/arrow/Home/End keyboard nav (roving-tabindex swap) + per-row checkbox + shift-range select + bulk-delete toolbar (uses `history_bulk_delete` from 12.1).

**Files:**
- Modify: `src/settings/sections/history.ts`, `src/settings.css`

**Interfaces:**
- Consumes: `history_bulk_delete(ids)` IPC (from 12.1); the roving-tabindex row DOM (from 12.3); existing `twoStepConfirm`.
- Produces: a keyboard-navigable, bulk-selectable history list.

- [ ] **Step 1: Add bulk-select state + the checkbox to each row**

In `src/settings/sections/history.ts` `buildEnabledContent()`, near the filter state (added in 12.3), add the bulk state:

```ts
    // Bulk-select state (Task 12.4).
    const selectedIds: Set<number> = new Set();
    let lastClickedId: number | null = null; // shift-range anchor
    const bulkBar = document.createElement("div");
    bulkBar.className = "bulk-bar hidden";
    const bulkLabel = document.createElement("span");
    bulkLabel.className = "bulk-label";
    const bulkDeleteBtn = twoStepConfirm(t("history.bulk_delete"), () => doBulkDelete());
    const bulkClearBtn = Button(t("history.bulk_clear"), () => clearSelection());
    bulkBar.append(bulkLabel, bulkDeleteBtn, bulkClearBtn);
    function refreshBulkBar(): void {
      const n = selectedIds.size;
      bulkBar.classList.toggle("hidden", n === 0);
      bulkLabel.textContent = n > 0 ? t("history.bulk_selected").replace("{n}", String(n)) : "";
    }
    function clearSelection(): void {
      selectedIds.clear();
      for (const cb of listHost.querySelectorAll<HTMLInputElement>(".hist-select")) cb.checked = false;
      refreshBulkBar();
    }
```

In `renderRow` (from 12.3), insert the checkbox as the first child of the row. After `row.dataset.rowId = String(r.id);` add:

```ts
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "hist-select";
      checkbox.setAttribute("aria-label", t("history.select_row"));
      checkbox.checked = selectedIds.has(r.id);
      checkbox.addEventListener("click", (e) => {
        // shift+click range select (loaded rows only — pagination boundary)
        if (e.shiftKey && lastClickedId !== null) {
          const ids = currentLoadedIds();
          const i = ids.indexOf(r.id);
          const j = ids.indexOf(lastClickedId);
          if (i !== -1 && j !== -1) {
            const [lo, hi] = i < j ? [i, j] : [j, i];
            for (let k = lo; k <= hi; k++) selectedIds.add(ids[k]);
          }
        } else {
          if (checkbox.checked) selectedIds.add(r.id);
          else selectedIds.delete(r.id);
        }
        lastClickedId = r.id;
        // sync all checkboxes (range select may have changed others)
        syncCheckboxes();
        refreshBulkBar();
      });
      // stop the row click (expand) from firing when clicking the checkbox
      checkbox.addEventListener("click", (e) => e.stopPropagation());
```

Then change the final `row.append(main, actions);` to `row.append(checkbox, main, actions);`.

Add the helper functions inside `buildEnabledContent` (near the other inner closures):

```ts
    function currentLoadedIds(): number[] {
      const out: number[] = [];
      for (const row of listHost.querySelectorAll<HTMLElement>(".hist-row")) {
        const id = Number(row.dataset.rowId);
        if (Number.isFinite(id)) out.push(id);
      }
      return out;
    }
    function syncCheckboxes(): void {
      for (const row of listHost.querySelectorAll<HTMLElement>(".hist-row")) {
        const id = Number(row.dataset.rowId);
        const cb = row.querySelector<HTMLInputElement>(".hist-select");
        if (cb) cb.checked = selectedIds.has(id);
      }
    }
```

- [ ] **Step 2: Insert the bulk bar into the list group**

Find the line `wrap.append(SettingsGroup(t("history.entries_title"), [listHost, moreBtn], …))` (from 12.3) and add `bulkBar` before `listHost`:

```ts
      SettingsGroup(t("history.entries_title"), [bulkBar, listHost, moreBtn], t("history.paste_hint")),
```

- [ ] **Step 3: Implement `doBulkDelete`**

Add inside `buildEnabledContent` (near `doDelete`):

```ts
    async function doBulkDelete(): Promise<void> {
      const ids = Array.from(selectedIds);
      if (ids.length === 0) return;
      try {
        await invoke("history_bulk_delete", { ids });
        selectedIds.clear();
        offset = 0;
        await query(false); // re-query resets the list + focus
        refreshBulkBar();
      } catch (e) {
        console.error("history_bulk_delete failed", e); // metadata-only
        showActionError(e);
      }
    }
```

- [ ] **Step 4: Add the keyboard handler (roving tabindex + j/k + Delete + Space + Home/End)**

Add inside `buildEnabledContent`. The handler attaches to `listHost` (delegation); it acts only when the focused element is a `.hist-row` (not when focus is inside an action button or the checkbox — those handle their own keys).

```ts
    listHost.addEventListener("keydown", (e) => {
      const row = (document.activeElement as HTMLElement | null)?.closest(".hist-row") as HTMLElement | null;
      if (!row || !listHost.contains(row)) return;
      const rows = Array.from(listHost.querySelectorAll<HTMLElement>(".hist-row"));
      const idx = rows.indexOf(row);
      if (idx === -1) return;

      const move = (i: number): void => {
        const clamped = Math.max(0, Math.min(rows.length - 1, i));
        // roving tabindex swap
        row.tabIndex = -1;
        rows[clamped].tabIndex = 0;
        rows[clamped].focus();
      };

      switch (e.key) {
        case "ArrowDown":
        case "j":
          e.preventDefault();
          move(idx + 1);
          break;
        case "ArrowUp":
        case "k":
          e.preventDefault();
          move(idx - 1);
          break;
        case "Home":
          e.preventDefault();
          move(0);
          break;
        case "End":
          e.preventDefault();
          move(rows.length - 1);
          break;
        case "Enter":
          e.preventDefault();
          // expand/collapse the focused row
          (row.querySelector(".hist-main") as HTMLElement | null)?.click();
          break;
        case "Delete": {
          e.preventDefault();
          const id = Number(row.dataset.rowId);
          void doDelete(id, row); // instant delete (no undo per spec §1); doDelete handles focus
          break;
        }
        case " ": {
          // Space toggles the row's select-checkbox (only if focus is on the row,
          // not inside a button — buttons get Space natively)
          if (document.activeElement === row) {
            e.preventDefault();
            const cb = row.querySelector<HTMLInputElement>(".hist-select");
            if (cb) {
              cb.checked = !cb.checked;
              cb.click(); // reuse the click handler (handles range/anchor logic for non-shift)
            }
          }
          break;
        }
      }
    });
```

- [ ] **Step 5: Focus management after delete (APG persistence-of-focus)**

In the existing `doDelete` function (currently lines ~212-226), after `row.remove();`, add focus handling. Replace the current body:

```ts
    async function doDelete(id: number, row: HTMLElement): Promise<void> {
      try {
        await invoke("history_delete", { id });
        const rows = Array.from(listHost.querySelectorAll<HTMLElement>(".hist-row"));
        const idx = rows.indexOf(row);
        selectedIds.delete(id);
        row.remove();
        if (listHost.children.length === 0) {
          if (offset > 0) { offset = 0; await query(false); }
          else renderRows([]);
        } else {
          // APG focus persistence: focus the next surviving row, else previous
          const remaining = Array.from(listHost.querySelectorAll<HTMLElement>(".hist-row"));
          const target = remaining[idx] ?? remaining[idx - 1] ?? null;
          if (target) {
            // roving tabindex: ensure target is the tab-0 row before focusing
            for (const r of remaining) r.tabIndex = -1;
            if (target) target.tabIndex = 0;
            target.focus();
          }
        }
        refreshBulkBar();
      } catch (e) {
        console.error("history_delete failed", e); // metadata-only
        showActionError(e);
      }
    }
```

- [ ] **Step 6: Set the initial `tabindex=0` on the first row when rendered**

In `renderRows` (the function that loops `listHost.append(renderRow(r))`), ensure the first rendered row gets `tabindex=0` so the list is Tab-reachable. After the render loop, add:

```ts
    function renderRows(rows: HistoryRow[]): void {
      if (rows.length === 0 && listHost.children.length === 0) {
        const empty = document.createElement("div");
        empty.className = "muted";
        // Distinguish "history is empty" from "filters yielded nothing".
        const hasFilter = search !== null || langFilter !== null || sinceFilter !== null;
        empty.textContent = hasFilter ? t("common.no_matches") : t("history.empty");
        listHost.append(empty);
        return;
      }
      for (const r of rows) listHost.append(renderRow(r));
      // roving tabindex: exactly one row (the first) is in the tab sequence.
      const first = listHost.querySelector<HTMLElement>(".hist-row");
      if (first) first.tabIndex = 0;
    }
```

(This also adds the filtered-empty `common.no_matches` state.)

- [ ] **Step 7: Add the CSS for checkbox + bulk bar**

Append to `src/settings.css`:

```css
/* History bulk select (Task 12.4) */
.hist-select {
  flex-shrink: 0;
  margin-block-start: 2px;
  accent-color: var(--accent);
}
.bulk-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: var(--canvas);
  border-block-end: 1px solid var(--border);
}
.bulk-bar.hidden { display: none; }
.bulk-label { color: var(--muted); font-size: 0.9em; margin-inline-end: auto; }
```

- [ ] **Step 8: Gate + commit**

Run: `npx tsc --noEmit` (exit 0); `npm run build` (exit 0).

```bash
git add src/settings/sections/history.ts src/settings.css
git commit -m "feat(history): keyboard nav (roving tabindex j/k/arrows) + bulk select + bulk delete"
```

---

## Task 12.5: Dictionary — live filter + undo-delete + import preview

**Role:** Live filter input + undo-delete toast (uses 12.2 action primitive) + import preview confirm (uses 12.1 split IPC).

**Files:**
- Modify: `src/settings/sections/dictionary.ts`, `src/settings/types.ts`, `src/settings.css`

**Interfaces:**
- Consumes: `dictionary_import_preview() -> Option<ImportPreview>` + `dictionary_import_apply(path)` (from 12.1); `toast(…, {action})` (from 12.2); existing `dictionary_list`/`dictionary_add`/`dictionary_remove`.
- Produces: `ImportPreview` TS type mirror.

- [ ] **Step 1: Add the `ImportPreview` TS mirror in `types.ts`**

In `src/settings/types.ts`, near the existing `DictEntry` type (find `export interface DictEntry`), add:

```ts
export interface ImportPreview {
  path: string;
  total: number;
  new: number;
  conflicts: number;
}
```

(This is an IPC row, NOT a Settings field — the R4 `Settings` interface is untouched.)

- [ ] **Step 2: Add the live-filter input + cache the loaded list in `dictionary.ts`**

In `src/settings/sections/dictionary.ts`, restructure `buildDictionary`. Add a module-level cache of the loaded list so the filter + undo can mutate without re-fetching. After `const listHost = document.createElement("div");` (near the top), add:

```ts
  let loaded: DictEntry[] = []; // cached for live filter + undo
  const filterInput = TextInput(
    t("dictionary.filter"), "", () => {},
    { placeholder: t("dictionary.filter_ph") },
  );
  // Re-render on each keystroke (client-side filter; no IPC).
  filterInput.wrap.querySelector("input")!.addEventListener("input", (e) => {
    const q = (e.target as HTMLInputElement).value.trim().toLowerCase();
    renderList(loaded.filter((it) =>
      it.entry.toLowerCase().includes(q) || it.replacement.toLowerCase().includes(q)
    ), q);
  });
```

- [ ] **Step 3: Widen `renderList` to accept the filter query + insert the filter input**

Replace the existing `renderList` (lines ~48-69) with:

```ts
  function renderList(items: DictEntry[], filterQ = ""): void {
    loaded = items; // cache for live filter + undo
    listHost.replaceChildren();
    if (items.length === 0) {
      const empty = document.createElement("div");
      empty.className = "muted";
      // Distinguish "dictionary empty" from "filter yielded nothing".
      empty.textContent = filterQ.length > 0 ? t("common.no_matches") : t("common.empty_dict");
      listHost.append(empty);
      return;
    }
    for (const it of items) {
      const row = document.createElement("div");
      row.className = "dic-row";
      const pair = document.createElement("button");
      pair.type = "button";
      pair.className = "dic-pair";
      pair.textContent = `${it.entry} → ${it.replacement}`;
      pair.addEventListener("click", () => beginEdit(it));
      const del = Button(t("common.delete"), () => void remove(it.entry, it.replacement));
      row.append(pair, del);
      listHost.append(row);
    }
  }
```

Insert `filterInput.wrap` into the section group. Find the existing `SettingsGroup(t("dictionary.title"), [toolRow, listHost, formRow], …)` line and add `filterInput.wrap` after `toolRow`:

```ts
  const group = SettingsGroup(
    t("dictionary.title"),
    [toolRow, filterInput.wrap, listHost, formRow],
    t("dictionary.title_tip"),
  );
```

- [ ] **Step 4: Implement undo-delete in `remove()`**

Replace the existing `remove` function (lines ~107-114) with:

```ts
  async function remove(entry: string, replacement: string): Promise<void> {
    try {
      await invoke("dictionary_remove", { entry });
      // Re-render from the cache (drops the removed entry) WITHOUT a re-fetch.
      loaded = loaded.filter((it) => it.entry !== entry);
      const q = filterInput.get().trim().toLowerCase();
      renderList(loaded.filter((it) =>
        it.entry.toLowerCase().includes(q) || it.replacement.toLowerCase().includes(q)
      ), q);
      // Undo-delete toast: 5s window. Clean inverse — dict is keyed by `entry`,
      // so dictionary_add(entry, replacement) restores it identically.
      toast("warning", t("dictionary.removed"), {
        durationMs: 5000,
        action: {
          label: t("dictionary.undo"),
          onClick: () => { void reAdd(entry, replacement); },
        },
      });
    } catch (e) {
      showError(e);
    }
  }

  async function reAdd(entry: string, replacement: string): Promise<void> {
    try {
      await invoke("dictionary_add", { entry, replacement });
      await load(); // re-fetch to restore canonical order
    } catch (e) {
      showError(e);
    }
  }
```

- [ ] **Step 5: Replace `runImport` with the 2-step preview → confirm → apply flow**

Replace the existing `runImport` function (lines ~116-124) with:

```ts
  async function runImport(): Promise<void> {
    let prev: ImportPreview | null = null;
    try {
      prev = await invoke<ImportPreview | null>("dictionary_import_preview");
    } catch (e) {
      showError(e);
      return;
    }
    if (!prev) return; // user cancelled the picker

    // Inline confirm panel replaces the import button until Import/Cancel.
    const panel = document.createElement("div");
    panel.className = "import-preview";
    const text = document.createElement("span");
    text.className = "muted";
    text.textContent = t("dictionary.preview_text")
      .replace("{total}", String(prev.total))
      .replace("{new}", String(prev.new))
      .replace("{conflicts}", String(prev.conflicts));
    const confirmBtn = Button(t("common.import"), () => void apply(prev!.path, panel));
    const cancelBtn = Button(t("common.cancel"), () => restoreImportButton(panel));
    panel.append(text, confirmBtn, cancelBtn);
    importBtn.replaceWith(panel);
  }

  async function apply(path: string, panel: HTMLElement): Promise<void> {
    try {
      await invoke("dictionary_import_apply", { path });
      toast("success", t("dictionary.imported"));
      restoreImportButton(panel);
      await load();
    } catch (e) {
      showError(e);
      restoreImportButton(panel);
    }
  }

  function restoreImportButton(panel: HTMLElement): void {
    panel.replaceWith(importBtn);
  }
```

(`importBtn` is already defined at the top of `buildDictionary` — the existing `const importBtn = Button(t("common.import"), () => void runImport());`. The `ImportPreview` type must be imported: add `ImportPreview` to the `import type { … } from "../types";` line at the top — it currently imports `DictEntry`.)

- [ ] **Step 6: Add the import type import**

At the top of `src/settings/sections/dictionary.ts`, change:

```ts
import type { DictEntry, SectionBuilder } from "../types";
```

to:

```ts
import type { DictEntry, ImportPreview, SectionBuilder } from "../types";
```

- [ ] **Step 7: Add the CSS for filter + import preview**

Append to `src/settings.css`:

```css
/* Dictionary live filter (Task 12.5) */
.dic-list .muted { padding: 8px 0; }

/* Import preview confirm panel (Task 12.5) */
.import-preview {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 8px 0;
}
.import-preview .muted { margin-inline-end: auto; }
```

- [ ] **Step 8: Gate + commit**

Run: `npx tsc --noEmit` (exit 0); `npm run build` (exit 0).

```bash
git add src/settings/sections/dictionary.ts src/settings/types.ts src/settings.css
git commit -m "feat(dictionary): live filter + undo-delete toast + import preview confirm"
```

---

## Task 12.6: i18n ×36 + final whole-branch review

**Role:** Propagate the 17 new keys to all 36 locale files + the final whole-branch review.

**Files:**
- Modify: `src/i18n/locales/en.ts` (canonical) + `src/i18n/locales/<other-35>.ts`

**Interfaces:**
- Consumes: the key set below (EN canonical).
- Produces: 36 locales with set-equal key sets.

- [ ] **Step 1: Add the 17 keys to `en.ts` (canonical)**

In `src/i18n/locales/en.ts`, find the `// common.*` cluster. Add `common.no_matches` after `common.imported` (or in the common.* cluster, alphabetically near `empty_dict`):

```ts
    "common.no_matches": "No matches.",
```

Find the `// history.*` cluster (after line ~113). After the existing `history.search_ph` (the last history key before the next cluster), add:

```ts
    "history.filter_lang": "Language",
    "history.lang_all": "All languages",
    "history.filter_date": "Date",
    "history.date_today": "Today",
    "history.date_7d": "Last 7 days",
    "history.date_30d": "Last 30 days",
    "history.date_all": "All time",
    "history.bulk_selected": "{n} selected",
    "history.bulk_delete": "Delete selected",
    "history.bulk_clear": "Clear selection",
    "history.select_row": "Select row",
```

Find the `// dictionary.*` cluster (after line ~103). After the existing `dictionary.title_tip` (the last dictionary key), add:

```ts
    "dictionary.filter": "Filter",
    "dictionary.filter_ph": "Filter entries…",
    "dictionary.undo": "Undo",
    "dictionary.removed": "Entry removed",
    "dictionary.preview_text": "{total} entries: {new} new, {conflicts} will overwrite.",
```

- [ ] **Step 2: Propagate the 17 keys to the other 35 locales**

For each of the 35 non-EN locale files in `src/i18n/locales/`, add the same 17 keys with translated values. The 17 keys:

```
common.no_matches
history.filter_lang, history.lang_all, history.filter_date,
history.date_today, history.date_7d, history.date_30d, history.date_all,
history.bulk_selected, history.bulk_delete, history.bulk_clear,
history.select_row,
dictionary.filter, dictionary.filter_ph, dictionary.undo,
dictionary.removed, dictionary.preview_text
```

Translation conventions (per AGENTS.md + Task 11/14 precedent):
- Tokens `{n}`/`{total}`/`{new}`/`{conflicts}` ASCII-verbatim in ALL locales incl RTL (ar/he) + CJK (ja/zh/ko) — they're replaced by `.replace()` in TS, must not be localized.
- Terminal punctuation per-locale: ja/zh end with `。`, hi with `।`, th no terminal period, others `.`
- RU: `common.no_matches` = "Нет совпадений."; `history.filter_lang` = "Язык"; `history.lang_all` = "Все языки"; `history.bulk_selected` = "Выбрано: {n}"; `history.bulk_delete` = "Удалить выбранное"; `dictionary.undo` = "Отменить"; `dictionary.removed` = "Запись удалена"; `dictionary.preview_text` = "Записей: {total}: {new} новых, {conflicts} будут заменены."
- DE: formal "Sie/Ihr" (Session-6 convention).
- Place each key in the SAME cluster position as en.ts (after the last history.* / dictionary.* / common.* key in that file).

Use the PowerShell UTF-8-no-BOM script pattern from Task 11.4 (read each file as raw UTF-8, detect CRLF, insert the block at the cluster anchor, write via `UTF8Encoding($false)`).

- [ ] **Step 3: Verify key-set parity ×36**

Run (PowerShell):

```powershell
$files = Get-ChildItem src/i18n/locales/*.ts
$enKeys = (Select-String -Path src/i18n/locales/en.ts -Pattern '^\s*"[^"]+":').Matches.Count
foreach ($f in $files) {
  $count = (Select-String -Path $f.FullName -Pattern '^\s*"[^"]+":').Matches.Count
  if ($count -ne $enKeys) { Write-Host "MISMATCH $($f.Name): $count vs en $enKeys" }
}
Write-Host "en keys: $enKeys"
```

Expected: no MISMATCH lines; en keys = prior count + 17.

Verify the 4 token keys exist in all 36:

```powershell
Select-String -Path src/i18n/locales/*.ts -Pattern 'history.bulk_selected' | Measure-Object | % Count   # 36
Select-String -Path src/i18n/locales/*.ts -Pattern 'dictionary.preview_text' | Measure-Object | % Count # 36
```

- [ ] **Step 4: Gate + commit**

Run: `npx tsc --noEmit` (exit 0); `npm run build` (exit 0).

```bash
git add src/i18n/locales/*.ts
git commit -m "i18n: history filters + bulk + dictionary filter/undo/preview keys (x36)"
```

- [ ] **Step 5: Final whole-branch review**

Dispatch a final code-reviewer subagent on the whole Task 12 branch (`BASE` = the pre-12.1 branch point → HEAD). Review package: commits + stat + `git diff -U10`. Give it the spec + plan + HARD constraints + the APG listbox-excludes-interactive-elements finding + the deferred-minor lines from per-task reviews.

Adjudicate findings: fix Critical/Important inline (one fix wave + scoped re-review); park Minors with a ruling each. Record the final-review verdict in the ledger.

**HUMAN SMOKE (the behavioral gate — full-feature checklist):**
1. History: click a row → expands to full text; click again → collapses. `↑/↓` + `j/k` move focus; `Home/End`; `Enter` toggles expand; `Delete` removes the focused row (focus moves to next); `Space` toggles the checkbox.
2. Filter chips: lang chips appear (if ≥2 langs in history); click "ru" → only RU rows; date "7d" → only last 7 days; compose with text search; "All languages"/"All time" clear. No-match → "No matches." message.
3. Bulk: checkbox + shift-click selects a range; bulk bar shows "N selected"; Delete selected → twoStepConfirm → rows gone; Clear selection works.
4. Dictionary: type in the filter → list filters live (entry OR replacement); no-match → "No matches."
5. Undo-delete: delete a dict entry → "Entry removed" toast with "Undo"; click Undo within 5s → entry returns; let it expire → permanent.
6. Import preview: Import → pick a CSV with some new + some existing → "{total} entries: {new} new, {conflicts} will overwrite." + Import/Cancel; Import → applies + "Imported." toast.
7. UI lang switch → all new keys localize; RTL (ar/he) chips + rows anchor correctly; reduced-motion OK.
8. Default dictation path intact (hotkey still pastes — these are settings sections).

---

## Self-review (run after writing, fix inline)

**1. Spec coverage:**
- Row expand → 12.3 Step 3. ✓
- Lang/date filters → 12.3 Steps 1-2. ✓
- Keyboard nav (roving tabindex) → 12.4 Step 4. ✓
- Bulk select + delete → 12.4 Steps 1-3. ✓
- Toaster action → 12.2. ✓
- Dict live filter → 12.5 Steps 2-3. ✓
- Dict undo-delete → 12.5 Step 4. ✓
- Dict import preview → 12.5 Step 5. ✓
- i18n ×36 → 12.6. ✓
- APG roving-tabindex (not listbox) → 12.3 Step 3 + 12.4 Step 4. ✓
- Privacy (metadata-only logging) → 12.1 IPC logs (counts/ids only). ✓
- No new deps → all Rust stdlib+rusqlite, frontend vanilla. ✓

**2. Placeholder scan:** No TBD/TODO. All code blocks are complete. (The "Step 3 / Step 3b" split in 12.1 is intentional — it flags the lifetime trap then gives the corrected version; the implementer uses 3b's code.)

**3. Type consistency:**
- `history_query(search, lang, since, limit, offset)` — Rust sig (12.1 Step 9) matches TS call (12.3 Step 2: `{ search, lang: langFilter, since: sinceFilter, limit, offset }`). ✓
- `history_bulk_delete(ids)` — Rust `Vec<i64>` (12.1 Step 9) matches TS `{ ids }` (12.4 Step 3). ✓
- `history_distinct_langs()` — Rust `Vec<String>` (12.1 Step 9) matches TS `invoke<string[]>` (12.3 Step 1). ✓
- `dictionary_import_preview() -> Option<ImportPreview>` — Rust (12.1 Step 9) matches TS `invoke<ImportPreview | null>` (12.5 Step 5). ✓
- `dictionary_import_apply(path: String)` — Rust (12.1 Step 9) matches TS `{ path }` (12.5 Step 5). ✓
- `ImportPreview` fields `{path, total, new, conflicts}` — Rust struct (12.1 Step 7) matches TS mirror (12.5 Step 1). ✓
- `dictionary.*` namespace used consistently (not `dict.*`). ✓

Plan is internally consistent. Proceed to execution.
