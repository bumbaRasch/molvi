# Phase-3 Task 15 — Privacy widen + perf remeasure + final gate
## Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase-3 by (1) widening the privacy test substrate to cover the profile-prompt leak surface + redacting the OS-username-bearing `%APPDATA%` prefix from path logs, (2) empirically remeasuring the blaze NFRs against the Phase-1/2 baselines, (3) running the binary-unlocked full gate suite green.

**Architecture:** Two small code changes (one log_privacy test widening + one `paths::redact_appdata` helper applied at 8 call sites) — both off the default RU/PTT/Smart hot path, zero new deps (stdlib `var_os` + `strip_prefix`, NOT `dirs`). Perf remeasure is empirical/human-runs (not code); final gate is verification. Spec: `docs/superpowers/specs/2026-08-06-phase3-task15-privacy-perf-gate-design.md`.

**Tech Stack:** Rust stable 1.97.1, Tauri 2.11.5, tracing-subscriber 0.3.23, Rust stdlib (`std::env::var_os`, `std::path::Path::strip_prefix`). Docs-verified: `doc.rust-lang.org/std/path/struct.Path.html#method.strip_prefix` (2026-07-14, rustc 1.97.1).

## Global Constraints

(copy verbatim from spec §8 + molvi AGENTS.md; every task inherits these)

