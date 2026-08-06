//! Voice-cue → stored-block expansion store. MIRRORS `dictionary.rs` for CRUD
//! (schema, `Mutex<Option<...>>` cache, import/export, error mapping). The
//! apply transform is **whole-text equality** (NOT token substitution — spec
//! §6.3 distinction): `expand("signature")` with cue "sig" → `None`, whereas
//! `dictionary.apply` would replace "sig" inside "signature".
//! IPC-thread conn; cache crosses to the finalize side-thread (spec §6.5.5).
//! Privacy: `expand` is in-memory; never logs cue/expansion/text (spec §10.1).
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, params};

use crate::csv_util;
use crate::errors::{MolviError, Result};
use crate::paths;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnippetEntry {
    pub cue: String,
    pub expansion: String,
}

pub struct Snippets {
    conn: Connection,
    // Both `conn` and `cache` are reached from the finalize side-thread: Smart
    // `expand()` calls `list()` (→ conn) on a cold cache, then builds `cache`.
    // CRUD (add/remove) runs on the IPC thread and invalidates `cache`. The
    // outer `Arc<Mutex<Snippets>>` serializes ALL access; this inner Mutex
    // guards `cache` across threads + enables the `&self` CRUD/expand API.
    // (Mirrors `dictionary.rs` exactly.) Key is the lowercased cue; value is
    // the expansion.
    cache: Mutex<Option<HashMap<String, String>>>,
}

