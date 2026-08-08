You are continuing molvi (push-to-talk dictation: Tauri 2 webview shell + local CPU
ASR via ort — GigaAM Russian via `transcribe-rs`, Nemotron multilingual via
`parakeet-rs`). The **multi-platform port** (branch `multiplatform-port`, PR #1)
is in progress: **Phase 1 (cross-platform build + CI) + Phase 2 (macOS code,
Tasks 4-9) are COMPLETE & CI-green**; you are now executing **Phase 3 (Linux)**.
Work via **superpowers:subagent-driven-development** (fresh subagent per task,
review between tasks, fix ALL findings incl. Minor), verifying everything
against live docs.

## READ FIRST (mandatory, in this order)
1. `AGENTS.md` — project bible (toolchain, deps, architecture, privacy §10.1,
   blaze NFRs, **"Multi-platform port status"** + **the 8 plan corrections**).
2. `.superpowers/sdd/2026-08-07-molvi-multiplatform-port/progress.md` — the SDD
   ledger (THE recovery map: per-task commits, review outcomes, fix rounds, spike
   results). Tasks with a `complete` line are DONE — resume at the first without one.
3. `docs/next-session-handoff.md` — this-port briefing + hard context + resume steps.
4. `docs/superpowers/plans/2026-08-07-molvi-multiplatform-port.md` — THE plan
   (Phase 3 = Tasks 11-13). The plan is authoritative where it matches reality;
   the 8 corrections in AGENTS.md override the plan text where they conflict.
5. `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md` — design
   spec (decisions D1–D6). NOTE: the spec §Architecture still says macOS paste =
   `Key::Unicode('v')` + `Key::Command` and leans the ashpd portal for Wayland —
   BOTH SUPERSEDED by the plan + the 8 corrections (Key::Other(9)+Key::Meta;
   compositor-keybinding). Trust the plan/corrections over the spec.

## HARD RULES (non-negotiable)

### 1. VERIFY EVERYTHING AGAINST LIVE DOCS — never from memory.
Every crate/API/signature must be re-checked via the `find-docs` skill
(Context7: `npx ctx7@latest …`) + docs.rs/crates.io BEFORE coding, and for
multi-model crates (transcribe-rs/parakeet-rs/objc2/tauri-nspanel/enigo) verify
against the **resolved source on disk** (`~/.cargo/registry/src/index.crates.io-*/`
and `~/.cargo/git/checkouts/`). Live ctx7 IDs: `/pykeio/ort`, `/enigo-rs/enigo`,
`/ahkohd/tauri-nspanel`, `/websites/v2_tauri_app`, `/cjpais/transcribe-rs`,
`/altunenes/parakeet-rs`. For Phase 3 specifically verify: x11rb (EWMH
`_NET_ACTIVE_WINDOW`/`_NET_WM_PID`), wl-clipboard/wl-copy, ashpd, and the Tauri
`tauri-plugin-single-instance` argv-forwarding API.

### 2. BLAZE = the prime directive (performance, not compatibility).
- Default RU/PTT/Smart path MUST hold **RTF ≤ 0.03** measured per platform.
- Hot loop (capture→engine→finalize→paste) MUST stay allocation/lock/blocking-free
  and contain **NO runtime platform branches** (compile-time `#[cfg]` only).
- **Nemotron feeds ONLY at the 8960-sample (560ms) boundary — DO NOT CHANGE.**

### 3. NO BACKWARD COMPATIBILITY — refactor freely. The gate is measured RTF +
clean architecture, not "the diff is byte-identical."

### 4. CLEAN CODE / CLEAN ARCHITECTURE.
- Inline `#[cfg(target_os = "...")]` per feature module + `[target.'cfg(...)'.dependencies]`.
  **NO `mod platform` abstraction, NO trait objects, NO dyn dispatch.**
- `ponytail:` comments for deliberate simplifications, SAFETY on all unsafe,
  fail-open on every OS-API error, privacy-safe logging (metadata only).
- DRY — reuse helpers (`paste_key()`, `paste_modifier()`, `letter_key()`,
  `redact_appdata`, `macos_frontmost_pid()`).

### 5. PRIVACY §10.1 (HARD).
NEVER log transcript/partials/post-proc/dict/history/snippet/command/prompt text
at any level. The 6 `log_privacy` substrates stay green.

### 6. BEAT THE COMPETITORS.
Wayland PTT = compositor keybinding → `molvi record toggle` IPC (NOT the ashpd
portal — `ashpd#213` breaks it for overlay apps; voxtype/whisrs/hyprwhspr
converged on keybindings). Wayland paste = `wl-clipboard` + blast-release
modifiers (NOT enigo libei — `enigo#453` panics in Tokio, `#336` rejected by
Chromium). The plan's Appendix has the full competitor research.

## EXECUTION APPROACH
- REQUIRED SKILL: `superpowers:subagent-driven-development` (load it before
  starting). Also `superpowers:executing-plans` for inline checkpoints.
- The SDD scripts are bash; this box is Windows PowerShell — replicate their
  logic with native tools (git, `gh`, `Set-Content`, `ConvertFrom-Json`). Create
  task briefs + reports under `.superpowers/sdd/2026-08-07-molvi-multiplatform-port/`.
- Execute **Phase 3** task-by-task: **Task 11** (Wayland PTT: `molvi record
  toggle` CLI subcommand → single-instance argv-forwarding → the existing PTT
  toggle command; + `docs/linux-install.md` compositor-keybinding per-compositor
  one-liners), then **Task 12** (Linux platform bodies: X11 `foreground_exe`
  via `_NET_WM_PID`→`/proc/<pid>/exe`; X11 `capture_target`/`ensure_focus` via
  `_NET_ACTIVE_WINDOW`; `has_disk_space` via `statvfs`; Wayland paste =
  wl-clipboard + blast-release), then **Task 13** (Linux packaging AppImage/deb/rpm).
- macOS/Linux code can only be COMPILE-verified from this Windows box (only the
  Windows arm compiles here); each Linux arm is verified by the **ubuntu CI job**
  after push. Runtime Linux behavior = a human smoke (parallel to Task 10 for Mac).
- After Phase 3 → `superpowers:finishing-a-development-branch` (PR #1 review/merge).

## GATES (every code task)
`cargo fmt --manifest-path src-tauri/Cargo.toml` + `cargo clippy --manifest-path
src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test --manifest-path
src-tauri/Cargo.toml --lib` + (`cargo test --manifest-path src-tauri/Cargo.toml
--test log_privacy` if no `molvi.exe` is locked) + `npx tsc --noEmit` + `npm run
build`. **Do NOT kill a running `cargo tauri dev`** — use `cargo check
--all-targets` + `cargo test --lib` if the dev app holds the binary. CI
(`.github/workflows/ci.yml`) re-runs on every push to `multiplatform-port` (PR #1).

## START
1. Read the 5 files above.
2. Load `superpowers:subagent-driven-development`.
3. Confirm the ledger + `git log --oneline -5` on `multiplatform-port` in 3–5 lines
   (you should see Phase-2 Task 9 at the tip).
4. Begin **Phase 3, Task 11**. Verify the Tauri single-instance argv API + the
   existing coordinator toggle command live before coding. Keep every commit's
   default path RTF ≤0.03 + hot loop untouched.

Note (fix on next AGENTS.md edit if still present): none — AGENTS.md + the
handoff + the ledger are current as of the Phase-2 completion. The spec's stale
bits (Key::Unicode/Command; portal-leaning Wayland) are already flagged above.
