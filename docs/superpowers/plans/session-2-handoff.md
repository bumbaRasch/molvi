# Session 2 — handoff prompt

> Copy-paste the block below as your FIRST message in session 2 (after `/clear`).
> AGENTS.md is auto-loaded by opencode, so it's already the canonical context —
> this prompt just adds the session state + workflow + your task.

---

```
Resume work on molvi.

## State
- Branch `phase2` @ `c6a9e9d` — merge-ready. Last milestone landed: full 36-language UI i18n (Settings + overlay + tray, RTL ar/he), a toast notification system (replaced the always-persistent alerts), a Nemotron recognition-language selector (auto + 40 locales, detected language now recorded in history), locales split into per-lang files. AGENTS.md is canonical and current — trust it.
- The SDD ledger / reports live under `.superpowers/sdd/2026-08-04-ui-i18n/` (gitignored scratch — if absent, `git log --oneline` on phase2 is the durable record).

## Gates (run before claiming any task done)
- Rust: `cargo fmt --manifest-path src-tauri/Cargo.toml` + `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test --manifest-path src-tauri/Cargo.toml --lib` (91 tests).
- TS: `npx tsc --noEmit` + `npm run build`.
- Binary lock: if a `molvi.exe` (running `cargo tauri dev`) locks the debug binary, use `cargo test --lib` + `--tests` — do NOT kill my running app.

## HARD rules
- Privacy §10.1: NEVER log transcript / partials / audio (not even at `trace`). Detected lang code IS metadata.
- Backward compat is NOT needed — clean changes over compatibility shims.
- ctx7 / find-docs is MANDATORY for any external API before coding (parakeet-rs `/altunenes/parakeet-rs`, transcribe-rs `/cjpais/transcribe-rs`, Tauri `/websites/v2_tauri_app` — the `/tauri-apps/api` id is stale).
- ponytail FULL: smallest diff, stdlib/native first, no unrequested abstraction, `// ponytail:` comments for deliberate shortcuts.
- Multi-step feature work → superpowers:subagent-driven-development (fresh implementer per task + review). Do NOT commit / push / merge unless I explicitly ask.

## Known remaining
- One pre-release gate left: human GUI smoke (`cargo tauri dev`) — PTT cycle on GigaAM and Nemotron, language switch, RTL (ar/he), toasts, Nemotron lang selector.
- All 11 deferred-minor findings from the final review are RESOLVED (swept in commits `954d4ad`/`4772c24`/`be3c129`/`c6a9e9d`); the only intentional leave is the `text.post_${o}` dynamic-key pattern in `text.ts` (correct data-driven code — "fixing" would break it).

## What I want now
<YOUR TASK HERE>
```

---

## Notes for filling in `<YOUR TASK HERE>`

- For the **GUI smoke + finish**: "Run the GUI smoke checklist (yourself or guide me), then walk me through merge options for phase2 → main (load superpowers:finishing-a-development-branch)."
- For **new feature work**: state it in one paragraph; the session will brainstorm → plan → SDD-execute.
- For a **parked-minor fix**: e.g. "Fix the toast cap-evict exit animation" — name the item from the ledger.
