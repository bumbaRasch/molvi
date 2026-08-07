# Resume prompt — paste into the fresh session

> Copy everything in the block below (starting at "START") and paste it as your
> first message in the new session. The fresh session has no memory of this one —
> this prompt + the committed docs carry all the state.

---

START

Continuing work on **molvi** (Windows 11 push-to-talk dictation app — Tauri 2 webview shell + local CPU ASR). This is a fresh session after the Track A brainstorm (multi-platform port, open source). The **design phase is COMPLETE and committed**. We are now in the **PLANNING phase**.

## READ FIRST (in this order, mandatory)
1. `AGENTS.md` — the project bible (toolchain, dependencies, architecture, privacy §10.1, blaze NFRs, doc-verification rules). Already corrected during this brainstorm.
2. `docs/next-session-handoff.md` — the recovery map. Contains: what's done, exact next steps, gate commands, binary-lock caveat, OPEN decisions.
3. `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md` — **THE spec** (Track A design: decisions D1–D6, doc-verified crate matrix as of August 2026, 3 blockers, 3 spikes, inline-cfg architecture, per-platform specifics, Wayland scoping OPEN, NFRs).
4. `docs/superpowers/specs/2026-08-07-paste-focus-guard-spike.md` — spike #3 (paste focus-guard: macOS ⌘V, tauri-nspanel, verify/restore/no-restore shape).
5. `docs/mobile-strategy.md` — mobile = separate product (not now).

## HARD rules
- **NEVER work from memory.** Re-verify every crate/API/signature against the `find-docs` skill (ctx7: `npx ctx7@latest …`) + docs.rs/crates.io as of August 2026. AGENTS.md lists the live ctx7 IDs: `/pykeio/ort` (NOT `/pyke.io/ort`), `/enigo-rs/enigo`, `/websites/v2_tauri_app`, `/cjpais/transcribe-rs`, `/altunenes/parakeet-rs`. For multi-model crates ctx7 autodocs are unreliable — cross-check against the pinned source in `~/.cargo/registry`.
- **Do NOT write code before the plan exists.** Sequence: brainstorm → spec (✅ done) → **writing-plans** → execute.
- **Blaze = PERFORMANCE NFR, not compatibility.** The main/Windows code may be refactored freely (backward compatibility is not required), but the default RU/PTT/Smart path holds RTF ≤ 0.03 + a hot loop free of allocations/locks/blocking — verified by measurement on each platform. Nemotron feeds ONLY at the 8960-sample boundary (do not change).
- **Privacy §10.1 HARD RULE:** never log transcript/partials/post-proc/dict/history/snippet/command/prompt text at any level. Keep the 6 `log_privacy` substrates green.
- **Architecture D2:** inline `#[cfg(target_os = "...")]` per feature module + `[target.'cfg(windows)'.dependencies]` in Cargo.toml. **NO `mod platform`** (doc-verified as premature for 6 single-use functions).

## NEXT STEP = invoke the `writing-plans` skill
Turn the spec into a phased implementation plan → `docs/superpowers/plans/2026-08-07-molvi-multiplatform-port.md`. Suggested phasing (from handoff §3):
- **Phase 1** — unblock cross-platform builds: license files (`MIT OR Apache-2.0`); **Step 0** (cfg-gate the 4 unconditional Win32 sites: `audio.rs:6-7`, `ort_affinity.rs:10,14`, `profiles.rs:13-18`, `paste.rs:9-10`; `model_store.rs:214` is already gated); **CI matrix** (`.github/workflows`: windows/macos-14/ubuntu — the CI **IS** the mechanism for spikes #1/#2; green on mac(Apple Silicon)/linux = engine spikes pass).
- **Phase 2** — macOS port (Apple Silicon): needs `tauri-nspanel` (overlay `focusable:false` is broken — tauri#14102), enigo Accessibility permission, paste = ⌘V (`Key::Command`, NOT Control).
- **Phase 3** — Linux/Wayland (Wayland scoping OPEN: portal vs evdev).
After the plan is written → execute it (via `executing-plans` / `subagent-driven-development`).

## Gate commands (for any code work)
`cargo fmt` + `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test --manifest-path src-tauri/Cargo.toml --lib` + (binary-unlocked) `cargo test --test log_privacy` + `npx tsc --noEmit` + `npm run build`. Binary-lock: do NOT kill a running `cargo tauri dev` — if it holds molvi.exe, use `cargo check --all-targets` + `cargo test --lib`.

## START
Read `AGENTS.md` + handoff + spec + spike #3, confirm your understanding of the plan back to me in 5–7 lines, then invoke the `writing-plans` skill and begin turning the spec into a phased plan. Ask clarifying questions as you go. Verify everything via find-docs/ctx7 — never from memory.

END
