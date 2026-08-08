//! User-authored token->replacement dictionary store + the deterministic
//! whole-token `apply` transform consumed by Smart post-proc (Task 4).
//! IPC-thread only — bare `Connection`, no Mutex around the conn (spec §5).
//! Privacy: `apply` is in-memory; never logs text or entries (spec §10.1).
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use regex::Regex;
use rusqlite::{Connection, params};

use crate::csv_util;
use crate::errors::{MolviError, Result};
use crate::paths;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DictEntry {
    pub entry: String,
    pub replacement: String,
}

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

/// Compiled apply-transform cache: the alternation regex + a lowercase entry
/// -> replacement map for the replacement callback.
struct Cache {
    re: Regex,
    lookup: HashMap<String, String>,
}

pub struct Dictionary {
    conn: Connection,
    // Both `conn` and `cache` are reached from the finalize side-thread: Smart
    // `apply()` calls `list()` (→ conn) on a cold cache, then builds `cache`.
    // CRUD (add/remove) runs on the IPC thread and invalidates `cache`. The
    // outer `Arc<Mutex<Dictionary>>` (in pipeline.rs/ipc.rs) serializes ALL
    // access; this inner `Mutex<Option<Cache>>` additionally (a) guards `cache`
    // across those two threads and (b) gives the `&self` CRUD/apply API the
    // interior mutability it needs. Don't "simplify" to a RefCell: the cross-
    // thread access is real, and removing the inner Mutex would force `&mut`
    // through every caller for no gain.
    cache: Mutex<Option<Cache>>,
}

