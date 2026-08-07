# Phase-3 Task 15 — Privacy widen + perf remeasure + final gate

**Status:** approved design (brainstorm 2026-08-06).
**Branch:** `phase3`. Per-task commits; never push/merge.
**Scope owner:** this spec. Recovery map: `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` (append a "Task 15" subsection).
**Predecessors:** Tasks 1-12 + 14 shipped + review-clean. This is the Phase-3 **closing task**.

## 1. Role

The Phase-3 plan's Task 15 (`docs/superpowers/plans/2026-08-05-molvi-phase3.md` ~line 429) is high-level: "Wire end-to-end, prove privacy + blaze, green the CI gate." This spec concretizes it into three pillars — (1) a privacy-code widen that closes the one real substrate gap surfaced by a full Phase-3 audit, plus a log-hygiene redaction; (2) an empirical perf remeasure run by the human; (3) a binary-unlocked final gate. Task 13 (brand mark) follows in a separate session; it touches no Rust/logging/perf surface, so this gate's result carries forward to it (only a quick re-gate after 13 lands).

## 2. Audit findings (the foundation for Pillar 1)

A full grep of every Phase-3 (and pre-existing) `tracing::` call site (100 in `src-tauri/src`) and every `console.*` TS call (31 in `src/`) against the 6 existing `log_privacy.rs` substrates + 2 model-gated substrates. Outcome:

### 2.1 The one real substrate gap — profile prompt in the Polished path

`finalize_substrates_log_no_transcript` (`log_privacy.rs:253`) runs the Polished arm with:

```rust
let polished_settings = PostProcessing {
    mode: PostMode::Polished,
    endpoint: Some("http://127.0.0.1:1".to_string()),
    model: Some("x".to_string()),
    ..PostProcessing::default()   // ← prompt: None (default)
};
```

The Polished arm DOES run `build_polished_body` (`postproc.rs:285`), which interpolates `settings.prompt.as_deref().unwrap_or(MOLVI_DEFAULT_PROMPT)` at `postproc.rs:289`. With `prompt: None`, the user-set/profile-loaded prompt path is NOT exercised. A future debug log that interpolates the prompt (e.g. a `tracing::debug!("polished prompt: {p}")` added during LLM tuning) would NOT trip the existing `!logs.contains(POSTPROC_SENTINEL)` assertion. The prompt is user content (§10.1: "profile prompts … not even at `trace`") — the gap is real.

### 2.2 Not gaps (metadata-only, §10.1 explicitly compliant)

| Phase-3 surface | Log content | Verdict |
|---|---|---|
| `history_query` widen (Task 12.1) | `history: query returned {} rows` | count — metadata |
| `history_bulk_delete` (Task 12.1) | `history: bulk deleted {} ids` | count — metadata |
| `history_distinct_langs` (Task 12.1) | (no log) | n/a |
| `dictionary_import_preview/_apply` (Task 12.1) | `dictionary: imported {} entries` | count — metadata; `parse_csv_vec`/`parse_json_vec` are pure (zero `tracing::`) |
| model picker IPC (Task 14) | `model {model_id}` + events `{model,bytes,total,pct}` | model_id + numbers — metadata; `FileProgress.filename` never accessed (verified in Task 14 review) |
| `profiles::foreground_exe` (Task 7) | `tracing::debug!("foreground exe: {base}")` | basename — §10.1 explicitly "may be logged" |
| `profiles::apply_profile_override` (Task 8a) | (no log — pure fn) | n/a; the PROMPT it loads flows into `build_polished_body` = §2.1 gap |
| All 31 TS `console.error` | error objects only | metadata; each carries an inline `// metadata-only` comment |

No substrate additions needed for these.

### 2.3 Log-hygiene observation — `%APPDATA%` path logs contain OS username

Not a §10.1 transcript violation (§10.1 forbids transcript-equivalent text, not OS usernames), but a privacy-first-app hygiene concern: `molvi.log` is the artifact users paste into bug reports, and `%APPDATA%` expands to `C:\Users\<name>\AppData\Roaming`. The username is PII-adjacent. Sites:

