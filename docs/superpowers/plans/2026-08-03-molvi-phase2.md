# molvi — Phase-2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn molvi from a RU-only PTT MVP into the best local dictation app — history + dictionary, full settings UI, deterministic Smart + endpoint Polished post-processing, toggle mode, autostart, signed updater, audio feedback, P-core affinity — staying 100 % local/offline/private, blaze preserved, with a gated Nemotron multilingual spike.

**Architecture:** The Phase-1 4-thread core (Tauri main / coordinator / cpal / inference worker) is unchanged. Phase-2 adds narrow off-hot-path concerns: a `rusqlite` store (history + dictionary, separate DBs), a `postproc` step on the existing finalize side-thread, P-core **process** affinity at startup, a real tray menu, the updater/autostart plugins, and a vanilla-TS settings/history UI over a tiny signal store. Nemotron is a standalone measurement spike that conditionally adds a second engine behind a thin adapter trait.

**Tech Stack:** Rust (tauri 2.11.5, transcribe-rs 0.3.11 / ort 2.0.0-rc.12, rusqlite 0.40.1 bundled, ureq 3.3.0, windows 0.62.2 +Win32_System_Threading +Win32_System_SystemInformation, tauri-plugin-updater 2.10.1, -process, -autostart 2.5.1, -dialog 2.7.2, cpal 0.18.1); frontend = Vite 8 + TypeScript 7, vanilla TS + ~40-line signal store, no framework; spike = parakeet-rs 0.3.7.

**Spec:** [`docs/superpowers/specs/2026-08-03-molvi-phase2-design.md`](../specs/2026-08-03-molvi-phase2-design.md)

---

## Global Constraints

Copied from the spec — every task inherits these:

- **App identity:** `molvi`, identifier `com.molvi.app`. App-data dir: `%APPDATA%\com.molvi.app\`.
- **Target platform:** Windows 11 x64, WebView2. MSVC build tools.
- **Toolchain:** rustc **1.97.1**, edition **2024** (`rust-toolchain.toml`).
- **Privacy (HARD RULE, spec §10.1/§14):** NEVER log transcript text, partials, post-processed text, dictionary entries, history rows, or audio — not even at `trace`. Logs carry metadata only. Enforced by a widened log-privacy assertion test (Task 19).
- **No backward compatibility:** clean breaks only. `settings.json` regenerates from `#[serde(default)]` on any structural change; `history`/`dictionary` schemas may be dropped/recreated — no on-disk data guarantees until 1.0. No migration code.
- **Blaze (one-way ratchet):** no regression for the default RU/PTT/Smart user vs Phase-1 baselines (RTF 0.029, cold-start 1251 ms, RSS 292 MB, NSIS ≤ ~11 MB). The inference path (audio → worker → emit) stays allocation/lock-free; new work runs off the hot path.
- **Minimal deps, all latest & docs-verified:** every new crate is in spec §4 (verified 2026-08-03 via ctx7/docs.rs/crates.io). Any API the plan asserts is verified there or has an in-task `find-docs` gate.
- **Clean code (ponytail):** smallest working diff, no speculative abstractions, `ponytail:` comments mark deliberate shortcuts. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` and `cargo fmt --manifest-path src-tauri/Cargo.toml` clean at every commit. Comments explain *why*, never *what*.
- **Visual system:** teal `#0E7C86` accent (CTAs/selection/focus only), semantic colors status-only, system-ui font, 8px grid, Windows lighter-on-hover + stroke-focus, vanilla TS. Inline SVG icons (CSP `default-src 'self'`, no emoji).
- **Naming/copy:** the app is `molvi` (lowercase) everywhere user-facing. Privacy promise copy is verbatim in §14.

---

## Pre-flight: verified dependencies (spec §4 — do NOT re-derive)

| Crate | Version | Cargo line |
|---|---|---|
| rusqlite | 0.40.1 | `rusqlite = { version = "0.40.1", features = ["bundled"] }` |
| tauri-plugin-autostart | 2.5.1 | `tauri-plugin-autostart = "2.5.1"` |
| tauri-plugin-updater | 2.10.1 | `tauri-plugin-updater = "2.10.1"` |
| tauri-plugin-process | 2.x | `tauri-plugin-process = "2"` |
| tauri-plugin-dialog | 2.7.2 | `tauri-plugin-dialog = "2.7.2"` |
| ureq | 3.3.0 | `ureq = { version = "3.3", features = ["json"] }` |
| windows | 0.62.2 | add features `Win32_System_Threading`, `Win32_System_SystemInformation` |
| parakeet-rs (SPIKE ONLY) | 0.3.7 | spike crate, never a `molvi` dep until Task 18 GO |

In-task `find-docs`/ctx7 gates remain for: cpal output-stream playback (Task 9), the exact `windows` features for `GetLogicalProcessorInformationEx` (Task 5), Nemotron ONNX export + NOML license (Task 17).

---

## File Structure (target — Phase-2 deltas)

```
src-tauri/src/
  lib.rs             — widen: menu, plugins (autostart/updater/process/dialog), IPC, p-core affinity call
  paths.rs           — + history_db_path(), dictionary_db_path()
  errors.rs          — + Db, Dictionary, PostProc, Updater variants
  settings.rs        — WIDEN (new fields, §spec 6.1); rename push_to_talk→recognition_mode
  audio.rs           — + output stream for start/stop tones
  ort_affinity.rs    — NEW: p_core_mask() + apply_process_affinity()
  engine.rs          — unchanged (GigaAM); thread-count comes for free from process affinity
  coordinator.rs     — Toggle branch on settings.recognition_mode
  pipeline.rs        — widen finalize side-thread: post-proc → paste → history insert (+ generation-guard fix)
  overlay.rs         — + bottom-center position, polishing phase, tone trigger hooks
  hotkey.rs          — + AltGr Ctrl+Alt mirror registration
  history.rs         — NEW: molvi.db (Arc<Mutex<Connection>>)
  dictionary.rs      — NEW: dictionary.db (bare Connection, IPC-thread)
  postproc.rs        — NEW: Smart pipeline + Polished (ureq)
  tray.rs            — NEW: Menu (Menu/MenuItem/CheckMenuItem/PredefinedMenuItem)
  updater.rs         — NEW: check + apply + error surfacing
  ipc.rs             — NEW: all #[tauri::command] handlers (settings/dict/history/dialog/update)
molvi-nemotron-spike/ — NEW (Task 17): measurement reference binary
src/                  — settings UI (Tasks 14–16)
  settings/main.ts    — signal store + sidebar nav + mount
  settings/store.ts   — tiny signal store
  settings/ui.ts      — Toggle/Select/Input/SecretInput/Slider/SettingsGroup/SettingRow/Button/Alert/Tooltip
  settings/icons.ts   — inline SVG set
  settings/sections/*.ts — one file per sidebar section
  settings.css        — :root tokens + base styles
  settings.html       — entry
```