impl Dictionary {
    fn schema(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dictionary (\
             entry TEXT PRIMARY KEY, \
             replacement TEXT NOT NULL, \
             created_at INTEGER NOT NULL)",
            [],
        )
        .map_err(|e| MolviError::Dictionary(format!("schema: {e}")))?;
        Ok(())
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| MolviError::Dictionary(format!("open in-memory: {e}")))?;
        Self::schema(&conn)?;
        Ok(Self {
            conn,
            cache: Mutex::new(None),
        })
    }

    pub fn open() -> Result<Self> {
        let p = paths::dictionary_db_path()?;
        let conn = Connection::open(&p).map_err(|e| {
            MolviError::Dictionary(format!("open {}: {e}", crate::paths::redact_appdata(&p)))
        })?;
        Self::schema(&conn)?;
        Ok(Self {
            conn,
            cache: Mutex::new(None),
        })
    }

    pub fn add(&self, entry: &str, replacement: &str) -> Result<()> {
        // Reject empty/whitespace-only entries at the store boundary: a
        // dictionary of only empty keys would compile to a degenerate empty-
        // alternation regex that mangles text via zero-width matches.
        // Silent no-op (matches the lenient import style; UI never offers this).
        if entry.trim().is_empty() {
            return Ok(());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.conn
            .execute(
                "INSERT OR REPLACE INTO dictionary (entry, replacement, created_at) \
                 VALUES (?, ?, ?)",
                params![entry, replacement, now],
            )
            .map_err(|e| MolviError::Dictionary(format!("add: {e}")))?;
        *self.cache.lock().unwrap() = None;
        Ok(())
    }

    pub fn remove(&self, entry: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM dictionary WHERE entry = ?", params![entry])
            .map_err(|e| MolviError::Dictionary(format!("remove: {e}")))?;
        *self.cache.lock().unwrap() = None;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<DictEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT entry, replacement FROM dictionary ORDER BY entry")
            .map_err(|e| MolviError::Dictionary(format!("list: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(DictEntry {
                    entry: row.get(0)?,
                    replacement: row.get(1)?,
                })
            })
            .map_err(|e| MolviError::Dictionary(format!("list: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| MolviError::Dictionary(format!("list: {e}")))?);
        }
        Ok(out)
    }

    /// Whole-token, case-insensitive substitution (Unicode-aware). Multi-word
    /// phrases win over their individual tokens (longest-first alternation).
    /// Surrounding punctuation/spacing preserved via `\b` word boundaries.
    /// If no entries, returns text unchanged.
    pub fn apply(&self, text: &str) -> String {
        let mut guard = self.cache.lock().unwrap();
        if guard.is_none() {
            let entries = match self.list() {
                Ok(e) => e,
                Err(e) => {
                    // Metadata only: `e` is a sqlite error string, never text/entries.
                    tracing::warn!("dictionary apply: list failed ({e}); returning text unchanged");
                    return text.to_string();
                }
            };
            if entries.is_empty() {
                return text.to_string();
            }
            *guard = Some(build_cache(&entries));
        }
        let cache = guard.as_ref().unwrap();
        cache
            .re
            .replace_all(text, |caps: &regex::Captures| -> String {
                let m = caps[0].to_lowercase();
                cache
                    .lookup
                    .get(&m)
                    .cloned()
                    .unwrap_or_else(|| caps[0].to_string())
            })
            .into_owned()
    }

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

    pub fn export_csv(&self, path: &Path) -> Result<()> {
        let entries = self.list()?;
        // RFC-4180: each field is quoted if it holds a special char, and rows
        // are CRLF-separated (canonical). Handles commas/quotes/newlines.
        let mut out = String::from("entry,replacement\r\n");
        for e in &entries {
            out.push_str(&csv_util::quote_field(&e.entry));
            out.push(',');
            out.push_str(&csv_util::quote_field(&e.replacement));
            out.push_str("\r\n");
        }
        std::fs::write(path, out).map_err(|e| {
            MolviError::Dictionary(format!("export_csv write {}: {e}", path.display()))
        })?;
        Ok(())
    }

    /// Parse a JSON file into (entry, replacement) pairs WITHOUT writing.
    /// Expects a JSON array of objects with string `entry` + `replacement`.
    pub fn parse_json_vec(path: &Path) -> Result<Vec<(String, String)>> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            MolviError::Dictionary(format!("parse_json read {}: {e}", path.display()))
        })?;
        let arr: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| MolviError::Dictionary(format!("parse_json parse: {e}")))?;
        let arr = arr
            .as_array()
            .ok_or_else(|| MolviError::Dictionary("parse_json: expected JSON array".into()))?;
        let mut out = Vec::new();
        for obj in arr {
            let entry = obj["entry"]
                .as_str()
                .ok_or_else(|| MolviError::Dictionary("parse_json: missing entry string".into()))?;
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

    /// Dry-run import preview: parse the file + count new vs conflicts against
    /// the current dictionary. READ-ONLY — no DB writes, no cache invalidation.
    pub fn preview_import(&self, path: &Path) -> Result<ImportPreview> {
        // Format is already validated by the IPC layer (import_format_for_path);
        // re-derive only the json-vs-csv branch needed to pick the parser.
        let entries = if path.extension().and_then(|e| e.to_str()) == Some("json") {
            Self::parse_json_vec(path)?
        } else {
            Self::parse_csv_vec(path)?
        };
        // ponytail: bind the Vec to a local so the borrowed &str entries outlive
        // the HashSet (collecting from a temporary self.list() would dangle).
        let existing_entries = self.list()?;
        let existing: std::collections::HashSet<&str> =
            existing_entries.iter().map(|e| e.entry.as_str()).collect();
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

    pub fn export_json(&self, path: &Path) -> Result<()> {
        let entries = self.list()?;
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "entry": e.entry,
                    "replacement": e.replacement,
                })
            })
            .collect();
        let json = serde_json::to_string_pretty(&arr)
            .map_err(|e| MolviError::Dictionary(format!("export_json serialize: {e}")))?;
        std::fs::write(path, json).map_err(|e| {
            MolviError::Dictionary(format!("export_json write {}: {e}", path.display()))
        })?;
        Ok(())
    }
}

