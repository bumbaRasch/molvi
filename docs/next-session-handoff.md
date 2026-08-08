# molvi — next-session handoff (briefing for the multi-platform port, Phase 3)

> Purpose: a self-contained briefing so a FRESH session (after a clean context)
> can pick up the **multi-platform port** and run Phase 3 (Linux) without
> re-discovering the state. Read **AGENTS.md first** (the project bible —
> toolchain, deps, architecture, privacy §10.1, blaze NFRs, "Multi-platform
> port status" section), then this file.

## Where the multi-platform port is now

molvi v0.1.0 = Windows 11 push-to-talk dictation (Tauri 2 + local CPU ASR:
GigaAM Russian + Nemotron multilingual). The **multi-platform port** (branch
`multiplatform-port`, PR #1 → `main`) is taking it to macOS (Apple Silicon) +
Linux. UI is 36-language.

**Execution state lives in the SDD ledger** — read it FIRST:
`.superpowers/sdd/2026-08-07-molvi-multiplatform-port/progress.md` (per-task
commits, review outcomes, fix rounds, spike results — the recovery map).

- **Phase 1 COMPLETE & CI-green:** dual license (MIT OR Apache-2.0); Step-0
  cfg-gate (5 Win32 sites + `windows`→target-dep + paths tests Windows-gated);
  CI matrix `.github/workflows/ci.yml` (windows-latest + macos-14 + ubuntu-latest
  = fmt + clippy `-D warnings` + `test --lib` + tsc + build). Spikes #1 (Linux
  ort-CPU build) + #2 (macOS aarch64 ort/CoreML build) PASS. See
  `docs/spike-results.md`.
- **Phase 2 CODE COMPLETE & CI-green (Tasks 4-9):** macOS Cargo deps (CoreML
  engines + tauri-nspanel git + objc2 with per-class features); paths
  cross-platform; NSWorkspace `foreground_exe` + `macos_frontmost_pid`;
  per-platform paste keys (⌘V = `Key::Meta` + `Key::Other(9)`) + verify-only
  focus guard + command-chord `letter_key` (kVK_ANSI); NSPanel overlay
  (tauri-nspanel, `can_become_key_window:false`); `src-tauri/Info.plist`
  (NSMicrophoneUsageDescription + LSUIElement) + `minimumSystemVersion` + Secure
  Input detection (`IsSecureEventInputEnabled`). All compile-verified on macos-14.
- **Deferred (hardware-blocked): Task 10** — macOS blaze RTF ≤0.03 measurement +
  full feature smoke. Needs a Mac + the on-device model. NOT doable from Windows
  or CI (CI only compiles). This is the ONE remaining macOS gate.

Tip of `multiplatform-port` (newest first): Task 9 (Info.plist + Secure Input) →
Task 7 (NSPanel overlay) → Task 6 (per-platform paste) → Task 8 (NSWorkspace
profiles) → Task 5 (paths) → Task 4 (macOS deps) → Task 3 (CI) → Task 2
(cfg-gate) → Task 1 (license). All gates green at last check: `cargo fmt` +
`clippy --all-targets -D warnings` + `cargo test --lib` (189) + `npx tsc
--noEmit` + `npm run build`, on all 3 OSes.

## Hard context (do not re-litigate)

- **Privacy §10.1 (HARD RULE):** never log transcript/partials/dict/history/
  snippet/command/prompt text — any level. Enforced by 6 `log_privacy` substrates.
- **Blaze NFRs:** default RU/PTT/Smart RTF ≤0.03; hot loop (capture→engine→
  finalize→paste) must stay allocation/lock/blocking-free + NO runtime platform
  branches (compile-time `#[cfg]` only). Nemotron feeds ONLY at the 8960-sample
  boundary (load-bearing — do not change).
- **Inline cfg, NO `mod platform`:** platform dispatch is compile-time
  `#[cfg(target_os=…)]` per feature module. NO trait objects, NO dyn dispatch.
  (decision D2 — ponytail: a central trait for ~6 single-call-site helpers is
  cargo-cult.)
- **ort-pin `[patch]` is load-bearing** (Cargo.toml `[patch.crates-io]`
  transcribe-rs rev `efc66111…`). A clean resolve on all 3 OSes relies on it. Do
  NOT remove.
- **macOS paste = `Key::Meta` + `Key::Other(9)`** (NOT `Key::Unicode('v')` +
  `Key::Command` — no such variant; the spec/old-handoff were wrong). macOS
  command chords use `kVK_ANSI` VKs (NOT Unicode — AZERTY bug). See AGENTS.md
  Hotkey.
- **macOS overlay = tauri-nspanel NSPanel** (`can_become_key_window:false`),
  NOT Tauri's `focusable:false` (broken on macOS, tauri#14102).
- **macOS/Linux code can only be COMPILE-verified from the Windows box** (only
  the Windows cfg arm compiles here); each macOS arm is verified by the macos-14
  CI job, each Linux arm by ubuntu. Runtime behavior (overlay focus, ⌘V paste,
  CoreML RTF, Wayland paste) = human smoke on the target OS (Task 10 for Mac;
  a Linux smoke for Phase 3).

## The 8 plan corrections (the plan text is WRONG here — these are verified)

1. tauri-nspanel is NOT on crates.io → git `branch="v2.1"`.
2. objc2-app-kit needs per-class features `NSWorkspace`+`NSRunningApplication`.
3. macOS command chords use `kVK_ANSI` VKs (NOT Unicode); macOS paste `Key::Other(9)`+`Key::Meta`.
4. `.plugin(tauri_nspanel::init())` is required (plan's Task 7 omitted it).
5. Tauri 2 has NO `bundle.macOS.infoPlist` → `src-tauri/Info.plist` file.
6. `NSURL::path()` returns `Option<Retained<NSString>>`.
7. edition 2024 → `unsafe extern "C"`.
8. Any Windows-only platform boundary breaks `cargo test --lib` on other OSes
   (the paths.rs lesson) — gate the tests or make cross-platform in the same task.

(All already applied in Phase 1/2. Listed so a fresh session doesn't re-discover them.)

## How to resume (Phase 3 — Linux)

1. Read `AGENTS.md` (now corrected) + this file + the ledger
   (`.superpowers/sdd/2026-08-07-molvi-multiplatform-port/progress.md`).
2. Read the plan: `docs/superpowers/plans/2026-08-07-molvi-multiplatform-port.md`
   (Phase 3 = Tasks 11-13). And `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md`.
3. **NEXT = Task 11** (Wayland PTT via `molvi record toggle` IPC subcommand +
   compositor-keybinding docs in `docs/linux-install.md`). The Wayland decision
   is RESOLVED (not open): compositor-keybinding → CLI subcommand → single-instance
   IPC signal (the `ashpd` GlobalShortcuts portal is BROKEN for overlay apps —
   ashpd#213; voxtype/whisrs/hyprwhspr converged on keybindings).
4. Use **superpowers:subagent-driven-development** (fresh subagent per task,
   review between tasks, fix ALL findings incl. Minor). Load it first.
5. **VERIFY every crate/API live** (the #1 rule): x11rb (EWMH `_NET_ACTIVE_WINDOW`/
   `_NET_WM_PID`), wl-clipboard, ashpd, the Tauri single-instance argv-forwarding
   API — via the `find-docs` skill (ctx7) + docs.rs + the resolved source in
   `~/.cargo/registry` + `~/.cargo/git/checkouts` before coding. The SDD scripts
   are bash; on Windows PowerShell, replicate their logic (git + gh + write files).
6. Gates per code task: `cargo fmt` + `clippy --all-targets -D warnings` + `test
   --lib` + (binary-unlocked) `test --test log_privacy` + `tsc --noEmit` + `npm
   run build`. Push → ubuntu CI verifies the Linux arms compile. Never kill a
   running `cargo tauri dev`. Do NOT remove the `[patch]` override.

## Open execution items (NOT Phase 3)

- **Task 10 (macOS blaze smoke)** — hardware-blocked (Mac + model). The ONE
  remaining macOS gate.
- **Updater pubkey/endpoint** (`tauri.conf.json`) — RELEASE BLOCKER (placeholder
  ed25519 + endpoint). Deployment work, not code. Out of scope for the port.
- **v0.1 Phase-3 Task 13** (brand mark) — separate effort (branch `phase3`).

## Brainstorm menu

After Phase 3 lands + the Mac/Linux smokes pass: v0.2 features (Profiles UI
editor, command-mode growth, LLM post-proc, multi-utterance context); or
double down on the blaze+accuracy moat. The competitor research in the plan's
Appendix is the reference.