Dependency order: **1 (foundation) → 2,3 (store) → 4 (postproc) → 5 (affinity) → 6,7 (interaction) → 8 (pipeline wire) → 9,10 (audio/overlay) → 11 (tray) → 12 (updater/autostart) → 13 (IPC) → 14,15,16 (UI) → 17 (spike) → [18 if GO] → 19 (integration/gate).**

---

## Task 1: Foundation — settings widen + paths + errors + Cargo deps

**Role:** Add the Cargo deps and the settings/paths/errors scaffolding every later task consumes.

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/settings.rs`, `src-tauri/src/paths.rs`, `src-tauri/src/errors.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `settings::Settings` with the new fields (spec §6.1); `paths::history_db_path()` / `paths::dictionary_db_path()`; `errors::MolviError::{Db,Dictionary,PostProc,Updater}`.
- Consumes: nothing new.

- [ ] **Step 1: Add deps to `src-tauri/Cargo.toml`**

In `[dependencies]` add (versions from Pre-flight):
```toml
rusqlite = { version = "0.40.1", features = ["bundled"] }
tauri-plugin-autostart = "2.5.1"
tauri-plugin-updater = "2.10.1"
tauri-plugin-process = "2"
tauri-plugin-dialog = "2.7.2"
ureq = { version = "3.3", features = ["json"] }
```
Widen the existing `windows` line:
```toml
windows = { version = "0.62", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation", "Win32_System_Threading", "Win32_System_SystemInformation"] }
```
Run `cargo fetch --manifest-path src-tauri/Cargo.toml`; if any version unresolvable, `find-docs` the exact latest and pin it.

- [ ] **Step 2: `paths.rs` — add DB path helpers**

```rust
pub fn history_db_path() -> Result<std::path::PathBuf> {
    Ok(app_data_dir()?.join("molvi.db"))
}
pub fn dictionary_db_path() -> Result<std::path::PathBuf> {
    Ok(app_data_dir()?.join("dictionary.db"))
}
```
Add inline tests asserting both end with the right filename under `app_data_dir()`.

- [ ] **Step 3: `errors.rs` — widen the enum**

Add variants: `Db(String)`, `Dictionary(String)`, `PostProc(String)`, `Updater(String)` (each `#[error("..: {0}")]`). Keep the existing `Result<T>` alias.

- [ ] **Step 4: `settings.rs` — add the new fields (TDD)**

Add the failing test first (append to `mod tests`):
```rust
#[test]
fn phase2_fields_default() {
    let s = Settings::default();
    assert_eq!(s.recognition_mode, RecognitionMode::PushToTalk);
    assert_eq!(s.post_processing.mode, PostMode::Smart);
    assert!(!s.history.enabled);
    assert_eq!(s.history.max_entries, 100);
    assert_eq!(s.history.max_age_days, 7);
    assert!(!s.autostart);
    assert!(!s.overlay.sounds.enabled);
}
```
Run `cargo test --manifest-path src-tauri/Cargo.toml settings::tests::phase2_fields_default` → FAIL (types missing).