- **Privacy HARD RULE §10.1:** NEVER log transcript/partials/post-processed text/dict entries/history rows/snippet cues-expansions/command phrases/profile prompts/audio — any level. Detected lang (locale code) + foreground exe basename ARE metadata, may be logged. Enforced by `src-tauri/tests/log_privacy.rs` — keep green, never weaken.
- **Blaze ratchet:** no regression for default RU/PTT/Smart. RTF 0.029 / cold-start 1251ms / RSS 292MB / NSIS ~11MB are the Phase-1/2 baselines; Nemotron streaming RTF ≤ 0.09. The code in this plan is OFF the hot path (log call sites + a test) — byte-for-byte unchanged.
- **NO new dependencies.** `dirs` is NOT a dep and must NOT be added — use `std::env::var_os("APPDATA")` (matches the established `paths.rs:7-9` ponytail comment).
- **Backward compatibility NOT needed** (Session-5 directive): clean breaks OK; `#[serde(default)]` regenerates settings.json.
- **Ponytail FULL:** smallest diff, stdlib/native first, no unrequested abstraction, `// ponytail:` for deliberate shortcuts, comments explain WHY never WHAT.
- **ctx7/find-docs MANDATORY before any external API.** For this plan the only API is `Path::strip_prefix` + `std::env::var_os` — BOTH verified against `doc.rust-lang.org/std` (rustc 1.97.1, 2026-07-14) + the existing in-tree uses (`log.rs:109`, `engine_adapter.rs:278`). No ctx7 call needed (stdlib, not a 3rd-party crate).
- **Gates (Rust):** `cargo fmt --manifest-path src-tauri/Cargo.toml --check` + `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test --manifest-path src-tauri/Cargo.toml --lib` + `cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy` + `cargo build --manifest-path src-tauri/Cargo.toml`. **Frontend:** `npx tsc --noEmit` + `npm run build`.
- **Binary lock:** if a live `molvi.exe` (running `cargo tauri dev`) locks the debug binary, `cargo build`/full `cargo test` fail at link — use `cargo check --all-targets` + `cargo test --lib` (do NOT kill the human's running app; coordinate the binary-unlocked gate run for Task 2).
- **Working tree hygiene:** there are TWO known-unrelated dirty files — `docs/superpowers/plans/session-3-handoff.md` (stale CRLF) + `src-tauri/tauri.conf.json` (human's dim experiment). NEVER `git add` either — stage only the files each task lists explicitly.

---

## File Structure

| File | Responsibility | This plan's change |
|---|---|---|
| `src-tauri/tests/log_privacy.rs` | Privacy enforcement (6 always-on + 2 model-gated substrate tests) | Widen `finalize_substrates_log_no_transcript` (Pillar 1a) |
| `src-tauri/src/paths.rs` | molvi's path resolution (`%APPDATA%\com.molvi.app\*`) | +`pub fn redact_appdata` + 1 unit test (Pillar 1b helper) |
| `src-tauri/src/log.rs` | tracing subscriber init + log retention | 1 site: `log_dir.display()` → `redact_appdata(&log_dir)` |
| `src-tauri/src/lib.rs` | Tauri builder + AppState + startup wiring | 2 sites: settings_path + model_dir |
| `src-tauri/src/model_store.rs` | hf-hub model download/cached-status | 2 sites: cached-at + ready-at model dir |
| `src-tauri/src/dictionary.rs` | dictionary store + CSV/JSON import/export | 1 site: db-open MolviError path |
| `src-tauri/src/history.rs` | history store (SQLite) | 1 site: db-open MolviError path |
| `src-tauri/src/settings.rs` | Settings struct + load/save | 1 site: write-settings MolviError path |
| `AGENTS.md` | agent notes incl. perf NFR table | +Phase-3 NFR row (Task 3) |
| `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` | recovery ledger | +"Task 15" subsection (Task 2 + 3) |
| `docs/superpowers/plans/2026-08-05-molvi-phase3.md` | the Phase-3 plan | mark Task 15 complete (Task 3) |

NO new files. NO frontend changes (this plan is Rust + docs only).

---

### Task 1: Privacy widen — profile-prompt substrate + `%APPDATA%` path redaction

**Role:** The Phase-3 privacy audit (`docs/superpowers/specs/2026-08-06-phase3-task15-privacy-perf-gate-design.md` §2) found one real substrate gap (the Polished-mode profile prompt is not exercised by `finalize_substrates_log_no_transcript`) and one log-hygiene surface (5 `tracing::` + 3 `MolviError` sites log the `%APPDATA%` path which expands to `C:\Users\<name>\AppData\Roaming` — the OS username is PII-adjacent in shared bug-report logs). This task closes both. Off the hot path; default RU/PTT/Smart byte-untouched.

**Files:**
- Modify: `src-tauri/tests/log_privacy.rs` (Pillar 1a — widen `finalize_substrates_log_no_transcript`)
- Modify: `src-tauri/src/paths.rs` (Pillar 1b — +`pub fn redact_appdata` + unit test)
- Modify: `src-tauri/src/log.rs:67`, `src-tauri/src/lib.rs:215,437`, `src-tauri/src/model_store.rs:252,288`, `src-tauri/src/dictionary.rs:79`, `src-tauri/src/history.rs:69`, `src-tauri/src/settings.rs:314` (8 call-site swaps)

**Interfaces:**
- Consumes: `std::env::var_os` (stdlib, `Option<OsString>`), `Path::strip_prefix` (stdlib, docs-verified — see Global Constraints). Nothing from earlier plan tasks.
- Produces: `pub fn paths::redact_appdata(path: &Path) -> String` — used by this task's 8 call sites; no later task consumes it (Task 2 + 3 are gate + docs).

#### Part 1a — Substrate widen (profile prompt)

- [ ] **Step 1: Add the distinct sentinel constant**

In `src-tauri/tests/log_privacy.rs`, find the sentinel-constants block (around line 248-250):

```rust
const POSTPROC_SENTINEL: &str = "СЕКРЕТПОСТПРОЦА";
const HISTORY_SENTINEL: &str = "СЕКРЕТИСТОРИЯ";
const DICT_SENTINEL: &str = "СЕКРЕТСЛОВАРЯ";
```

Add a 4th, distinct sentinel for the polished prompt (matches the existing convention — see the test's own comment at line 246: "Distinct sentinels (P3): a leak in one substrate trips its own assertion"):

```rust
const POSTPROC_SENTINEL: &str = "СЕКРЕТПОСТПРОЦА";
const HISTORY_SENTINEL: &str = "СЕКРЕТИСТОРИЯ";
const DICT_SENTINEL: &str = "СЕКРЕТСЛОВАРЯ";
const POLISHED_PROMPT_SENTINEL: &str = "СЕКРЕТПРОМПТА";
```

- [ ] **Step 2: Wire the sentinel into `polished_settings.prompt`**

In the same file, in `finalize_substrates_log_no_transcript` (around line 273-278), the `polished_settings` currently uses the default prompt (`..PostProcessing::default()` → `prompt: None`):

```rust
    let polished_settings = PostProcessing {
        mode: PostMode::Polished,
        endpoint: Some("http://127.0.0.1:1".to_string()),
        model: Some("x".to_string()),
        ..PostProcessing::default()
    };
```

Add the prompt field so the sentinel flows through `build_polished_body` (`postproc.rs:289`: `settings.prompt.as_deref().unwrap_or(MOLVI_DEFAULT_PROMPT)`):

```rust
    let polished_settings = PostProcessing {
        mode: PostMode::Polished,
        endpoint: Some("http://127.0.0.1:1".to_string()),
        model: Some("x".to_string()),
        prompt: Some(POLISHED_PROMPT_SENTINEL.to_string()),
        ..PostProcessing::default()
    };
```

**Why:** the Polished arm already runs `build_polished_body` (which builds the JSON body with the prompt in the `system` message) against the dead-port endpoint `http://127.0.0.1:1` (instant `ConnectionFailed`). With `prompt: None` the user-set/profile-loaded prompt path was never exercised; a future `tracing::debug!("polished prompt: {p}")` would not trip the existing assertion. Now the sentinel-bearing prompt flows through the exact leak surface.

- [ ] **Step 3: Add the privacy assertion**

In the same test, find the existing postproc/history/dict sentinel asserts (around line 355-366):

```rust
    assert!(
        !logs.contains(POSTPROC_SENTINEL),
        "PRIVACY VIOLATION: postproc (smart/polished) sentinel leaked into logs:\n{logs}"
    );
    assert!(
        !logs.contains(HISTORY_SENTINEL),
        "PRIVACY VIOLATION: history sentinel leaked into logs:\n{logs}"
    );
    assert!(
        !logs.contains(DICT_SENTINEL),
        "PRIVACY VIOLATION: dictionary sentinel leaked into logs:\n{logs}"
    );
```

Append a 4th assert for the prompt sentinel:

```rust
    assert!(
        !logs.contains(POLISHED_PROMPT_SENTINEL),
        "PRIVACY VIOLATION: polished prompt sentinel leaked into logs:\n{logs}"
    );
```

- [ ] **Step 4: Run the test to verify it passes**

This is a widen of an existing GREEN test (not a RED→GREEN TDD cycle — the production code `build_polished_body` already never logs the prompt; we're adding the regression guard for the FUTURE). Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy finalize_substrates_log_no_transcript
```

Expected: PASS. If it FAILS with the PRIVACY VIOLATION message, the prompt IS leaking somewhere in `postproc::run`'s Polished arm — that's a real bug; capture the log line, root-cause it (do NOT weaken the assertion). (Per the spec §2.2 audit, no current call site interpolates the prompt, so this should pass.)

**Note on binary lock:** if `molvi.exe` is running (`cargo tauri dev`), this test fails at LINK (it's an integration test → links the binary). Use `cargo check --tests --manifest-path src-tauri/Cargo.toml` to confirm compilation, then defer the actual run to Task 2's binary-unlocked gate. Do NOT kill the human's dev app.

#### Part 1b — `paths::redact_appdata` helper + 8 call-site swaps

- [ ] **Step 5: Write the failing unit test for `redact_appdata`**

In `src-tauri/src/paths.rs`, in the `#[cfg(test)] mod tests` block (after the existing `db_paths_are_nested` test, before the closing `}`):

```rust
    #[test]
    fn redact_appdata_strips_prefix_and_falls_back() {
        // %APPDATA% is set on every Windows user session (app_data_dir()
        // already hard-depends on it at paths.rs:11). If a hostile env lacks
        // it, the helper returns the raw path — assert both branches.
        let appdata = std::env::var_os("APPDATA").expect("APPDATA set (app_data_dir depends on it)");
        // Under-%APPDATA% path: prefix replaced with the literal %APPDATA%.
        let under = PathBuf::from(&appdata)
            .join("com.molvi.app")
            .join("models")
            .join("gigaam-v3-e2e-ctc");
        assert_eq!(
            redact_appdata(&under),
            r"%APPDATA%\com.molvi.app\models\gigaam-v3-e2e-ctc"
        );
        // Foreign path (user-picked import file): not under %APPDATA% → raw.
        let foreign = Path::new(r"C:\foreign\dict.csv");
        assert_eq!(redact_appdata(foreign), foreign.display().to_string());
    }
```

- [ ] **Step 6: Run the test to verify it fails**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib paths::tests::redact_appdata_strips_prefix_and_falls_back
```

Expected: FAIL — `redact_appdata` not found (error E0425 / "cannot find function").

- [ ] **Step 7: Implement `redact_appdata`**

In `src-tauri/src/paths.rs`, add a `use std::path::Path;` import at the top (the file currently imports only `PathBuf` — line 1: `use std::path::PathBuf;`). Change to:

```rust
use std::path::{Path, PathBuf};
```

Then add the helper AFTER `log_dir()` (before the `#[cfg(test)]` block), so the path-resolution functions stay grouped:

```rust
/// Redact the username-bearing %APPDATA% prefix from a path for privacy-safe
/// logging. `%APPDATA%` = `C:\Users\<name>\AppData\Roaming`; users share
/// molvi.log in bug reports, and <name> is PII-adjacent. Replace the prefix
/// with the literal `%APPDATA%` — mirrors how this file documents every path
/// (e.g. the `app_data_dir` doc comment: "%APPDATA%\com.molvi.app\"), preserves
/// the relative structure (debug value intact), and is instantly recognizable
/// to a Windows reader. If %APPDATA% is unset or the path isn't under it,
/// fall back to the raw path (user-picked import/export paths, test fixtures).
///
/// Ponytail: `std::env::var_os("APPDATA")`, NOT `dirs::home_dir()` — adds no
/// dep, matches the `app_data_dir()` pattern at line 11 (same ponytail call).
/// `strip_prefix` is component-based (doc-verified: "Only considers whole
/// path components to match") so `C:\Users\me\AppData\Roaming` won't partially
/// match `C:\Users\me2\...` — clean prefix-or-fall-through.
pub fn redact_appdata(path: &Path) -> String {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return path.display().to_string();
    };
    match path.strip_prefix(appdata) {
        Ok(rel) => format!("%APPDATA%\\{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}
```

**API verification (already done by plan author, recorded for the implementer):** `Path::strip_prefix<P: AsRef<Path>>(&self, base: P) -> Result<&Path, StripPrefixError>` — stable since 1.0.0 (doc-rs/std, rustc 1.97.1, 2026-07-14). `OsString: AsRef<Path>` is in the trait-impls list. `appdata` (owned `OsString`) moves into `strip_prefix` — it's the last use, no borrow needed. `MAIN_SEPARATOR` is NOT used — the literal `\\` in the format string is one backslash (Windows-style, matching `rel.display()` output).

- [ ] **Step 8: Run the unit test to verify it passes**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib paths::tests::redact_appdata_strips_prefix_and_falls_back
```

Expected: PASS.

- [ ] **Step 9: Swap the 8 call sites**

For each site: change `<var>.display()` → `crate::paths::redact_appdata(&<var>)` (or `paths::redact_appdata` if the file already `use crate::paths;`). The template string in the `tracing::`/`format!` is UNCHANGED — only the argument expression changes. Each is a one-token swap.

**Site 1 — `src-tauri/src/log.rs:67`:**
```rust
// BEFORE:
    tracing::info!("molvi logging initialized (dir = {})", log_dir.display());
// AFTER:
    tracing::info!("molvi logging initialized (dir = {})", crate::paths::redact_appdata(&log_dir));
```

**Site 2 — `src-tauri/src/lib.rs:215`:**
```rust
// BEFORE:
        Ok(p) => tracing::info!("settings loaded from {}", p.display()),
// AFTER:
        Ok(p) => tracing::info!("settings loaded from {}", crate::paths::redact_appdata(&p)),
```

**Site 3 — `src-tauri/src/lib.rs:437`:**
```rust
// BEFORE:
                    tracing::info!("model dir: {}", model_dir.display());
// AFTER:
                    tracing::info!("model dir: {}", crate::paths::redact_appdata(&model_dir));
```

**Site 4 — `src-tauri/src/model_store.rs:252`:**
```rust
// BEFORE:
        tracing::info!("model {model_id} already cached at {}", dir.display());
// AFTER:
        tracing::info!("model {model_id} already cached at {}", crate::paths::redact_appdata(&dir));
```

**Site 5 — `src-tauri/src/model_store.rs:288`:**
```rust
// BEFORE:
    tracing::info!("model {model_id} ready at {}", dir.display());
// AFTER:
    tracing::info!("model {model_id} ready at {}", crate::paths::redact_appdata(&dir));
```

**Site 6 — `src-tauri/src/dictionary.rs:79`:** (inside a `.map_err` — the `p` is the dictionary.db path)
```rust
// BEFORE:
            .map_err(|e| MolviError::Dictionary(format!("open {}: {e}", p.display())))?;
// AFTER:
            .map_err(|e| MolviError::Dictionary(format!("open {}: {e}", crate::paths::redact_appdata(&p))))?;
```

**Site 7 — `src-tauri/src/history.rs:69`:** (the `p` is the molvi.db path)
```rust
// BEFORE:
            .map_err(|e| MolviError::Db(format!("open {}: {e}", p.display())))?;
// AFTER:
            .map_err(|e| MolviError::Db(format!("open {}: {e}", crate::paths::redact_appdata(&p))))?;
```

**Site 8 — `src-tauri/src/settings.rs:314`:** (the `path` is the settings.json path)
```rust
// BEFORE:
            .map_err(|e| MolviError::Settings(format!("write {}: {e}", path.display())))?;
// AFTER:
            .map_err(|e| MolviError::Settings(format!("write {}: {e}", crate::paths::redact_appdata(&path))))?;
```

**Import-path note:** every site uses the fully-qualified `crate::paths::redact_appdata`. Do NOT add a `use crate::paths;` — these files each have exactly ONE call site (or two in `lib.rs` + `model_store.rs`), and the fully-qualified form is clearer at the call site (reader sees "paths" without scrolling to imports). This matches the existing convention: `log.rs` uses `crate::paths::log_dir()` fully-qualified at line 67. If clippy complains (`clippy::redundant_closure` or similar — it won't), adjust; but `crate::paths::redact_appdata(&x)` is the idiomatic one-call-site form.

- [ ] **Step 10: Format + lint (incremental gate)**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: both clean (exit 0). If clippy flags `too_many_lines` or similar on `redact_appdata`, the function is ~7 lines — well under any threshold; do NOT add `#[allow]`. If clippy flags the `format!` sites for some reason, prefer adjusting the call over suppressing.

- [ ] **Step 11: Run the lib tests (185 → 186)**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: **186 passed** (185 baseline + `redact_appdata_strips_prefix_and_falls_back`). If you see 185, the new test didn't compile/run — check the `#[cfg(test)] mod tests` block placement. If you see a test failure in an UNRELATED test, the 8-site swap likely broke something — re-read each site's context (the change is argument-only; the template string is unchanged).

- [ ] **Step 12: Run the log_privacy integration test (if binary-unlocked)**

If `cargo tauri dev` is NOT running (no binary lock):

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy
```

Expected: **6 passed** (the count is unchanged — Pillar 1a widens an existing test, it does NOT add a new test fn). If the dev app IS running, this fails at LINK — defer to Task 2 Step 1 (binary-unlocked gate). Use `cargo check --tests --manifest-path src-tauri/Cargo.toml` to confirm the integration test compiles.

- [ ] **Step 13: Stage + commit Task 1**

Stage ONLY the 8 files this task touched — do NOT stage the two known-dirty files (`docs/superpowers/plans/session-3-handoff.md`, `src-tauri/tauri.conf.json`):

```powershell
git add src-tauri/tests/log_privacy.rs src-tauri/src/paths.rs src-tauri/src/log.rs src-tauri/src/lib.rs src-tauri/src/model_store.rs src-tauri/src/dictionary.rs src-tauri/src/history.rs src-tauri/src/settings.rs
git commit -m "test(phase3): widen privacy substrate (profile prompt) + redact %APPDATA% from path logs"
```

Commit message style: matches the established `docs(design):` / `feat(...):` / `fix(...):` convention (see `git log --oneline -25 -- docs/superpowers/specs/`).

**Per-task review gate (superpowers:subagent-driven-development):** dispatch a task-reviewer with this plan + the spec §3 + the diff. Reviewer checks: (a) sentinel distinct + assert correct + test passes; (b) helper uses stdlib only (no `dirs`); (c) all 8 sites swapped (grep `\.display\(\)` in `src-tauri/src` — the only remaining hits should be the user-picked import/export paths in `dictionary.rs`/`snippets.rs` + the `eprintln!` in `engine.rs` test code + inside `redact_appdata` itself); (d) default hot path byte-untouched; (e) `--lib` = 186.

---

### Task 2: Final gate (binary-unlocked — full suite green)

**Role:** Close `cargo tauri dev` to unlock the binary, then run the complete gate suite green. This is the Phase-3 code gate (Task 13 brand mark is later — this gate's result carries forward; only a quick re-gate after 13).

**Files:** none modified (verification only). Output recorded in `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` (the ledger).

**Interfaces:** consumes Task 1's code. Produces the gate-results record (ledger).

- [ ] **Step 1: Coordinate the binary unlock with the human**

The human must close the running `cargo tauri dev` app (the live `molvi.exe` binary-locks the debug build). Confirm with the human before proceeding — do NOT kill the app yourself (AGENTS.md: "Do NOT kill the human's running app"). Once the human confirms the dev app is closed:

- [ ] **Step 2: Format check**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Expected: exit 0. If it reports unformatted files, Task 1's `cargo fmt` (Step 10) was missed — run `cargo fmt` (no `--check`), re-stage, amend or new commit.

- [ ] **Step 3: Clippy (all targets, deny warnings)**

```powershell
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: exit 0. If warnings appear, fix at the source (do NOT add `#[allow]` unless the plan/spec sanctions it — none do for this task).

- [ ] **Step 4: Lib tests (186)**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

Expected: `186 passed`. Record the exact count.

- [ ] **Step 5: log_privacy integration tests (6)**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy
```

Expected: `6 passed` (the 2 model-gated tests — `engine_worker_logs_no_transcript`, `nemotron_streaming_substrates_log_no_transcript` — SKIP gracefully if the ~2.6GB models aren't present; they print `skipping: model not present at ...` and exit 0. The 6 always-on tests run unconditionally. If you see `8 passed`, the models ARE present and the gated tests ran — even better.)

- [ ] **Step 6: Full molvi.exe link**

```powershell
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: exit 0, `molvi.exe` links clean (this is the binary-unlock proof — the link that fails when the dev app holds the binary lock). Record the build time.

- [ ] **Step 7: Frontend gate**

```powershell
npx tsc --noEmit
npm run build
```

Expected: both exit 0. `npm run build` reports the module count (was 70 after Task 12). This plan touches NO frontend files, so a failure here means a pre-existing issue unrelated to Task 15 — flag to the human, do NOT attempt to fix frontend in this task.

- [ ] **Step 8: Record gate results in the ledger**

Append to `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` a new "## Session-N — Task 15 EXECUTION (SDD)" section (or append to the existing Task 15 subsection if Task 1 already started one). Record: branch + HEAD SHA, each gate's exit code + the `--lib` count + the `log_privacy` count + the build time. Mark Task 1 + Task 2 status.

```powershell
# Example ledger entry (adapt to actual results):
## Session-N — Task 15 EXECUTION (SDD)

Task 1 (Privacy widen): complete (commit <sha>, review clean — <N findings>).
  - Pillar 1a: finalize_substrates_log_no_transcript widened with
    POLISHED_PROMPT_SENTINEL; prompt now flows through build_polished_body
    under the scoped capture; +1 assert. log_privacy 6/6 pass.
  - Pillar 1b: paths::redact_appdata (stdlib var_os + strip_prefix, no dirs
    dep) applied at 8 sites (5 tracing + 3 MolviError); +1 unit test. --lib
    185 -> 186. All %APPDATA% path logs now show literal %APPDATA%\com.molvi.app\...

Task 2 (Final gate): GREEN (binary-unlocked, dev app closed).
  - cargo fmt --check: exit 0
  - cargo clippy --all-targets -D warnings: exit 0 (<N>s)
  - cargo test --lib: 186 passed
  - cargo test --test log_privacy: 6 passed (model-gated tests skipped)
  - cargo build: exit 0 (<N>m<Ns>)
  - npx tsc --noEmit: exit 0
  - npm run build: exit 0 (70 modules)
```

(Do NOT commit the ledger yet — Task 3 appends the perf numbers + the docs commit. OR commit the ledger now as a checkpoint and amend in Task 3; the human's call. The two dirty files stay unstaged.)

---

### Task 3: Perf remeasure (human) + docs closure

**Role:** The empirical blaze measurement + the documentation commit that closes Task 15 + Phase-3. This is HUMAN-driven measurement (the human runs the app with controlled conditions); the agent's job is to document the protocol, then write the docs commit once the human supplies the numbers.

**Files:**
- Modify: `AGENTS.md` (+Phase-3 NFR row)
- Modify: `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` (Task 15 closure)
- Modify: `docs/superpowers/plans/2026-08-05-molvi-phase3.md` (mark Task 15 complete — a one-line checkbox or status note at the Task 15 section ~line 429)

**Interfaces:** consumes Task 2's green gate. Produces the Phase-3 closure record.

- [ ] **Step 1: Human runs the perf protocol (NOT agent code)**

With the dev app closed (binary unlocked — same state as Task 2), the human measures the 5 NFR metrics. Protocol (from spec §4.2):

| Metric | How to measure | Baseline | NFR |
|---|---|---|---|
| Default RU/PTT/Smart RTF | Dictate a controlled RU reference phrase of the SAME length used for the 0.029 baseline; read `rtf=` from `%APPDATA%\com.molvi.app\logs\molvi.log.<date>`. 3 runs, median. | 0.029 | ≤ 0.03 |
| Cold-start to tray | Time from `molvi.exe` launch to the `PTT ready` log line (or tray icon visible). 3 runs, median. | 1251 ms | ≤ 1251 ms |
| Peak RSS idle | Task Manager / Process Explorer, after 30 s idle post-startup (model loaded). | 292 MB | ≤ 292 MB |
| NSIS installer size | `cargo tauri build` → file size of `src-tauri/target/release/bundle/nsis/*.exe`. | ~11 MB | (informational) |
| Nemotron streaming RTF | Switch model to Nemotron, dictate the same reference phrase, read streaming RTF from the log. | — | ≤ 0.09 |

**Regression policy (spec §4.3):** if default RTF / cold-start / RSS exceed NFR thresholds, that is a SEPARATE fix task — Task 15 measures + documents, it does NOT touch the hot path. Flag any regression to the human; do not attempt a perf fix here.

- [ ] **Step 2: Fill the Phase-3 NFR row in `AGENTS.md`**

Find the perf table in `AGENTS.md` (search for "Phase-1 results" or "NFR" or the baseline numbers `0.029` / `1251`). Add a Phase-3 row (or a Phase-3 column) with the 5 measured numbers + the date + the dev-build commit SHA (HEAD after Task 1's commit). Mirror the existing table's format exactly.

Example (adapt to actual measured values):
```markdown
| Metric | Phase-1 baseline | Phase-3 (2026-08-06, <sha>) | NFR |
|---|---|---|---|
| Default RU/PTT/Smart RTF | 0.029 | <measured> | ≤ 0.03 |
| Cold-start to tray | 1251 ms | <measured> | ≤ 1251 ms |
| Peak RSS idle | 292 MB | <measured> | ≤ 292 MB |
| NSIS installer | ~11 MB | <measured> | (info) |
| Nemotron streaming RTF | — | <measured> | ≤ 0.09 |
```

- [ ] **Step 3: Mark the Phase-3 plan Task 15 complete**

In `docs/superpowers/plans/2026-08-05-molvi-phase3.md`, the Task 15 section starts at ~line 429 (`## Task 15: Integration — privacy widen + perf remeasure + gate`). Add a one-line status note under the heading (mirror how other completed tasks are marked, if any — or just prepend `**STATUS: COMPLETE (2026-08-06).** See docs/superpowers/specs/2026-08-06-phase3-task15-privacy-perf-gate-design.md + the Task 15 ledger subsection.`). Do NOT rewrite the plan's Task 15 body — it's the historical record; just add the status note.

- [ ] **Step 4: Append the closure to the ledger**

In `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md`, append the Task 3 results to the Task 15 subsection started in Task 2 Step 8:

```
Task 3 (Perf remeasure + docs closure): complete.
  - Perf (human-measured 2026-08-06): default RTF=<x>, cold-start=<x>ms,
    RSS=<x>MB, NSIS=<x>MB, Nemotron streaming RTF=<x>. <REGRESSION / NO
    REGRESSION vs Phase-1/2 baselines>.
  - AGENTS.md Phase-3 NFR row filled.
  - Phase-3 plan Task 15 marked complete.

**TASK 15 FULLY CLOSED.** Phase-3 remaining: Task 13 (brand mark — separate
session, pure UI, no privacy/perf surface; quick re-gate after). Then
superpowers:finishing-a-development-branch for the phase3 merge decision.
```

- [ ] **Step 5: Stage + commit Task 3 (the docs closure)**

Stage the 3 docs files (NOT the two known-dirty files):

```powershell
git add AGENTS.md docs/superpowers/plans/2026-08-05-molvi-phase3.md .superpowers/sdd/2026-08-05-molvi-phase3/progress.md
git commit -m "docs(agents): phase-3 perf NFR row + task-15 closure"
```

- [ ] **Step 6: Hand off to superpowers:finishing-a-development-branch**

Task 13 (brand mark) is the only remaining Phase-3 task, and it's a separate session (pure UI). After Task 13 lands + re-gates, invoke `superpowers:finishing-a-development-branch` to decide the phase3 branch integration (the human's earlier decision was "option 3: keep phase3 as-is" — revisit once Task 13 is done). Record the decision in the ledger.

---

## Self-Review

**1. Spec coverage:** every spec section maps to a task.

| Spec section | Task |
|---|---|
| §3.1 Substrate widen (profile prompt) | Task 1 Part 1a (Steps 1-4) |
| §3.2 `redact_appdata` helper | Task 1 Part 1b Steps 5-8 |
| §3.2 8 call-site swaps | Task 1 Part 1b Step 9 (sites 1-8 verbatim) |
| §3.3 unchanged hot path / no deps / count 186 | Task 1 Steps 10-12 (gate) |
| §4 Perf remeasure protocol | Task 3 Step 1 (the table) |
| §4.3 regression policy | Task 3 Step 1 (the callout) |
| §5 Final gate (7 commands) | Task 2 Steps 2-7 |
| §6 2 commits | Task 1 Step 13 + Task 3 Step 5 |
| §7 Privacy HARD RULE | Task 1 Part 1a IS the enforcement widen |
| §8 Out of scope (Task 13, perf fix, user-picked paths, backward compat, new deps) | Respected — none of these appear in any task |

No gaps.

**2. Placeholder scan:** searched for "TBD", "TODO", "implement later", "add appropriate", "similar to". None present. Every step has concrete code or a concrete command. The 8 call-site swaps (Step 9) each show BEFORE/AFTER verbatim. The `redact_appdata` helper (Step 7) is the full function body. The unit test (Step 5) is the full test body.

**3. Type consistency:** `redact_appdata(path: &Path) -> String` — defined in Step 7, consumed identically at all 8 sites in Step 9 (`crate::paths::redact_appdata(&<var>)` where `<var>` is `PathBuf` or `&PathBuf`; `&<var>` coerces to `&Path` via `Deref`). The test in Step 5 calls `redact_appdata(&under)` (`under: PathBuf`) and `redact_appdata(foreign)` (`foreign: &Path`) — both compile (`&PathBuf` → `&Path` via Deref; `&Path` passed as-is). `POLISHED_PROMPT_SENTINEL` (Step 1) is used at Step 2 (`Some(POLISHED_PROMPT_SENTINEL.to_string())`) + Step 3 (the assert). Sentinel names + the `redact_appdata` name are used identically across steps.

**4. Blaze verification:** the default RU/PTT/Smart path is touched ZERO times by this plan. The 8 call sites are all `tracing::`/`MolviError` log/error construction — off the inference + paste path. The substrate widen (Part 1a) is test-only. `redact_appdata` is called only at log sites (cold-start + model-download, never per-utterance). Task 3's perf measurement is the empirical proof — if the numbers regress, it's a separate fix task (spec §4.3 + Task 3 Step 1 callout).

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-06-phase3-task15-privacy-perf-gate.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Task 1 is the code task (fresh implementer + task-reviewer). Task 2 needs the human to close the dev app first (binary unlock) — coordinate, then a subagent runs the gate + records results. Task 3 needs the human's perf numbers — the subagent writes the docs commit once the human supplies them.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