impl Snippets {
    fn schema(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS snippets (\
             cue TEXT PRIMARY KEY, \
             expansion TEXT NOT NULL, \
             created_at INTEGER NOT NULL)",
            [],
        )
        .map_err(|e| MolviError::Snippet(format!("schema: {e}")))?;
        Ok(())
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| MolviError::Snippet(format!("open in-memory: {e}")))?;
        Self::schema(&conn)?;
        Ok(Self {
            conn,
            cache: Mutex::new(None),
        })
    }

    pub fn open() -> Result<Self> {
        let p = paths::snippets_db_path()?;
        let conn = Connection::open(&p).map_err(|e| {
            MolviError::Snippet(format!("open {}: {e}", crate::paths::redact_appdata(&p)))
        })?;
        Self::schema(&conn)?;
        Ok(Self {
            conn,
            cache: Mutex::new(None),
        })
    }

    pub fn add(&self, cue: &str, expansion: &str) -> Result<()> {
        // Reject empty/whitespace-only cues: a "" cue would whole-text-match an
        // empty transcript. Silent no-op (mirrors dictionary's empty-entry guard).
        if cue.trim().is_empty() {
            return Ok(());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.conn
            .execute(
                "INSERT OR REPLACE INTO snippets (cue, expansion, created_at) \
                 VALUES (?, ?, ?)",
                params![cue, expansion, now],
            )
            .map_err(|e| MolviError::Snippet(format!("add: {e}")))?;
        *self.cache.lock().unwrap() = None;
        Ok(())
    }

    pub fn remove(&self, cue: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM snippets WHERE cue = ?", params![cue])
            .map_err(|e| MolviError::Snippet(format!("remove: {e}")))?;
        *self.cache.lock().unwrap() = None;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<SnippetEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT cue, expansion FROM snippets ORDER BY cue")
            .map_err(|e| MolviError::Snippet(format!("list: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SnippetEntry {
                    cue: row.get(0)?,
                    expansion: row.get(1)?,
                })
            })
            .map_err(|e| MolviError::Snippet(format!("list: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| MolviError::Snippet(format!("list: {e}")))?);
        }
        Ok(out)
    }

    /// Whole-text, case-insensitive cue match. Returns `Some(expansion)` iff
    /// the ENTIRE (trimmed, lowercased) text equals a stored cue; `None`
    /// otherwise. Privacy: in-memory only; the only log (on `list()` failure)
    /// is a metadata-only sqlite-error warning — never cue/expansion/text.
    //
    // ponytail: normalization is trim + to_lowercase only — no internal-
    // whitespace collapse, no ё-fold, no trailing-punct strip (commands::parse-
    // style). Deeper normalization is the caller's concern: the post-proc
    // wiring task can pre-normalize if ASR quirks demand it. Predictable and
    // deterministic beats clever here.
    pub fn expand(&self, text: &str) -> Option<String> {
        let mut guard = self.cache.lock().unwrap();
        if guard.is_none() {
            let entries = match self.list() {
                Ok(e) => e,
                Err(e) => {
                    // Metadata only: `e` is a sqlite error string, never cue/expansion/text.
                    tracing::warn!("snippets expand: list failed ({e})");
                    return None;
                }
            };
            let map: HashMap<String, String> = entries
                .into_iter()
                .map(|e| (e.cue.to_lowercase(), e.expansion))
                .collect();
            *guard = Some(map);
        }
        let cache = guard.as_ref().unwrap();
        cache.get(&text.trim().to_lowercase()).cloned()
    }

    pub fn import_csv(&self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| MolviError::Snippet(format!("import_csv read {}: {e}", path.display())))?;
        // RFC-4180 parse (handles quoted multi-line fields). Row 0 is the
        // header written by export_csv; rows with <2 fields (blank lines,
        // short rows) are skipped rather than erroring — lenient for external
        // files, and a quoted multi-line field is no longer split into rows.
        for row in csv_util::parse_rows(&content).into_iter().skip(1) {
            if row.len() < 2 {
                continue;
            }
            // Light trim preserves the legacy behavior for legacy/external
            // files with stray edge whitespace; RFC-4180 fields are exact.
            self.add(row[0].trim(), row[1].trim())?;
        }
        Ok(())
    }

    pub fn export_csv(&self, path: &Path) -> Result<()> {
        let entries = self.list()?;
        // RFC-4180: each field is quoted if it holds a special char, and rows
        // are CRLF-separated (canonical). Handles commas/quotes/newlines.
        let mut out = String::from("cue,expansion\r\n");
        for e in &entries {
            out.push_str(&csv_util::quote_field(&e.cue));
            out.push(',');
            out.push_str(&csv_util::quote_field(&e.expansion));
            out.push_str("\r\n");
        }
        std::fs::write(path, out).map_err(|e| {
            MolviError::Snippet(format!("export_csv write {}: {e}", path.display()))
        })?;
        Ok(())
    }

    pub fn import_json(&self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            MolviError::Snippet(format!("import_json read {}: {e}", path.display()))
        })?;
        let arr: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| MolviError::Snippet(format!("import_json parse: {e}")))?;
        for obj in arr
            .as_array()
            .ok_or_else(|| MolviError::Snippet("import_json: expected JSON array".into()))?
        {
            let cue = obj["cue"]
                .as_str()
                .ok_or_else(|| MolviError::Snippet("import_json: missing cue string".into()))?;
            let expansion = obj["expansion"].as_str().ok_or_else(|| {
                MolviError::Snippet("import_json: missing expansion string".into())
            })?;
            self.add(cue, expansion)?;
        }
        Ok(())
    }

    pub fn export_json(&self, path: &Path) -> Result<()> {
        let entries = self.list()?;
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "cue": e.cue,
                    "expansion": e.expansion,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&arr)
            .map_err(|e| MolviError::Snippet(format!("export_json serialize: {e}")))?;
        std::fs::write(path, json).map_err(|e| {
            MolviError::Snippet(format!("export_json write {}: {e}", path.display()))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> Snippets {
        Snippets::open_in_memory().unwrap()
    }

    #[test]
    fn expand_matches_whole_text_case_insensitive() {
        let s = tmp();
        s.add("brb", "be right back").unwrap();
        assert_eq!(s.expand("BRB"), Some("be right back".to_string()));
        assert_eq!(s.expand("brb"), Some("be right back".to_string()));
        assert_eq!(s.expand("Brb"), Some("be right back".to_string()));
    }

    #[test]
    fn expand_returns_none_for_non_cue() {
        let s = tmp();
        s.add("brb", "be right back").unwrap();
        assert_eq!(s.expand("hello world"), None);
    }

    /// THE distinguishing test: snippets is whole-text equality, NOT token
    /// substitution. With cue "sig", "signature" is NOT a match (dictionary's
    /// `apply` WOULD substitute "sig" inside "signature"; `expand` must not).
    #[test]
    fn expand_returns_none_for_cue_as_substring() {
        let s = tmp();
        s.add("sig", "signature block").unwrap();
        assert_eq!(s.expand("signature"), None);
        assert_eq!(s.expand("signed"), None);
        assert_eq!(s.expand("sig"), Some("signature block".to_string()));
    }

    #[test]
    fn expand_trims_whitespace() {
        let s = tmp();
        s.add("hi", "hello").unwrap();
        assert_eq!(s.expand("  hi  "), Some("hello".to_string()));
        assert_eq!(s.expand("\thi\n"), Some("hello".to_string()));
    }

    #[test]
    fn crud_roundtrip() {
        let s = Snippets::open_in_memory().unwrap();
        s.add("brb", "be right back").unwrap();
        s.add("sig", "signature block").unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].cue, "brb");
        assert_eq!(list[1].cue, "sig");
        s.remove("brb").unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].cue, "sig");
        s.remove("sig").unwrap();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn csv_export_then_import_roundtrips() {
        let s = Snippets::open_in_memory().unwrap();
        s.add("brb", "be right back").unwrap();
        s.add("sig", "signature block").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_snip_csv_{}_{}.csv",
            std::process::id(),
            line!()
        ));
        s.export_csv(&path).unwrap();

        let s2 = Snippets::open_in_memory().unwrap();
        s2.import_csv(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let orig = s.list().unwrap();
        let imported = s2.list().unwrap();
        assert_eq!(orig.len(), imported.len());
        for (a, b) in orig.iter().zip(imported.iter()) {
            assert_eq!(a.cue, b.cue);
            assert_eq!(a.expansion, b.expansion);
        }
    }

    /// THE regression for RFC-4180: a multi-line expansion (signature/address
    /// block) holding a newline, a comma, and a double-quote must survive
    /// export→import byte-for-byte. The old naive `lines()`+`splitn` parser
    /// split this into garbage rows.
    #[test]
    fn csv_export_then_import_roundtrips_multiline() {
        let s = Snippets::open_in_memory().unwrap();
        s.add("sig", "Jane Doe,\nSenior \"Engineer\",\nAcme Corp")
            .unwrap();
        s.add("brb", "be right back").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_snip_csv_ml_{}_{}.csv",
            std::process::id(),
            line!()
        ));
        s.export_csv(&path).unwrap();

        let s2 = Snippets::open_in_memory().unwrap();
        s2.import_csv(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let orig = s.list().unwrap();
        let imported = s2.list().unwrap();
        assert_eq!(orig.len(), imported.len());
        for (a, b) in orig.iter().zip(imported.iter()) {
            assert_eq!(a.cue, b.cue);
            assert_eq!(a.expansion, b.expansion);
        }
        assert_eq!(
            imported
                .iter()
                .find(|e| e.cue == "sig")
                .map(|e| e.expansion.as_str()),
            Some("Jane Doe,\nSenior \"Engineer\",\nAcme Corp")
        );
    }

    #[test]
    fn json_export_then_import_roundtrips() {
        let s = Snippets::open_in_memory().unwrap();
        s.add("brb", "be right back").unwrap();
        s.add("sig", "signature block").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_snip_json_{}_{}.json",
            std::process::id(),
            line!()
        ));
        s.export_json(&path).unwrap();

        let s2 = Snippets::open_in_memory().unwrap();
        s2.import_json(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let orig = s.list().unwrap();
        let imported = s2.list().unwrap();
        assert_eq!(orig.len(), imported.len());
        for (a, b) in orig.iter().zip(imported.iter()) {
            assert_eq!(a.cue, b.cue);
            assert_eq!(a.expansion, b.expansion);
        }
    }
}