Now add the types (replace Phase-1 `push_to_talk: bool` cleanly — no migration):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionMode { #[default] PushToTalk, Toggle }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostMode { Raw, #[default] Smart, Polished }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SmartToggles {
    pub apply_dictionary: bool,
    pub fix_case: bool,
    pub normalize_whitespace: bool,
    pub cleanup_repeated_marks: bool,
    pub merge_chunks: bool,
    pub remove_duplicate_words: bool,
    pub normalize_numbers_dates: bool,
    pub remove_fillers: bool,
    pub inter_chunk_punctuation: bool,
}
impl Default for SmartToggles { fn default() -> Self { Self { apply_dictionary:true, fix_case:true, normalize_whitespace:true, cleanup_repeated_marks:true, merge_chunks:true, remove_duplicate_words:true, normalize_numbers_dates:true, remove_fillers:false, inter_chunk_punctuation:true } } }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PostProcessing {
    pub mode: PostMode,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub smart: SmartToggles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistorySettings { pub enabled: bool, pub max_entries: u32, pub max_age_days: u32 }
impl Default for HistorySettings { fn default() -> Self { Self { enabled:false, max_entries:100, max_age_days:7 } } }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundsSettings { pub enabled: bool, pub start: String, pub stop: String }

// OverlaySettings: add `sounds: SoundsSettings` (default off). AudioSettings unchanged.
// Settings: replace `push_to_talk: bool` with `recognition_mode: RecognitionMode`;
//           add `post_processing: PostProcessing`, `history: HistorySettings`,
//           `autostart: bool`, `updater: UpdaterSettings { check_on_startup: bool, channel: String }`.
//           `hotkey_altgr_mirror: bool` (default false) — flat field, hotkey is still a String.
```
Wire `Default for Settings` to the new defaults (`recognition_mode: PushToTalk`, `post_processing: PostProcessing::default()`, `history: HistorySettings::default()`, `autostart:false`, `updater:{check_on_startup:true,channel:"stable".into()}`, `hotkey_altgr_mirror:false`). The existing `Settings::default()` test must still pass after updating the `push_to_talk` assertion → rename it to `recognition_mode`.

Run the test → PASS. Run `cargo test --manifest-path src-tauri/Cargo.toml` → all green.

- [ ] **Step 5: Wire modules in `lib.rs`**

Add `mod history; mod dictionary; mod postproc; mod ort_affinity; mod tray; mod updater; mod ipc;` now (each file is created empty / with a doc-comment `// ponytail: filled in Task N` until its task; gate dead-code with `#[allow(dead_code)]` at the module level until wired, matching Phase-1's engine pattern). Add `mod` lines only — do not call them yet.

- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Fix any warnings (e.g. unused imports from new deps are expected only until their tasks land — if clippy errors on the empty modules, add `#![allow(dead_code)]` at the top of each empty module file).
```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/settings.rs src-tauri/src/paths.rs src-tauri/src/errors.rs src-tauri/src/lib.rs src-tauri/src/{history,dictionary,postproc,ort_affinity,tray,updater,ipc}.rs
git commit -m "feat(phase2): foundation — settings widen, db paths, new error variants, deps"
```

---

## Task 2: `dictionary.rs` — dictionary.db CRUD + import/export + apply-transform

**Role:** The user-authored custom dictionary store + the deterministic "apply" transform used by Smart post-proc.

**Files:**
- Create/fill: `src-tauri/src/dictionary.rs`
- Test: inline `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `Dictionary { conn: Connection }`, `Dictionary::open() -> Result<Self>` (opens `dictionary_db_path`, creates schema), `crud(entry,replacement)`, `remove(entry)`, `list() -> Vec<DictEntry>`, `import_csv(path)` / `export_csv(path)` / `import_json(path)` / `export_json(path)`, and `apply(&self, text: &str) -> String` (whole-token, case-insensitive, multi-word phrase match, preserves surrounding spacing/punctuation).
- Consumes: `paths::dictionary_db_path`, `errors::{MolviError::Dictionary, Result}`.

- [ ] **Step 1: Failing tests (apply transform + CRUD)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn tmp() -> Dictionary { Dictionary::open_in_memory().unwrap() }

    #[test]
    fn apply_replaces_single_token_case_insensitive() {
        let mut d = Dictionary::open_in_memory().unwrap();
        d.add("molvi", "Molvi").unwrap();
        assert_eq!(d.apply("я люблю molvi и MOLVI"), "я люблю Molvi и Molvi");
    }
    #[test]
    fn apply_handles_multiword_phrase() {
        let mut d = tmp();
        d.add("нью йорк", "Нью-Йорк").unwrap();
        assert_eq!(d.apply("лечу в нью йорк завтра"), "лечу в Нью-Йорк завтра");
    }
    #[test]
    fn apply_preserves_surrounding_punctuation() {
        let mut d = tmp();
        d.add("api", "API").unwrap();
        assert_eq!(d.apply("вызови api, потом api."), "вызови API, потом API.");
    }
    #[test]
    fn crud_roundtrip() { /* add, list, remove, assert list empty */ }
    #[test]
    fn csv_export_then_import_roundtrips() { /* export to temp, clear, import, assert equal */ }
}
```
Run → FAIL (methods undefined).

- [ ] **Step 2: Implement `dictionary.rs`**

```rust
use rusqlite::{params, Connection};
use crate::errors::{MolviError, Result};
use crate::paths;

pub struct DictEntry { pub entry: String, pub replacement: String }

pub struct Dictionary { conn: Connection }

impl Dictionary {
    fn schema(conn: &Connection) -> Result<()> {
        conn.execute("CREATE TABLE IF NOT EXISTS dictionary (entry TEXT PRIMARY KEY, replacement TEXT NOT NULL, created_at INTEGER NOT NULL)", [])
            .map_err(|e| MolviError::Dictionary(format!("schema: {e}")))?;
        Ok(())
    }
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| MolviError::Dictionary(format!("open: {e}")))?;
        Self::schema(&conn)?;
        Ok(Self { conn })
    }
    pub fn open() -> Result<Self> {
        let p = paths::dictionary_db_path()?;
        let conn = Connection::open(&p).map_err(|e| MolviError::Dictionary(format!("open {}: {e}", p.display())))?;
        Self::schema(&conn)?;
        Ok(Self { conn })
    }
    pub fn add(&self, entry: &str, replacement: &str) -> Result<()> { /* INSERT OR REPLACE */ }
    pub fn remove(&self, entry: &str) -> Result<()> { /* DELETE */ }
    pub fn list(&self) -> Result<Vec<DictEntry>> { /* SELECT ORDER BY entry */ }
    // import/export: read/write CSV (entry,replacement) and JSON via std I/O (no extra dep).
}
```

`apply` (the load-bearing transform): load all entries once into a `Vec<(String,String)>` (cache on the struct behind a `OnceCell` refreshed on add/remove), build a regex alternation of the entries (longest first so multiword phrases win before their tokens), replace case-insensitively with the replacement. Use the `regex` crate (transitive via transcribe-rs already; if not present, add `regex = "1"` — verify it's already in the lockfile first; if not, add it: one dep, earned). **Privacy:** `apply` is in-memory; never `tracing::` the text.

- [ ] **Step 3: Tests pass + fmt + clippy + commit**

```bash
cargo test --manifest-path src-tauri/Cargo.toml dictionary
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add src-tauri/src/dictionary.rs src-tauri/Cargo.toml
git commit -m "feat(dictionary): dictionary.db CRUD + import/export + deterministic apply transform"
```

---

## Task 3: `history.rs` — molvi.db insert/prune/query/delete/clear/disable&erase

**Role:** Opt-in transcript history behind `Arc<Mutex<Connection>>` (written by the finalize side-thread, read by the IPC thread).

**Files:**
- Fill: `src-tauri/src/history.rs`
- Test: inline

**Interfaces:**
- Produces: `History(Arc<Mutex<Connection>>)`; `History::open_if_enabled(settings: &HistorySettings) -> Option<Result<History>>` (None when disabled — table not created); `insert(&self, text, lang, engine, post_mode) -> Result<()>` (inserts + prunes); `query(&self, search: Option<&str>, limit: u32, offset: u32) -> Result<Vec<HistoryRow>>`; `delete(&self, id) -> Result<()>`; `clear(&self) -> Result<()>` (DELETE all, keep table); `drop_table(&self) -> Result<()>` (Disable & Erase).
- Consumes: `paths::history_db_path`, `settings::HistorySettings`.

- [ ] **Step 1: Failing tests** — insert+prune (insert 105 rows, assert only newest 100 remain; insert old row, assert age-prune removes it), query with search term, delete, clear, drop_table. Use `History::open_in_memory()`.
- [ ] **Step 2: Implement** — schema from spec §6.2. `insert` runs in one transaction: INSERT then `DELETE WHERE id NOT IN (SELECT id ORDER BY created_at DESC LIMIT ?max_entries)` and `DELETE WHERE created_at < (now - max_age_days)`. `query` builds `WHERE text LIKE ?search` only when search is Some.
- [ ] **Step 3: Tests pass + fmt + clippy + commit** — `git commit -m "feat(history): opt-in transcript history with retention prune + disable&erase"`.

---

## Task 4: `postproc.rs` — Smart deterministic pipeline + Polished (ureq)

**Role:** The Raw/Smart/Polished transform between finalize and paste (spec §8).

**Files:**
- Fill: `src-tauri/src/postproc.rs`
- Test: inline (per-step + determinism + idempotence)

**Interfaces:**
- Produces: `pub enum PostOutcome { Used(String), Failed(String /*err*/, String /*raw fallback*/) }`; `pub fn run(text: &str, settings: &PostProcessing, dict: Option<&Dictionary>) -> PostOutcome`.
- Consumes: `settings::PostProcessing`/`SmartToggles`, `dictionary::Dictionary` (for `apply`), `ureq`.

- [ ] **Step 1: Failing tests — one per Smart step + pipeline invariants**

```rust
#[test] fn cleanup_repeated_marks() { assert_eq!(smart_step_repeated_marks("что??? да..."), "что? да…"); }
#[test] fn remove_duplicate_words() { assert_eq!(smart_step_dup_words("я я пошёл пошёл домой"), "я пошёл домой"); }
#[test] fn fix_case_sentence_start() { assert_eq!(smart_step_case("привет. как дела"), "Привет. Как дела"); }
#[test] fn normalize_whitespace() { assert_eq!(smart_step_ws("а   б ,в"), "а б, в"); }
#[test] fn pipeline_is_deterministic() { let s = Settings::default().post_processing; let a = run("тест. тест", &s, None); let b = run("тест. тест", &s, None); assert_eq!(a, b); }
#[test] fn pipeline_is_idempotent() { /* run twice, assert second == first (modulo ws) */ }
```
Run → FAIL.

- [ ] **Step 2: Implement the Smart steps as pure fns** — `smart_step_merge`, `_inter_chunk_punct`, `_repeated_marks`, `_dup_words`, `_case`, `_fillers` (off by default; small RU filler list `["ээ","э-э","мм","ну","типа","короче"]`), `_numbers_dates` (best-effort digit/date normalization — conservative, no guessing), `_ws`. `run` composes them in spec §8.2 order, gated by `settings.smart.*` toggles; applies dictionary between steps 4 and 5 when `apply_dictionary` and `dict.is_some()`. **Privacy:** no `tracing::` of `text`.

- [ ] **Step 3: Implement Polished (ureq)** — `fn polished(text, settings) -> Result<String, String>`:

```rust
let agent: ureq::Agent = ureq::Agent::config_builder()
    .timeout_global(Some(std::time::Duration::from_secs(20))).build().into();
let endpoint = settings.endpoint.as_deref().ok_or("no endpoint")?.trim_end_matches('/');
let body = serde_json::json!({
    "model": settings.model,
    "messages": [
        {"role":"system","content": settings.prompt.as_deref().unwrap_or(MOLVI_DEFAULT_PROMPT)},
        {"role":"user","content": text}
    ], "temperature": 0.0
});
let resp = agent.post(&format!("{endpoint}/chat/completions")).send_json(&body);
match resp {
    Ok(r) => { let v: serde_json::Value = r.into_body().read_json().map_err(|e| format!("parse: {e}"))?;
               v["choices"][0]["message"]["content"].as_str().map(|s| s.to_string()).ok_or("no content") }
    Err(ureq::Error::StatusCode(c)) => Err(format!("endpoint {c}")),
    Err(ureq::Error::Timeout(_)) => Err("timeout".into()),
    Err(e) => Err(format!("{e}")),
}
```
`MOLVI_DEFAULT_PROMPT` fixes punctuation/case/grammar, preserves meaning + language + dictionary terms, never rephrases style.

`run`: match `settings.mode` → Raw returns text as-is; Smart runs the pipeline (wrap non-fatal step errors as no-ops + log metadata); Polished calls `polished()` and on `Err` returns `PostOutcome::Failed(err, raw_text)` (caller pastes raw + surfaces error — never lose the transcript).

- [ ] **Step 4: Tests pass + fmt + clippy + commit** — `git commit -m "feat(postproc): Smart deterministic pipeline + Polished OpenAI-compatible endpoint (ureq)"`.

---

## Task 5: `ort_affinity.rs` — P-core process affinity (measure-first)

**Role:** Apply the robust blaze lever — restrict the whole process to P-cores so ort's intra-op pool lands there. No fork. (spec §11, corrected.)

**Files:**
- Fill: `src-tauri/src/ort_affinity.rs`
- Test: inline smoke

**Interfaces:**
- Produces: `pub fn p_core_mask() -> Option<usize>` (None = homogeneous CPU or enumeration failed → caller skips); `pub fn apply_process_affinity() -> Option<usize>` (sets + returns the previous mask for logging).

- [ ] **Step 1: find-docs gate — verify the exact `windows` features + the `GetLogicalProcessorInformationEx` shape for `EfficiencyClass`**

Run `npx ctx7@latest library windows "GetLogicalProcessorInformationEx EfficiencyClass processor relationship"` and/or read `microsoft.github.io/windows-docs-rs`. Confirm: `Win32_System_SystemInformation` provides `GetLogicalProcessorInformationEx`, `LOGICAL_PROCESSOR_RELATIONSHIP(0=RelationProcessorCore)`, `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX` with `ProcessorRelationship(EfficiencyClass: u8; 0=Performance/P-core, 1=Efficiency/E-core; Group: [..] with mask)`. If the API shape differs, adapt the code below.

- [ ] **Step 2: Failing smoke test**

```rust
#[test]
fn p_core_mask_is_some_or_none_gracefully() {
    // On the dev i5-12450H (4P+4E) this is Some(non-zero); on homogeneous CI it may be None.
    // Either is acceptable; we only assert we never panic and never return 0.
    match p_core_mask() { Some(m) => assert_ne!(m, 0), None => {} }
}
```

- [ ] **Step 3: Implement** — enumerate `GetLogicalProcessorInformationEx(RelationProcessorCore)` over the variable-size buffer (the standard grow-and-retry loop), sum the group masks where `EfficiencyClass == 0`. Return `None` if no E-cores present (homogeneous → affinity pointless) or on any Win32 error (fail open — never break startup over a perf optimization).

```rust
use windows::Win32::Foundation::FALSE;
use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessAffinityMask};
use windows::Win32::System::SystemInformation::{GetLogicalProcessorInformationEx, LOGICAL_PROCESSOR_RELATIONSHIP};

pub fn p_core_mask() -> Option<usize> {
    // 1. enumerate RelationProcessorCore (0); collect (efficiency_class, mask).
    // 2. has_e_cores = any efficiency_class == 1; if none → return None (homogeneous).
    // 3. mask = OR of masks where efficiency_class == 0; if mask == 0 → None.
    //    ponytail: per-group masks collapsed to one usize (Windows <64 logical CPUs/group;
    //    molvi targets single-group consumer CPUs).
    todo!("see Step 1 verification for the exact field accessors")
}

pub fn apply_process_affinity() -> Option<usize> {
    let mask = p_core_mask()?;
    // SAFETY: SetProcessAffinityMask on the current process; mask computed from real topology.
    let prev = unsafe { SetProcessAffinityMask(GetCurrentProcess(), mask) };
    if prev != 0 { tracing::info!("process affinity set to P-cores (mask=0x{mask:X})"); Some(prev) }
    else { tracing::warn!("SetProcessAffinityMask failed; inference runs on all cores"); None }
}
```

- [ ] **Step 4: Wire into `lib.rs` startup** — call `ort_affinity::apply_process_affinity();` at the very top of `run()` (before Tauri builder), best-effort (ignore the Option). This must run BEFORE the engine spawns so ort's pool is born into the restricted set.

- [ ] **Step 5: Smoke test passes + fmt + clippy + commit** — `git commit -m "perf(ort): P-core process affinity (no fork); measure-first"`. Add a follow-up benchmark note in the commit body: "RTF delta to be measured in Task 19."

---

## Task 6: `coordinator.rs` — Toggle mode

**Role:** Branch the state machine on `settings.recognition_mode`.

**Files:**
- Modify: `src-tauri/src/coordinator.rs`

**Interfaces:**
- Produces: no signature change — the coordinator reads `settings.recognition_mode` via the Pipeline trait (add `fn recognition_mode(&self) -> RecognitionMode` to `Pipeline`, or pass it in `Command::Input`). Cleanest: `Command::Input { is_pressed, mode: RecognitionMode }` (the hotkey task builds it from settings).
- Consumes: `settings::RecognitionMode`.

- [ ] **Step 1: Failing tests** — add `Command::Input { mode }`; tests: PTT press+release → begin/finalize (existing); Toggle press → begin, Toggle press again → finalize (release is a no-op in Toggle). Update existing tests to pass `mode: RecognitionMode::PushToTalk`.
- [ ] **Step 2: Update `handle`** — in `Command::Input`:
  - PTT: `is_pressed:true` in Idle → begin; `is_pressed:false` in Recording → finalize (unchanged).
  - Toggle: `is_pressed:true` in Idle → begin; `is_pressed:true` in Recording → finalize; `is_pressed:false` ignored.
- [ ] **Step 3: Tests pass + fmt + clippy + commit** — `git commit -m "feat(coordinator): Toggle mode (PTT default; tap start/stop)"`.

---

## Task 7: `hotkey.rs` — AltGr Ctrl+Alt mirror

**Role:** Register the `Ctrl+Alt+` mirror of the binding when `settings.hotkey_altgr_mirror` is on, so RU/EU layouts (AltGr = synthesized Ctrl+Alt) fire the hotkey (AGENTS.md hotkey note).

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1:** Parse `settings.hotkey` (e.g. `"Alt+`"`); when `hotkey_altgr_mirror` is on, also register `"Ctrl+Alt+`"` (same handler, same coordinator channel). ~3 lines in the registration path + a test that asserts both bindings get registered when the flag is on (mock the register fn).
- [ ] **Step 2:** fmt + clippy + commit — `git commit -m "feat(hotkey): AltGr Ctrl+Alt mirror for RU/EU layouts (opt-in)"`.

---

## Task 8: `pipeline.rs` — widen finalize side-thread + generation-guard fix

**Role:** Wire post-proc + history into the finalize side-thread (spec §13); fix the deferred Phase-1 edge case (cancel-during-Processing paste).

**Files:**
- Modify: `src-tauri/src/pipeline.rs`

**Interfaces:**
- Consumes: `postproc::run`, `history::History` (held in managed state, `Option<Arc<Mutex<...>>>`), `settings::Settings`.
- Produces: the widened finalize flow (no new public types).

- [ ] **Step 1: Thread post-proc + history into `finalize_session`'s side thread**

In the existing `std::thread::spawn(move || { let text = rx.recv()...; ... paste ... })`:
```rust
// after the generation-guard check, before paste:
let outcome = postproc::run(&text, &settings.post_processing, dict.as_ref());
let final_text = match &outcome {
    PostOutcome::Used(t) => t.clone(),
    PostOutcome::Failed(err, raw) => { tracing::warn!("post-proc failed: {err} (paste raw)"); overlay::emit_toast(&app, "post-processing failed — pasted raw"); raw.clone() }
};
// paste final_text (not `text`)
// after successful paste:
if let Some(h) = &history {
    let _ = h.lock().unwrap().insert(&final_text, &lang, &engine, post_mode_label(&settings.post_processing.mode));
}
```
`dict` and `history` are cloned from `AppState` into the side-thread closure (`Arc` clones). Pull `lang`/`engine` from settings (`"ru"`/`"gigaam"`; Nemotron wiring in Task 18 extends `engine`).

- [ ] **Step 2: Generation-guard edge fix (deferred Phase-1 polish)** — the existing generation counter already covers cancel-during-Processing; audit and assert: if `generation.load() != captured_gen` after `rx.recv()`, skip paste AND skip history insert (current code skips paste only — add the history skip). Add a coordinator test that Cancel during Processing produces no paste and no history row (the privacy-implicating case).

- [ ] **Step 3:** fmt + clippy + commit — `git commit -m "feat(pipeline): post-proc + history on finalize side-thread; cancel-during-Processing history skip"`.

---

## Task 9: `audio.rs` — start/stop tones (cpal output) + overlay hooks

**Role:** Opt-in audio feedback.

**Files:**
- Modify: `src-tauri/src/audio.rs`, `src-tauri/src/overlay.rs` (trigger hooks), `src/settings.html`/bundle wavs.

- [ ] **Step 1: find-docs gate** — `npx ctx7@latest library cpal "build output stream play samples StreamTrait"`. Confirm cpal 0.18 output-stream build + `play()`/`pause()`. Verify tone-playback latency is acceptable (a short 512-sample sine burst is fine).
- [ ] **Step 2:** Add two tiny wavs `src-tauri/sounds/start.wav` and `stop.wav` (~1-2 KB each; generate with a one-off script or commit royalty-free blips). Bundle via `tauri.conf.json` `bundle.resources`. Add `fn play_tone(kind: Tone)` that opens a short-lived cpal output stream, writes the decoded samples, drains, closes. Gate by `settings.overlay.sounds.enabled`.
- [ ] **Step 3:** Trigger from `pipeline.rs`: `play_tone(Start)` in `begin_session`, `play_tone(Stop)` at finalize start. Off the hot path (tone playback is fire-and-forget on its own micro-thread).
- [ ] **Step 4:** fmt + clippy + commit — `git commit -m "feat(audio): opt-in start/stop tones (cpal output, bundled wavs)"`.

---

## Task 10: `overlay.rs` — bottom-center positioning + polishing phase + scrim

**Role:** Phase-1 deferred overlay polish.

**Files:**
- Modify: `src-tauri/src/overlay.rs`, `src/overlay.ts`, `src/overlay.css`, `src-tauri/tauri.conf.json` (overlay window position).

- [ ] **Step 1:** Move overlay window to bottom-center (compute x from monitor work-area width, y above taskbar) on show — `tauri.conf.json` keeps the window; `overlay::show` repositions via `SetPosition` using the primary monitor work-area. Add `emit_phase` variant `kind: "polishing"` (Task 8 emits it).
- [ ] **Step 2:** Caption scrim refinement in `overlay.css`: `text-shadow: 0 1px 3px rgba(0,0,0,.6)` + a subtle `background: rgba(15,23,42,.55)` rounded card, so system-ui is legible over any background. (Bundle Atkinson Hyperlegible ONLY if a real-screenshot contrast test fails — ponytail: skip unless measured.)
- [ ] **Step 3:** fmt + clippy + manual smoke (open over a white bg + a busy bg, read caption) + commit — `git commit -m "feat(overlay): bottom-center position + polishing phase + caption scrim"`.

---

## Task 11: `tray.rs` — real tray menu

**Role:** Status / Toggle / Settings / History / Quit.

**Files:**
- Fill: `src-tauri/src/tray.rs`, modify `src-tauri/src/lib.rs` (replace the inline `TrayIconBuilder`).

**Interfaces (verified spec §4):** `Menu::with_items(app, &[...])`, `MenuItem::with_id`, `CheckMenuItem::with_id`, `PredefinedMenuItem::with_id(app, id, PredefinedMenuItemKind::Separator)`, `TrayIconBuilder::with_id("main").menu(&menu).on_menu_event(|app,e| match e.id().as_ref() {...}).on_tray_icon_event(...)`.

- [ ] **Step 1:** Build the menu (Status item disabled, separator, Toggle check, Settings, History, separator, Quit). Attach to tray. `on_menu_event`: Settings/History → show the settings window + (History) emit a "navigate to history section" event the frontend listens for; Toggle → flip `settings.recognition_mode` + update the check + re-register hotkey; Quit → `app.exit(0)`.
- [ ] **Step 2:** Status text flips "molvi (warming up)" → "molvi" once PTT ready (existing behavior, now driven from `tray::set_status`). Add a `set_recording(active: bool)` that flips the Status dot/text while live.
- [ ] **Step 3:** fmt + clippy + commit — `git commit -m "feat(tray): real menu — status/toggle/settings/history/quit"`.

---

## Task 12: `updater.rs` + autostart + process plugins

**Role:** Wire the plugins (spec §12).

**Files:**
- Fill: `src-tauri/src/updater.rs`, modify `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`.

- [ ] **Step 1: Config** — `tauri.conf.json`: `"bundle": { ..., "createUpdaterArtifacts": true }`, `"plugins": { "updater": { "pubkey": "<PASTE PUBLIC KEY>", "endpoints": ["https://github.com/<owner>/molvi/releases/latest/download/latest.json"] } }`. Generate the keypair OUT of repo: `cargo tauri signer generate -w ~/.tauri/molvi.key -p "<pw>"`; paste pubkey; record in `AGENTS.md` that the private key lives in CI secrets `TAURI_SIGNING_PRIVATE_KEY`/`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (do NOT commit the key).
- [ ] **Step 2: Plugins in `lib.rs`** — `.plugin(tauri_plugin_updater::Builder::new().build())`, `.plugin(tauri_plugin_process::init())`, `.plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--autostarted"])))`, `.plugin(tauri_plugin_dialog::init())`.
- [ ] **Step 3: `updater.rs`** — `pub async fn check(app: &AppHandle) -> Result<String>` returns a status string ("up to date" / "version X available"); `pub async fn apply(app: &AppHandle) -> Result<()>` calls `app.updater()?.check().await?` → `download_and_install(..).await?` → `app.restart()`. The IPC task (Task 13) surfaces results to the Updates section. Gated by `settings.updater.check_on_startup` on app start (async, non-blocking).
- [ ] **Step 4: Autostart sync** — when the user toggles `settings.autostart` (IPC Task 13), call `app.autolaunch().enable()/disable()` (trait `ManagerExt`). Sync on startup: if `settings.autostart` != `app.autolaunch().is_enabled()`, reconcile to settings.
- [ ] **Step 5:** fmt + clippy + commit — `git commit -m "feat(updater): signed auto-updater + autostart + process plugins"`.

---

## Task 13: `ipc.rs` — Tauri commands (settings/dict/history/dialog/update)

**Role:** The frontend ↔ Rust bridge.

**Files:**
- Fill: `src-tauri/src/ipc.rs`, modify `src-tauri/src/lib.rs` (`invoke_handler`).

**Interfaces (verified):** each `#[tauri::command] async fn ...`. `tauri-plugin-dialog::DialogExt::dialog().file().blocking_pick_file()/blocking_save_file()` (off main thread — these are `async` commands).

- [ ] **Step 1:** Commands:
  - `get_settings(state) -> Settings` / `set_settings(state, settings)` (writes file + live-applies: hotkey re-register, autostart sync, post-proc/history reload).
  - `dictionary_list / dictionary_add(entry,replacement) / dictionary_remove(entry)`.
  - `dictionary_import / dictionary_export` (use `app.dialog().file()...blocking_pick_file()/save_file()`, then call `dictionary::import_*/export_*`).
  - `history_query(search, limit, offset) -> Vec<HistoryRow>` / `history_delete(id)` / `history_clear()` / `history_disable_and_erase()` (also flips `settings.history.enabled=false`).
  - `check_update() -> String` / `apply_update()`.
  - `re_paste(id)` — read a history row, push its text through the paste path (reuse `paste::paste_text`).
- [ ] **Step 2:** Register all in `tauri::generate_handler![...]`. **Privacy:** no command logs a transcript/dict/history `text` payload — only ids/counts/durations.
- [ ] **Step 3:** fmt + clippy + commit — `git commit -m "feat(ipc): settings/dictionary/history/dialog/update commands"`.

---

## Task 14: Settings UI shell — sidebar + signal store + component kit + CSS tokens

**Role:** The app chrome + the visual system (spec §7).

**Files:**
- Create: `src/settings.html`, `src/settings/main.ts`, `src/settings/store.ts`, `src/settings/ui.ts`, `src/settings/icons.ts`, `src/settings.css`

**Interfaces:** IPC commands from Task 13 (`invoke("get_settings")`, etc.).

- [ ] **Step 1: `src/settings.html`** — `<div id="app"><nav id="sidebar"></nav><main id="content"></main></div>` + `<script type="module" src="./settings/main.ts"></script>`.
- [ ] **Step 2: `src/settings.css`** — paste the spec §7.2 `:root` token block verbatim; add base styles (`body{font-family:system-ui,"Segoe UI",sans-serif;background:var(--canvas);color:var(--text);margin:0}`, sidebar list, focus rings = `outline:2px solid var(--accent);outline-offset:1px`, controls radius `var(--radius-control)`, `.settings-group` card style).
- [ ] **Step 3: `store.ts`** — ~40-line signal store:
```ts
type Listener = () => void;
export class Store<T extends object> {
  private ls = new Set<Listener>();
  constructor(private state: T) {}
  get(): Readonly<T> { return this.state; }
  set(patch: Partial<T>) { Object.assign(this.state, patch); this.ls.forEach(l => l()); }
  sub(l: Listener): () => void { this.ls.add(l); return () => this.ls.delete(l); }
}
```
- [ ] **Step 4: `ui.ts`** — the ~10 component helpers (`Toggle`, `Select`, `TextInput`, `SecretInput`, `Slider`, `SettingsGroup`, `SettingRow(label, control, help?)`, `Button`, `Alert`, `Tooltip`). Each returns an `HTMLElement` + (where relevant) a getter/setter. Inline-SVG icons in `icons.ts`.
- [ ] **Step 5: `main.ts`** — fetch settings via `invoke("get_settings")`, build the sidebar (9 sections in spec §7.1 order with icons), route clicks to section modules (Task 15), mount the default section (Recognition).
- [ ] **Step 6:** `cargo tauri dev` opens the window; visually confirm the sidebar renders + tokens apply. fmt (tsc) + commit — `git commit -m "feat(ui): settings shell — sidebar + signal store + component kit + visual tokens"`.

---

## Task 15: Settings UI sections — Recognition/Microphone/Text/Dictionary/Hotkey/Overlay/Updates/About

**Role:** The 8 non-History sections (History is Task 16).

**Files:**
- Create: `src/settings/sections/{recognition,microphone,text,dictionary,hotkey,overlay,updates,about}.ts`

- [ ] **Step 1: Recognition** — `Select` Engine (`GigaAM (Russian)`; add `Nemotron Multilingual` ONLY if Task 18 landed), `Select` Language, collapsed **Advanced** (`Slider`s for VAD min/max chunk, padding, energy threshold from `settings.vad`).
- [ ] **Step 2: Microphone** — `Select` input device (populated via a `list_audio_devices` command — add it to Task 13 if missing), **Refresh devices** button, a live level meter (subscribe to a `mic-level` event while this pane is open).
- [ ] **Step 3: Text** — `Toggle` paste mode (clipboard/type), radio `PostMode` (Raw/Smart/AI Rewrite). **Conditional reveal**: AI Rewrite → Endpoint `TextInput` + API key `SecretInput` + Model `TextInput` + Prompt `Textarea`; Smart → the 9 `SmartToggles`. Each change → `invoke("set_settings", {settings})` (debounced 300ms).
- [ ] **Step 4: Dictionary** — `SettingsGroup` listing entries (entry → replacement) with edit/delete, add-row form, **Import**/Export buttons (`invoke("dictionary_import")` / `invoke("dictionary_export")` — the Rust side opens the dialog).
- [ ] **Step 5: Hotkey** — hotkey-capture `TextInput` (listen for the next keystroke, serialize to `"Alt+`"`), `Toggle` mode (PTT/Toggle), `Toggle` AltGr mirror.
- [ ] **Step 6: Overlay** — `Toggle` enabled, `Select` position, `Toggle`s waveform/timer, **Sounds** (`Toggle` + start/stop pickers — grouped here per spec).
- [ ] **Step 7: Updates** — version label, **Check now** button (`invoke("check_update")`), channel label. **About** — version, licenses, links, **Privacy Promise** as its own item (spec §14 verbatim copy).
- [ ] **Step 8:** Manual sweep of every section; `tsc --noEmit` + commit — `git commit -m "feat(ui): settings sections — recognition/microphone/text/dictionary/hotkey/overlay/updates/about"`.

---

## Task 16: History UI — consent-first screen + viewer + actions

**Role:** The History section, laid out consent-first (spec §7.1).

**Files:**
- Create: `src/settings/sections/history.ts`

- [ ] **Step 1:** Layout order: (1) opt-in `Toggle` + privacy-promise copy at top; (2) retention (`TextInput` number entries / days); (3) search box; (4) list (paginated `invoke("history_query")`, each row: timestamp + truncated text + re-paste + delete); (5) bottom: **Clear all** + **Disable & erase** (confirm dialog). When opt-in is off, only block (1) shows; the rest is hidden.
- [ ] **Step 2:** Re-paste calls `invoke("re_paste", {id})`; Clear → `history_clear`; Disable & Erase → confirm → `history_disable_and_erase` → flip the Toggle off + clear the list view.
- [ ] **Step 3:** Manual smoke; `tsc` + commit — `git commit -m "feat(ui): history — consent-first screen + viewer + clear/disable&erase"`.

---

## Task 17: Nemotron viability spike (standalone, parallelizable)

**Role:** Answer "can Nemotron run real-time on the i5-12450H?" and auto-emit a report. NOT a molvi dep.

**Files:**
- Create: `molvi-nemotron-spike/Cargo.toml`, `molvi-nemotron-spike/src/main.rs`, `molvi-nemotron-spike/clips/` (EN/ES/DE/FR/RU short wavs + reference transcripts), `molvi-nemotron-spike/report/` (generated).

**Interfaces (verified spec §4):** `parakeet_rs::Nemotron::from_pretrained(dir, None)`, `set_target_lang("ru-RU"|"en-US"|…|"auto")` ("auto"=101), offline `transcribe_audio(&[f32]) -> String`.

- [ ] **Step 1: find-docs gates** — confirm `parakeet-rs 0.3.7` API (`npx ctx7@latest docs /altunenes/parakeet-rs "Nemotron from_pretrained set_target_lang transcribe_audio"`); pick the ONNX export (`pantinor/...-onnx` parakeet-rs layout vs `onnx-community/...-int4` — prefer int4 for size); confirm the **NVIDIA Open Model License** terms; record in `molvi-nemotron-spike/LICENSE.nemotron`.
- [ ] **Step 2:** Spike crate: load Nemotron, warm, then for each clip × language measure cold-load ms, warm-load ms, per-utterance RTF (wall/audio-dur), peak RSS (windows `GetProcessMemoryInfo`), CPU% (sampling), WER vs reference (after normalize). `set_target_lang("auto")` for the multilingual clips.
- [ ] **Step 3:** Auto-emit `report/<timestamp>.{md,json}` with every metric + model id + lang + clip length. Print a one-line verdict: GO (<0.5) / Conditional (0.5–1.0) / NO-GO (≥1.0) per spec §10.3.
- [ ] **Step 4:** Run on the dev machine; commit the report + the binary. **Decision checkpoint:** based on the report, either proceed to Task 18 (GO/Conditional) or close Nemotron for Phase-2 (NO-GO → skip Task 18, document in the report + spec).
- [ ] **Step 5:** commit — `git commit -m "feat(spike): Nemotron viability spike — measurement report + GO/NO-GO"`.

---

## Task 18 (CONDITIONAL on Task 17 GO/Conditional): Nemotron wiring — adapter trait + engine picker

**Role:** Add Nemotron as a second engine behind a thin local adapter; manual picker; NO auto-routing; NO streaming.

**Files:**
- Create: `src-tauri/src/engine_adapter.rs`, modify `src-tauri/src/engine.rs`, `pipeline.rs`, `settings.rs` (`model` accepts `"nemotron-..."`), `src/settings/sections/recognition.ts`.

- [ ] **Step 1: Adapter trait** (only now — two engines justify it):
```rust
pub trait Engine: Send {
    fn transcribe_session(&mut self, samples: &[f32], on_partial: &(dyn Fn(&str)+Send+Sync)) -> Result<String>;
}
```
GigaAM keeps the VadChunked path; Nemotron implements it by buffering samples and calling `Nemotron::transcribe_audio` on finalize (offline whole-buffer — no partials in Phase-2; `on_partial` unused for Nemotron). The worker selects the engine from `settings.model`.
- [ ] **Step 2:** Add `parakeet-rs = "0.3.7"` to molvi's Cargo (only now — it was spike-only before). Verify the ort version unify (`parakeet` rc.13 vs transcribe-rs rc.12) compiles; if it breaks, pin ort per AGENTS.md.
- [ ] **Step 3:** Recognition UI adds `Nemotron Multilingual` to the Engine select; if Task 17 was Conditional, show the warning badge *"Multilingual mode is slower than Russian recognition."* + default the engine to GigaAM.
- [ ] **Step 4:** Manual multilingual smoke (dictate EN + RU); fmt + clippy + commit — `git commit -m "feat(engine): Nemotron multilingual (non-streaming, manual pick) behind adapter trait"`.

---

## Task 19: Integration — privacy test widen + perf remeasure + gate

**Role:** Wire end-to-end, prove privacy + blaze, green the CI gate.

**Files:**
- Modify: `src-tauri/tests/log_privacy.rs` (widen), `src-tauri/src/lib.rs` (final wiring), `AGENTS.md` (NFR table update).

- [ ] **Step 1: Privacy test widen** — run a transcript through finalize + Smart + a Polished mock + a history insert; capture the log buffer; assert no transcript/post-proc/dict/history substring appears (regex-scan for a known sentinel string from the fixture). Assert `ort=warn` filter still drops transcribe-rs's `log::info!("  -> \"{}\"", text)`.
- [ ] **Step 2: Perf remeasure** — re-measure RTF (GigaAM, with P-core affinity on/off for the delta), cold-start to tray, peak RSS, NSIS size. Fill the Phase-2 NFR row in `AGENTS.md`. Assert no regression vs Phase-1 baselines (RTF ≤ 0.029, cold ≤ ~1300ms, RSS ≤ ~310MB, NSIS ≤ ~11MB). If P-core affinity gave a measured win, record it; if the sequential/spin PR (spec §11) is worth it, open it now — otherwise leave deferred.
- [ ] **Step 3: Full gate** — `cargo fmt`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml` (15 std + Phase-2 unit tests green; model-gated behind `engine-model-test`), `npm run build` (tsc + vite), `cargo tauri build` produces the signed NSIS.
- [ ] **Step 4:** commit — `git commit -m "test(phase2): widen privacy assertion + remeasure NFRs + green gate"`.

---

## Self-Review (run before handoff)

**Spec coverage** — every Phase-2 goal (spec §2) maps to a task: history(3,8,13,16), dictionary(2,4,13,15), settings UI(14,15,16), toggle(6,11), autostart(12), updater(12), audio feedback(9), tray(11), AI post-proc(4), ort threading(5,19), Nemotron spike(17,+18), deferred polish(8 generation-guard, 9 progress-via-bundle, 10 overlay position). Quality Bar (§2.1) enforced via Global Constraints + clippy/fmt gate (Task 19). Privacy §14 → Task 19 widened test. Visual system → Tasks 14/15. ✓

**Placeholder scan** — Task 5 Step 3 contains a `todo!()` for the `GetLogicalProcessorInformationEx` field accessors, deliberately gated behind its Step 1 find-docs verification (the exact `EfficiencyClass`/`Group` accessors vary across windows-rs point releases — verifying before writing is the AGENTS.md rule, not a placeholder). All other steps have real code. ✓

**Type consistency** — `RecognitionMode`/`PostMode`/`SmartToggles`/`PostProcessing`/`HistorySettings`/`SoundsSettings` defined in Task 1, consumed identically in Tasks 4/6/8/15. `Dictionary::apply`, `History::{insert,query,delete,clear,drop_table}`, `postproc::run → PostOutcome`, `Engine` adapter trait — names match across producers/consumers. `Command::Input { mode }` (Task 6) matches the hotkey builder. ✓

**Scope** — single cohesive Phase-2 release; one plan is appropriate (UI/Rust/Nemotron are coupled by the IPC + engine-picker seams and ship together). ✓

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-03-molvi-phase2.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks (spec/quality + ctx7 spot-checks at each gate), fast iteration. Required sub-skill: `superpowers:subagent-driven-development`.

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints for review.

Which approach?