/// Build the compiled regex (longest-first alternation, case-insensitive, word-
/// bounded) and the lowercase->replacement lookup. Entries are regex-escaped so
/// metacharacters in user input are treated literally.
fn build_cache(entries: &[DictEntry]) -> Cache {
    let mut sorted: Vec<&str> = entries
        .iter()
        .map(|e| e.entry.as_str())
        .filter(|s| !s.is_empty())
        .collect();
    // ponytail: sort_by_key is stable, so equal-length entries retain DB (alphabetical) order.
    sorted.sort_by_key(|e| std::cmp::Reverse(e.len()));
    let pattern = sorted
        .iter()
        .map(|e| regex::escape(e))
        .collect::<Vec<_>>()
        .join("|");
    let re = Regex::new(&format!("(?i)\\b(?:{})\\b", pattern))
        .expect("dictionary alternation must compile (entries are escaped)");
    let lookup: HashMap<String, String> = entries
        .iter()
        .filter(|e| !e.entry.is_empty())
        .map(|e| (e.entry.to_lowercase(), e.replacement.clone()))
        .collect();
    Cache { re, lookup }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> Dictionary {
        Dictionary::open_in_memory().unwrap()
    }

    #[test]
    fn apply_replaces_single_token_case_insensitive() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("molvi", "Molvi").unwrap();
        assert_eq!(d.apply("я люблю molvi и MOLVI"), "я люблю Molvi и Molvi");
    }

    #[test]
    fn apply_handles_multiword_phrase() {
        let d = tmp();
        d.add("нью йорк", "Нью-Йорк").unwrap();
        assert_eq!(d.apply("лечу в нью йорк завтра"), "лечу в Нью-Йорк завтра");
    }

    #[test]
    fn apply_preserves_surrounding_punctuation() {
        let d = tmp();
        d.add("api", "API").unwrap();
        assert_eq!(d.apply("вызови api, потом api."), "вызови API, потом API.");
    }

    #[test]
    fn crud_roundtrip() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("molvi", "Molvi").unwrap();
        d.add("api", "API").unwrap();
        let list = d.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].entry, "api");
        assert_eq!(list[1].entry, "molvi");
        d.remove("api").unwrap();
        let list = d.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].entry, "molvi");
        d.remove("molvi").unwrap();
        assert!(d.list().unwrap().is_empty());
    }

    #[test]
    fn csv_export_then_import_roundtrips() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("molvi", "Molvi").unwrap();
        d.add("api", "API").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_dict_csv_{}_{}.csv",
            std::process::id(),
            line!()
        ));
        d.export_csv(&path).unwrap();

        let d2 = Dictionary::open_in_memory().unwrap();
        d2.import_csv(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let orig = d.list().unwrap();
        let imported = d2.list().unwrap();
        assert_eq!(orig.len(), imported.len());
        for (a, b) in orig.iter().zip(imported.iter()) {
            assert_eq!(a.entry, b.entry);
            assert_eq!(a.replacement, b.replacement);
        }
    }

    /// THE regression for RFC-4180: a replacement holding a newline, a comma,
    /// and a double-quote must survive export→import byte-for-byte. The old
    /// naive `lines()`+`splitn` parser split this into garbage rows.
    #[test]
    fn csv_export_then_import_roundtrips_multiline() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("addr", "123 Main St,\nApt \"4B\",\nSpringfield")
            .unwrap();
        d.add("plain", "Molvi").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_dict_csv_ml_{}_{}.csv",
            std::process::id(),
            line!()
        ));
        d.export_csv(&path).unwrap();

        let d2 = Dictionary::open_in_memory().unwrap();
        d2.import_csv(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let orig = d.list().unwrap();
        let imported = d2.list().unwrap();
        assert_eq!(orig.len(), imported.len());
        for (a, b) in orig.iter().zip(imported.iter()) {
            assert_eq!(a.entry, b.entry);
            assert_eq!(a.replacement, b.replacement);
        }
        assert_eq!(
            imported
                .iter()
                .find(|e| e.entry == "addr")
                .map(|e| e.replacement.as_str()),
            Some("123 Main St,\nApt \"4B\",\nSpringfield")
        );
    }

    #[test]
    fn json_export_then_import_roundtrips() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("нью йорк", "Нью-Йорк").unwrap();
        d.add("api", "API").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_dict_json_{}_{}.json",
            std::process::id(),
            line!()
        ));
        d.export_json(&path).unwrap();

        let d2 = Dictionary::open_in_memory().unwrap();
        d2.import_json(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let orig = d.list().unwrap();
        let imported = d2.list().unwrap();
        assert_eq!(orig.len(), imported.len());
        for (a, b) in orig.iter().zip(imported.iter()) {
            assert_eq!(a.entry, b.entry);
            assert_eq!(a.replacement, b.replacement);
        }
    }

    #[test]
    fn parse_csv_vec_reads_without_writing() {
        let d = Dictionary::open_in_memory().unwrap();
        d.add("existing", "E").unwrap();
        let path = std::env::temp_dir().join(format!(
            "molvi_dict_parse_{}_{}.csv",
            std::process::id(),
            line!()
        ));
        // file with 2 entries: one conflicts with "existing", one is new
        std::fs::write(
            &path,
            "entry,replacement\r\nexisting,OVERWRITE\r\nnew,NEW\r\n",
        )
        .unwrap();
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
        let path = std::env::temp_dir().join(format!(
            "molvi_dict_prev_{}_{}.csv",
            std::process::id(),
            line!()
        ));
        std::fs::write(
            &path,
            "entry,replacement\r\nexisting,OVERWRITE\r\nnew,NEW\r\n",
        )
        .unwrap();
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
}