**5 `tracing::info!` sites (always-on at startup — the shared-log surface):**
- `log.rs:67` — `molvi logging initialized (dir = {})` (`%APPDATA%\com.molvi.app\logs`)
- `lib.rs:215` — `settings loaded from {}` (`%APPDATA%\com.molvi.app\settings.json`)
- `lib.rs:437` — `model dir: {}` (`%APPDATA%\com.molvi.app\models`)
- `model_store.rs:252` — `model {id} already cached at {}` (Task 14.1)
- `model_store.rs:288` — `model {id} ready at {}` (Task 14.1)

**3 `MolviError` construction sites under `%APPDATA%` (rare-error surface — the path embeds in `{e}` when the error is traced):**
- `dictionary.rs:79` — `open {}: {e}` (dictionary.db open)
- `history.rs:69` — `open {}: {e}` (molvi.db open)
- `settings.rs:314` — `write {}: {e}` (settings.json write)

**Out of scope (verified):** user-picked import/export paths (`dictionary.rs:171/211/220/291`, `snippets.rs:146/174/181/214`) are NOT under `%APPDATA%` (Desktop/Downloads) — `redact_appdata` is a no-op on them; left raw (user chose the path, diagnostic value). Test-only `eprintln!` (`engine.rs:654/684`) — not production.

## 3. Pillar 1 — Privacy-code widen

### 3.1 Substrate widen — profile prompt

**File:** `src-tauri/tests/log_privacy.rs`, test `finalize_substrates_log_no_transcript`.

**Change:** add a distinct sentinel `const POLISHED_PROMPT_SENTINEL: &str = "СЕКРЕТПРОМПТА";` (mirrors the existing `POSTPROC_SENTINEL`/`HISTORY_SENTINEL`/`DICT_SENTINEL` convention — "P3 distinct sentinels: clean per-substrate attribution"). Set `polished_settings.prompt = Some(POLISHED_PROMPT_SENTINEL.to_string())`. Add `assert!(!logs.contains(POLISHED_PROMPT_SENTINEL), "PRIVACY VIOLATION: polished prompt sentinel leaked …")` alongside the existing postproc/history/dict asserts.

The Polished arm already runs `build_polished_body` against the dead-port endpoint (`http://127.0.0.1:1`) — the body IS built (with the prompt in the system message) before the connect fails. The sentinel therefore flows the prompt through the exact leak surface. ~4 lines added; the test's existing structure (scoped capture, non-vacuous outcome asserts) is unchanged.

### 3.2 `paths::redact_appdata` helper

**File:** `src-tauri/src/paths.rs` (new `pub fn` + 1 unit test).

