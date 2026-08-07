# Session 3 — handoff prompt

> Copy-paste the block below as your FIRST message in session 3 (after `/clear`).
> AGENTS.md is auto-loaded by opencode, so it's already the canonical context —
> this prompt just adds the session state + workflow + your task.

---

```
Resume work on molvi.

## State
- Branch `main` @ `d2dad9b` — clean tree, working dir clean. Last 3 commits
  on main (in reverse):
    d2dad9b docs(phase3): design spec + 15-task implementation plan + UX
           research + session-3 handoff
    50b5f34 fix: recognition lang coercion, Nemotron latency warning removed,
           About credits as clickable links
    855fb19 refactor: ponytail-audit sweep — dead config flags, no-op stub,
           single-impl abstractions (-347 lines net)
- Local branches `phase1` and `phase2` are FULLY ABSORBED into main (safe to
  delete as cleanup; only origin/phase1 exists on the remote).
- Phase-3 design + plan SAVED AND READY (committed at d2dad9b):
  - Spec: docs/superpowers/specs/2026-08-05-molvi-phase3-design.md
  - Plan: docs/superpowers/plans/2026-08-05-molvi-phase3.md (15 tasks)
  - UX research: docs/phase-3-ux-research.md
- Phase-3 goal: close 3 "feels broken" gaps (Nemotron live caption via
  parakeet-rs transcribe_chunk, toggle-mode auto-stop, replace-selected-text)
  + ship 5 local-differentiators (command-mode grammar, backtrack parsing,
  per-app profiles, snippets, model picker) + UX layer (overlay redesign,
  onboarding, federated search, history/dict upgrades, brand mark). 100%
  local, blaze, no telemetry, no backward compat.

## Recently shipped (post-audit baseline)
- Audit removed: LoggingSettings + level, OverlaySettings.position,
  UpdaterSettings.channel, SmartToggles.normalize_numbers_dates (no-op stub),
  Feedback enum, dead SettingRow/Tooltip, BufMaker/ScopedBufMaker unification,
  Cache.lookup Vec→HashMap, tests/common mod, ~13 trivial WHAT comments.
- Fixed: recognition language Select renders empty for stale values → coerces
  to "auto" + persists; removed misleading "Multilingual mode is slower"
  warning; About section credits are clickable links via tauri-plugin-opener
  2.5.4 (Nemotron first: HF model, parakeet-rs, GigaAM, transcribe-rs, ONNX
  Runtime, Tauri). AGENTS.md regenerated.
- i18n chunk: 62 → 58.98 KB gzip across audit + fixes.

## Gates (run before claiming any task done)
- Rust: `cargo fmt --manifest-path src-tauri/Cargo.toml` + `cargo clippy
  --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo
  test --manifest-path src-tauri/Cargo.toml --lib` (88 tests).
- TS: `npx tsc --noEmit` + `npm run build`.
- Binary lock: if a `molvi.exe` (running `cargo tauri dev`) locks the debug
  binary, use `cargo test --lib` + `cargo check --all-targets` (compiles test
  code without linking molvi.exe) — do NOT kill my running app.

## HARD rules
- Privacy §10.1: NEVER log transcript / partials / audio / command text /
  snippet cues-expansions / profile prompts (not even at `trace`). Detected
  lang + foreground exe basename ARE metadata and may be logged.
- Backward compat is NOT needed — clean changes over compatibility shims.
- ctx7 / find-docs is MANDATORY for any external API before coding (parakeet-rs
  `/altunenes/parakeet-rs`, transcribe-rs `/cjpais/transcribe-rs`, Tauri
  `/websites/v2_tauri_app`, enigo — the live ctx7 ids).
- ponytail FULL: smallest diff, stdlib/native first, no unrequested
  abstraction, `// ponytail:` comments for deliberate shortcuts. Comments
  explain WHY, never WHAT.
- Multi-step feature work → superpowers:subagent-driven-development (fresh
  implementer per task + review). Do NOT commit / push / merge unless I
  explicitly ask.

## Known remaining
- One pre-release gate left on main: human GUI smoke (`cargo tauri dev`) —
  PTT cycle on GigaAM (unchanged path) and Nemotron, language switch, RTL
  (ar/he), toasts, Nemotron lang selector. Verify the audit removals (overlay
  Select gone for position, channel line gone from Updates, smart toggle
  count = 8) + the new fixes (credits are clickable links in About, recognition
  lang Select never empty, no Nemotron latency warning) didn't break any
  visible surface.
- Phase-3 Task 1 (Nemotron streaming) is the single highest-ROI change and the
  natural starting point. ctx7 verify the parakeet-rs streaming API shape
  before writing any code (Task 1 Step 1).

## What I want now
<YOUR TASK HERE>
```

---

## Notes for filling in `<YOUR TASK HERE>`

- For **execute the Phase-3 plan**: "Execute Phase-3 starting from Task 1 (Nemotron streaming). Use superpowers:subagent-driven-development — fresh implementer per task + my review between tasks. Don't commit/push unless I say so."
- For **just Task 1** (the wow-feature): "Execute Phase-3 Task 1 only (Nemotron cache-aware streaming). ctx7-verify the API first."
- For **a different scope-cut**: name it — e.g. "Do the UX layer (Tasks 9-13) first, defer the engine work to session 4."
- For **human GUI smoke + finish on main**: "Walk me through the smoke checklist for the post-audit main branch before I start Phase-3."
- For **branch cleanup**: "Delete local phase1 + phase2 branches (fully absorbed into main). Optionally push main to origin."
