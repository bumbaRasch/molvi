//! Opt-in transcript history (spec §6.2). Written by the finalize side-thread,
//! read by the IPC thread -> behind `Arc<Mutex<Connection>>` (unlike dictionary,
//! which is IPC-thread-only and uses a bare `Connection`). OFF by default:
//! `open_if_enabled` returns `None` and no table is created until the user opts
//! in (privacy: nothing on disk until then).
//! Privacy (spec §10.1): NEVER logs transcript text or row contents.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};

use crate::errors::{MolviError, Result};
use crate::paths;
use crate::settings::HistorySettings;

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoryRow {
    pub id: i64,
    pub created_at: i64,
    pub text: String,
    pub lang: Option<String>,
    pub engine: Option<String>,
    pub post_mode: Option<String>,
}

pub struct History {
    // ponytail: brief sketched `History(Arc<Mutex<Connection>>)` as a tuple,
    // but a tuple can't carry the retention knobs (max_entries/max_age_days) the
    // pruner needs. Named struct; same kind of forced deviation as Task 2's
    // dictionary cache.
    conn: Arc<Mutex<Connection>>,
    max_entries: u32,
    max_age_days: u32,
}

impl History {
    fn schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT,\
                 created_at INTEGER NOT NULL,\
                 text TEXT NOT NULL,\
                 lang TEXT,\
                 engine TEXT,\
                 post_mode TEXT);\
             CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at DESC);",
        )
        .map_err(|e| MolviError::Db(format!("schema: {e}")))?;
        Ok(())
    }

    /// `None` when disabled (table NOT created -> nothing on disk). `Some(Ok)`
    /// opens `molvi.db`, creates the schema, stores the retention settings.
    pub fn open_if_enabled(settings: &HistorySettings) -> Option<Result<History>> {
        if !settings.enabled {
            return None;
        }
        Some(Self::open(settings))
    }

    pub(crate) fn open(settings: &HistorySettings) -> Result<History> {
        let p = paths::history_db_path()?;
        Self::open_at(&p, settings.max_entries, settings.max_age_days)
    }

    fn open_at(p: &Path, max_entries: u32, max_age_days: u32) -> Result<History> {
        let conn = Connection::open(p).map_err(|e| {
            MolviError::Db(format!("open {}: {e}", crate::paths::redact_appdata(p)))
        })?;
        Self::schema(&conn)?;
        Ok(History {
            conn: Arc::new(Mutex::new(conn)),
            max_entries,
            max_age_days,
        })
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::open_in_memory_with(100, 7)
    }

    pub fn open_in_memory_with(max_entries: u32, max_age_days: u32) -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| MolviError::Db(format!("open in-memory: {e}")))?;
        Self::schema(&conn)?;
        Ok(History {
            conn: Arc::new(Mutex::new(conn)),
            max_entries,
            max_age_days,
        })
    }

    /// Insert + prune in one transaction. Runs after a successful paste
    /// (Task 8 enforces ordering); this method just stores + trims.
    pub fn insert(
        &self,
        text: &str,
        lang: Option<&str>,
        engine: Option<&str>,
        post_mode: Option<&str>,
    ) -> Result<()> {
        self.insert_inner(text, unix_ms_now(), lang, engine, post_mode)
    }

    fn insert_inner(
        &self,
        text: &str,
        created_at: i64,
        lang: Option<&str>,
        engine: Option<&str>,
        post_mode: Option<&str>,
    ) -> Result<()> {
        let now = unix_ms_now();
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        let tx = guard
            .transaction()
            .map_err(|e| MolviError::Db(format!("tx begin: {e}")))?;
        tx.execute(
            "INSERT INTO history (created_at, text, lang, engine, post_mode) \
             VALUES (?, ?, ?, ?, ?)",
            params![created_at, text, lang, engine, post_mode],
        )
        .map_err(|e| MolviError::Db(format!("insert: {e}")))?;
        // count prune: keep newest `max_entries` by created_at.
        tx.execute(
            "DELETE FROM history \
             WHERE id NOT IN (\
                 SELECT id FROM history ORDER BY created_at DESC LIMIT ?\
             )",
            params![self.max_entries as i64],
        )
        .map_err(|e| MolviError::Db(format!("prune count: {e}")))?;
        // age prune: drop rows older than `max_age_days` (cutoff from real now,
        // not the row's created_at — so a backdated test row ages out correctly).
        let age_cutoff = now.saturating_sub((self.max_age_days as i64) * 86_400_000);
        tx.execute(
            "DELETE FROM history WHERE created_at < ?",
            params![age_cutoff],
        )
        .map_err(|e| MolviError::Db(format!("prune age: {e}")))?;
        tx.commit()
            .map_err(|e| MolviError::Db(format!("tx commit: {e}")))?;
        Ok(())
    }

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

        // Dynamic WHERE: each present filter contributes one clause + one owned
        // param. ponytail: Box<dyn ToSql> unifies String/str/i64 into one Vec;
        // values are MOVED into the box (not borrowed) so the trait object stays
        // 'static — borrowing a local &str would not compile (lifetime < 'static).
        let mut clauses: Vec<&str> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = search {
            // Escape LIKE wildcards so a literal %/_ in the user term is matched,
            // not treated as a wildcard. Backslash escaped first.
            let pat = format!(
                "%{}%",
                s.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            clauses.push("text LIKE ? ESCAPE '\\'");
            params.push(Box::new(pat));
        }
        if let Some(l) = lang {
            clauses.push("lang = ?");
            params.push(Box::new(l.to_string()));
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
            .query_map(
                rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())),
                map_row,
            )
            .map_err(|e| MolviError::Db(format!("query: {e}")))?;
        rows.map(|r| r.map_err(|e| MolviError::Db(format!("query row: {e}"))))
            .collect()
    }

    /// Fetch a single row by id. Used by the `re_paste` IPC command (R8a).
    /// Privacy: row text only crosses IPC inside the caller — never logged.
    pub fn get(&self, id: i64) -> Result<Option<HistoryRow>> {
        use rusqlite::OptionalExtension;
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        guard
            .query_row(
                "SELECT id, created_at, text, lang, engine, post_mode FROM history WHERE id = ?",
                params![id],
                map_row,
            )
            .optional()
            .map_err(|e| MolviError::Db(format!("get id {id}: {e}")))
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        guard
            .execute("DELETE FROM history WHERE id = ?", params![id])
            .map_err(|e| MolviError::Db(format!("delete id {id}: {e}")))?;
        Ok(())
    }

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
        let params: Vec<Box<dyn rusqlite::ToSql>> = ids
            .iter()
            .map(|i| Box::new(*i) as Box<dyn rusqlite::ToSql>)
            .collect();
        guard
            .execute(
                &sql,
                rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())),
            )
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
            .prepare("SELECT DISTINCT lang FROM history WHERE lang IS NOT NULL ORDER BY lang")
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

    /// Clear All: empty the table, keep the table itself (dictionary untouched).
    pub fn clear(&self) -> Result<()> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        guard
            .execute("DELETE FROM history", [])
            .map_err(|e| MolviError::Db(format!("clear: {e}")))?;
        Ok(())
    }

    /// Disable & Erase: `DROP TABLE history`. Caller (Task 8/IPC) also flips
    /// `settings.history.enabled = false`. One action.
    pub fn drop_table(&self) -> Result<()> {
        let guard = self
            .conn
            .lock()
            .map_err(|_| MolviError::Db("history mutex poisoned".into()))?;
        guard
            .execute("DROP TABLE IF EXISTS history", [])
            .map_err(|e| MolviError::Db(format!("drop table: {e}")))?;
        Ok(())
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryRow> {
    Ok(HistoryRow {
        id: row.get(0)?,
        created_at: row.get(1)?,
        text: row.get(2)?,
        lang: row.get(3)?,
        engine: row.get(4)?,
        post_mode: row.get(5)?,
    })
}

#[cfg(test)]
impl History {
    /// Test helper: insert with an explicit `created_at` so pruning is
    /// deterministic (production `insert` stamps `now`). Not in the public API.
    pub(crate) fn insert_at(
        &self,
        text: &str,
        created_at: i64,
        lang: Option<&str>,
        engine: Option<&str>,
        post_mode: Option<&str>,
    ) -> Result<()> {
        self.insert_inner(text, created_at, lang, engine, post_mode)
    }
}

fn unix_ms_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::HistorySettings;

    #[test]
    fn open_if_enabled_returns_none_when_disabled() {
        let s = HistorySettings {
            enabled: false,
            max_entries: 100,
            max_age_days: 7,
        };
        assert!(History::open_if_enabled(&s).is_none());
    }

    #[test]
    fn insert_prunes_to_max_entries() {
        let h = History::open_in_memory_with(5, 3650).unwrap(); // 5 rows, ~10yr age
        let base = unix_ms_now() - 1000;
        for i in 0..7 {
            h.insert_at(
                &format!("row {i}"),
                base + i,
                Some("ru"),
                Some("gigaam"),
                Some("smart"),
            )
            .unwrap();
        }
        let rows = h.query(None, None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 5, "keep only newest 5");
        assert_eq!(rows[0].text, "row 6");
        assert_eq!(rows[4].text, "row 2");
    }

    #[test]
    fn insert_prunes_old_age() {
        let h = History::open_in_memory_with(3650, 1).unwrap(); // 1 day
        let now = unix_ms_now();
        h.insert_at("fresh", now, None, None, None).unwrap();
        h.insert_at("old", now - 3 * 86_400_000, None, None, None)
            .unwrap();
        let rows = h.query(None, None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "fresh");
    }

    #[test]
    fn query_with_search() {
        let h = History::open_in_memory().unwrap();
        h.insert("привет мир", Some("ru"), None, None).unwrap();
        h.insert("до свидания", Some("ru"), None, None).unwrap();
        let rows = h.query(Some("привет"), None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "привет мир");
    }

    #[test]
    fn query_with_special_chars() {
        // LIKE wildcards in the user search term must be matched literally,
        // not treated as wildcards (which would over-broaden the match).
        let h = History::open_in_memory().unwrap();
        h.insert("discount 50% off_d", None, None, None).unwrap();
        h.insert("nothing here", None, None, None).unwrap();
        let rows = h.query(Some("50%"), None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1, "literal % must not act as a wildcard");
        assert_eq!(rows[0].text, "discount 50% off_d");
        let rows = h.query(Some("off_d"), None, None, 100, 0).unwrap();
        assert_eq!(rows.len(), 1, "literal _ must not act as a wildcard");
        assert_eq!(rows[0].text, "discount 50% off_d");
    }

    #[test]
    fn delete_removes_row() {
        let h = History::open_in_memory().unwrap();
        h.insert("one", None, None, None).unwrap();
        let id = h.query(None, None, None, 100, 0).unwrap()[0].id;
        h.delete(id).unwrap();
        assert!(h.query(None, None, None, 100, 0).unwrap().is_empty());
    }

    #[test]
    fn clear_keeps_table_removes_rows() {
        let h = History::open_in_memory().unwrap();
        h.insert("a", None, None, None).unwrap();
        h.insert("b", None, None, None).unwrap();
        h.clear().unwrap();
        assert!(h.query(None, None, None, 100, 0).unwrap().is_empty());
        // table still usable afterwards
        h.insert("c", None, None, None).unwrap();
        assert_eq!(h.query(None, None, None, 100, 0).unwrap().len(), 1);
    }

    #[test]
    fn drop_table_removes_table() {
        let h = History::open_in_memory().unwrap();
        h.insert("x", None, None, None).unwrap();
        h.drop_table().unwrap();
        assert!(h.query(None, None, None, 100, 0).is_err());
    }

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
        h.insert_at("old", now - 10 * 86_400_000, None, None, None)
            .unwrap();
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
        h.insert_at("привет мир", now, Some("ru"), None, None)
            .unwrap();
        h.insert_at("пока мир", now, Some("ru"), None, None)
            .unwrap();
        h.insert_at("hello world", now, Some("en"), None, None)
            .unwrap();
        let rows = h
            .query(Some("мир"), Some("ru"), Some(now - 1000), 100, 0)
            .unwrap();
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
        // Distinct created_at via insert_at (not insert) so the newest-first
        // ordering is deterministic — three insert()s in one ms would tie and
        // make take(2) grab an unpredictable pair.
        let base = unix_ms_now();
        h.insert_at("a", base - 2, None, None, None).unwrap();
        h.insert_at("b", base - 1, None, None, None).unwrap();
        h.insert_at("c", base, None, None, None).unwrap();
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
}
