# Session-clear resume prompt — Task 12 execution

Paste the block below after a full session clear to resume Task 12 execution.

---

Resume molvi Phase-3 (fresh session — full context cleared).
FIRST: read `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` — it is the recovery ledger (fully updated through Task 11 final review + human smoke PASS + Task 12 design+plan done). Then `git status` + `git log --oneline -6` on branch `phase3`.
STATE — branch `phase3`, HEAD `1e6b9c8`, clean tree (modulo stale `docs/superpowers/plans/session-3-handoff.md` CRLF churn + human's `src-tauri/tauri.conf.json` dimension experiment — ignore both; never stage them).
- Tasks 1-11 + 14 SHIPPED + review-clean + human-smoke-verified.
- Settings-UI audit punch-list 5/5 done.
- AGENTS.md documents Tasks 1-11 + 14 + Task 12 plan-ready; ledger has full detail for all.
TASK 12 (History + Dictionary upgrades) is READY TO EXECUTE — brainstorm → spec → plan all DONE + committed. Unlike the high-level Phase-3 plan sketch, there is now a COMPLETE verbatim-code implementation plan.
- **Spec:** `docs/superpowers/specs/2026-08-06-history-dict-upgrades-design.md` (commit `66f608f` + align `a37c92d`).
- **Plan:** `docs/superpowers/plans/2026-08-06-history-dict-upgrades.md` (commit `1c1b238`, self-reviewed). 6 tasks, verbatim code, TDD for Rust.
EXECUTE the plan via superpowers:subagent-driven-development (fresh implementer per task + task-reviewer per task; auto-fix loop). **Pause after Task 12.3 (history UX = riskiest) and after Task 12.5 (dict complete) for my review.** Record progress in the EXISTING ledger (`.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` — append a "Task 12.X" subsection per task). Do NOT create a separate workspace; keep ONE recovery map.
THE 6 TASKS (read the plan file for verbatim code):
- 12.1 — Rust IPC (TDD): widen `history_query` (+lang/since, dynamic WHERE) + `history_bulk_delete(Vec<i64>)` + `history_distinct_langs() -> Vec<String>` + split `dictionary_import` → `dictionary_import_preview` (read-only pick+parse+count) + `dictionary_import_apply(path)` + `dictionary.rs` extracts `parse_csv_vec`/`parse_json_vec`/`apply_vec` + `ImportPreview` type + `lib.rs` registration. Existing `import_csv`/`import_json` become thin wrappers (tests stay green).
- 12.2 — Toaster action primitive (`ui.ts` `toast()` opts +`action?: {label, onClick}` + CSS `.toast-action`).
- 12.3 — History: row DOM restructure (roving tabindex — NOT listbox; `.hist-main` is a `<div>` not a `<button>`) + click/Enter expand + lang/date filter chips + IPC wiring.
- 12.4 — History: keyboard nav (↑/↓/j/k/Home/End/Enter/Delete/Space) + bulk select (checkbox + shift-range) + bulk delete (existing `twoStepConfirm` → `history_bulk_delete`).
- 12.5 — Dictionary: live filter (client-side) + undo-delete (5s, remove→toast→re-add; uses 12.2) + import preview (2-IPC; uses 12.1). `types.ts` +`ImportPreview` mirror (IPC row, NOT a Settings field → R4 invariant untouched).
- 12.6 — i18n ×36 (17 new keys, `dictionary.*` namespace NOT `dict.*`) + final whole-branch review.
KEY DECISIONS (locked in brainstorm, do NOT re-litigate):
- **All filter/bulk/preview IPC is server-side** (correct UX; ~25 lines Rust off the hot path).
- **History single-row undo-delete is OUT** (architecturally unclean for ephemeral auto-pruned data; dict undo IS in — clean inverse, keyed by `entry`).
- **Keyboard nav = roving-tabindex composite, NOT listbox** (APG listbox excludes interactive-element options; history rows have checkbox + buttons). Verified against w3.org/WAI/ARIA/apg 2026-08-06.
- **17 i18n keys, `dictionary.*` namespace.** No `select_all` button (YAGNI — shift-range covers it; the loaded-vs-DB ambiguity needs a tooltip = net cognitive cost). `common.import` reused for the preview-confirm button.
HARD RULES (unchanged, non-negotiable):
- PRODUCT IDENTITY: molvi = the FASTEST, most ACCURATE, most PERFORMANT. Default RU/PTT/Smart dictation path byte-for-byte untouched. Task 12 is frontend-heavy (history/dict are settings sections) but 12.1 adds Rust IPC — must not regress the default path.
- Privacy §10.1 (NEVER log transcript/dict/history content — any level; `console.error` logs the error object only, metadata-only; UI DISPLAYS the user's own data — display ≠ logging).
- NO new dependencies (Cargo.toml + package.json untouched — stdlib Rust, vanilla TS).
- Backward compat NOT needed (clean breaks; `#[serde(default)]` regenerates).
- ctx7/find-docs MANDATORY before any external API (WAI-ARIA, Tauri 2 IPC, rusqlite). Never trust memory for API signatures.
- ponytail FULL (smallest diff, stdlib/native first, `// ponytail:` for shortcuts).
- Фикси все баги. Делай всё правильно. Делай максимально производительно и код максимально быстрым, блейс. Также обязательно смотри все документации — делай только по документациям, никогда не полагайся только на свою память.
GATES per task:
- Rust (12.1): `cargo fmt --manifest-path src-tauri/Cargo.toml` + `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test --manifest-path src-tauri/Cargo.toml --lib` + `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` (binary-lock safe if `molvi.exe` is running — do NOT kill the human's app; if clippy --all-targets fails to link, use `--lib`).
- Frontend (12.2-12.6): `npx tsc --noEmit` + `npm run build`. Human GUI smoke is the behavioral gate (no JS test runner).
- Per-task commits on `phase3`; NEVER push/merge.
BEGIN: read the ledger + the plan file (`docs/superpowers/plans/2026-08-06-history-dict-upgrades.md`), confirm git state, then invoke superpowers:subagent-driven-development and dispatch the Task 12.1 implementer. И сделай максимально правильно, обращайся к документациям.