```rust
/// Redact the username-bearing %APPDATA% prefix from a path for privacy-safe
/// logging. `%APPDATA%` = `C:\Users\<name>\AppData\Roaming`; users share
/// molvi.log in bug reports, and <name> is PII-adjacent. Replace the prefix
/// with the literal `%APPDATA%` — mirrors how this file documents every path
/// (e.g. `%APPDATA%\com.molvi.app\`), preserves the relative structure (debug
/// value intact), and is instantly recognizable to a Windows reader. If
/// %APPDATA% is unset or the path isn't under it, fall back to the raw path
/// (user-picked import/export paths, test fixtures, etc.).
///
/// Ponytail: `std::env::var_os("APPDATA")`, NOT `dirs::home_dir()` — adds no
/// dep, matches the established `app_data_dir()` pattern (paths.rs:7-9).
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

**Apply at 8 sites** (each is a one-token swap: `x.display()` → `paths::redact_appdata(&x)`; the `tracing::`/`format!` template string is unchanged):

| Site | Current | After |
|---|---|---|
| `log.rs:67` | `log_dir.display()` | `paths::redact_appdata(&log_dir)` |
| `lib.rs:215` | `p.display()` | `paths::redact_appdata(&p)` |
| `lib.rs:437` | `model_dir.display()` | `paths::redact_appdata(&model_dir)` |
| `model_store.rs:252` | `dir.display()` | `paths::redact_appdata(&dir)` |
| `model_store.rs:288` | `dir.display()` | `paths::redact_appdata(&dir)` |
| `dictionary.rs:79` | `p.display()` (in MolviError) | `paths::redact_appdata(&p)` |
| `history.rs:69` | `p.display()` (in MolviError) | `paths::redact_appdata(&p)` |
| `settings.rs:314` | `path.display()` (in MolviError) | `paths::redact_appdata(&path)` |

**Unit test** (in `paths.rs` `#[cfg(test)] mod tests`):
```rust
#[test]
fn redact_appdata_strips_prefix() {
    let appdata = std::env::var_os("APPDATA").expect("APPDATA set under CI/user");
    let p = PathBuf::from(&appdata).join("com.molvi.app").join("models").join("x");
    assert_eq!(redact_appdata(&p), r"%APPDATA%\com.molvi.app\models\x");
    // Non-%APPDATA% path falls through unchanged (user-picked import path).
    let foreign = Path::new(r"C:\foreign\path.csv");
    assert_eq!(redact_appdata(foreign), foreign.display().to_string());
}
```
(If `APPDATA` is unset in a hostile env, the test's `expect` documents the assumption; molvi is Windows-only and `app_data_dir()` already hard-depends on `APPDATA`, so this is consistent.)

### 3.3 What Pillar 1 does NOT change

- The default RU/PTT/Smart transcription/paste path is byte-untouched (changes are in log call sites + a test).
- No new dependencies (`dirs` deliberately avoided — see `paths.rs:7-9` ponytail comment).
- No new IPC, no settings field, no frontend change.
- `log_privacy.rs` test COUNT stays at 6 (the widen is INSIDE `finalize_substrates_log_no_transcript`, not a new test fn). `--lib` count rises by 1 (the `redact_appdata` unit test): 185 → 186.

## 4. Pillar 2 — Perf remeasure (empirical, human-runs — NOT code)

### 4.1 Baselines (Phase-1/2 NFR table)

| Metric | Baseline | NFR threshold |
|---|---|---|
| Default RU/PTT/Smart RTF | 0.029 | ≤ 0.03 |
| Cold-start to tray | 1251 ms | ≤ 1251 ms |
| Peak RSS idle | 292 MB | ≤ 292 MB |
| NSIS installer | ~11 MB | (informational) |
| Nemotron streaming RTF | — | ≤ 0.09 steady-state |

### 4.2 Protocol (human runs with `cargo tauri dev` CLOSED for binary-unlock)

1. **Cold-start:** launch the release exe (or `cargo run`); time from process start to the `PTT ready` log line (or tray icon visible). 3 runs, take median.
2. **Default RTF:** dictate a controlled RU reference phrase of the SAME length used for the 0.029 baseline (re-establish that length from the Phase-1 ledger if needed). Read RTF from `molvi.log` (`feed_secs`, `audio_secs`, `rtf=`). 3 runs, median.
3. **Peak RSS:** Task Manager / Process Explorer, after 30 s idle post-startup (model loaded, no dictation).
4. **NSIS:** `cargo tauri build` → installer size in `src-tauri/target/release/bundle/nsis/`.
5. **Nemotron streaming RTF:** switch model to Nemotron, dictate the same reference phrase, read streaming RTF from `molvi.log`.

### 4.3 Documentation deliverable

Fill the Phase-3 NFR row in `AGENTS.md` (append to the existing perf table) + the "Task 15" ledger subsection. Record all 5 numbers + the date + the dev-build commit SHA.

**Regression policy:** if default RTF / cold-start / RSS exceed the NFR thresholds, that is a **separate fix task** (out of Task 15 scope — Task 15 measures + documents, it does NOT touch the hot path). The Session-8 cold-start smoke already showed GigaAM RTF 0.06–0.23 (varies with utterance length — short utterances have higher RTF from fixed overhead; `0.001 on cached/empty`). Controlled-condition measurement resolves whether a real regression exists. Blaze mandate: default RU/PTT/Smart path byte-untouched across all of Phase-3 — verified by the gate's `cargo build` + the absence of hot-path edits in the diff.

## 5. Pillar 3 — Final gate (binary-unlocked)

Close `cargo tauri dev` first (binary lock would fail the full link). Run, in order:

1. `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
2. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
3. `cargo test --manifest-path src-tauri/Cargo.toml --lib` — expect **186** (185 + `redact_appdata_strips_prefix`)
4. `cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy` — expect **6** (the substrate widen is inside an existing test; count unchanged)
5. `cargo build --manifest-path src-tauri/Cargo.toml` — full `molvi.exe` link (proves the binary unlocks + links clean)
6. `npx tsc --noEmit`
7. `npm run build`
8. _Optional_ `cargo tauri build` (release NSIS — run only if the installer-size measurement in Pillar 2 needs it; long build, human's call)

Record every gate's exit code + the `--lib` count in the ledger.

## 6. Commits (on `phase3`)

1. `test(phase3): widen privacy substrate (profile prompt) + redact %APPDATA% from path logs` — Pillar 1 code: `log_privacy.rs` + `paths.rs` + the 8 redaction sites.
2. `docs(agents): phase-3 perf NFR row + task-15 closure` — Pillar 2 numbers + Pillar 3 gate results + AGENTS.md refresh + ledger "Task 15" subsection + mark the Phase-3 plan Task 15 complete. (After the human's perf measurement.)

Split rationale: the code commit is reviewable in isolation (pure privacy widen, no behavioral change to the hot path); the docs commit records the empirical evidence. If the reviewer prefers, the two can collapse to one — implementer's call.

## 7. Privacy (HARD RULE, spec §10.1)

Pillar 1 IS the privacy enforcement: it closes the profile-prompt substrate gap (§3.1) and removes OS-username PII from shared logs (§3.2). The widening keeps `log_privacy.rs` as the load-bearing enforcement gate — every transcript-bearing substrate that could leak is now exercised under a scoped `trace`-level capture with a distinct sentinel, and the redaction makes "users share logs for debugging" safe-by-construction. No transcript text, partial, post-proc text, dict entry, history row, snippet cue/expansion, command phrase, profile prompt, or audio sample is logged — at any level. Detected lang (locale code) + foreground exe basename ARE metadata and remain logged.

## 8. Out of scope

- **Task 13 (brand mark)** — next session; pure SVG/icon UI; no Rust/logging/perf surface; quick re-gate after.
- **Perf FIX** — if Pillar 2 surfaces a regression beyond NFR thresholds, a separate task addresses it (Task 15 measures + documents only).
- **User-picked import/export path redaction** — those paths aren't under `%APPDATA%`; `redact_appdata` is a no-op; left raw (user chose the path; diagnostic value).
- **Backward compatibility** — not needed (Session-5 directive: clean breaks OK; `#[serde(default)]` regenerates settings).
- **New dependencies** — zero. `dirs` explicitly avoided (matches `paths.rs:7-9`).

## 9. Verification checklist (definition of done)

- [ ] Pillar 1a: `finalize_substrates_log_no_transcript` widened with `POLISHED_PROMPT_SENTINEL` + assert; test still passes (6/6 log_privacy green).
- [ ] Pillar 1b: `paths::redact_appdata` implemented + unit test; 8 call sites swapped; `--lib` = 186.
- [ ] Pillar 2: human ran the 5-measurement protocol; numbers + date + SHA recorded in AGENTS.md + ledger.
- [ ] Pillar 3: all 7 gate commands green (exit 0); results recorded in ledger.
- [ ] Commits 1 + 2 on `phase3`; ledger "Task 15" subsection appended; Phase-3 plan Task 15 marked complete.
