# molvi — next-session handoff (briefing for brainstorm)

> Purpose: a self-contained briefing so a FRESH session (after a clean context)
> can pick up molvi and run a brainstorm without re-discovering the state. Read
> **AGENTS.md first** (the project bible — toolchain, deps, architecture,
> privacy §10.1, blaze NFRs), then this file.

## Where molvi is now

v0.1.0 — Windows 11 push-to-talk dictation app (Tauri 2 + local CPU ASR).
Two engines: **GigaAM-v3** (Russian, default, fast + punctuated) and **Nemotron
3.5 ASR** (multilingual streaming). UI fully internationalized (36 languages).

Commit history (branch `main`, newest first):
- `b3dc890` — docs: platform portability vision (Vosk + Linux/macOS/Pi/mobile)
- `7c5bbe1` — settings: drop dead CommandModeSettings + native snippets translations (8 langs)
- `d863ce9` — review: correctness fixes, complete snippets feature, prune dead code
- `de1a065` — init: molvi v0.1.0

All gates green at last check: `cargo fmt` + `clippy --all-targets -D warnings`
+ `cargo test --lib` (185) + `cargo test --test log_privacy` (6) + `tsc --noEmit`
+ `npm run build`. Full `cargo build` (release) NOT yet verified end-to-end.

## Done this session (the work behind the 3 post-init commits)

1. **Whole-codebase code review** (6 parallel review subagents, doc-grounded).
   0 Critical found. Applied: resampler cross-session FFT reset; history
   enable/disable now live; model_store AggregateProgress handler + pct clamp;
   history.ts query race guard; onboarding listener cleanup; empty-entry guards;
   dead `original_affinity` removed.
2. **Snippets feature completed** (was half-shipped: store+expand wired, no
   IPC/UI). Added 5 IPC commands + Settings section + sidebar/icon + i18n ×36
   (en+ru proper, 8 more native, 27 en-baseline).
3. **Dead-code prune** (ponytail-audit): `MolviError::Command/PostProc`,
   `ort_affinity` no-op Nemotron arm, `history::open_at`, `set_status` dead
   branch, `Store.sub`, `mountToaster`, dead CSS/exports, orphan i18n ×36.
4. **CommandModeSettings cut** (dead config; command-mode feature stays working
   via `recognition_mode==Command` + commands.rs).
5. **Universal-adapter research → DECISION: don't build.** molvi already has
   the `SpeechEngine` adapter trait; a runtime plugin system is a distraction
   from the blaze+accuracy core, breaks the pinned-revision supply-chain
   posture, and the whole ecosystem (MacWhisper/Talon/sherpa-onnx) converges on
   curated 2–3 engine lists. See research notes.

## Hard context (do not re-litigate)

- **Privacy §10.1 (HARD RULE):** never log transcript/partials/dict/history/
  snippet/command/prompt text — any level. Enforced by 6 `log_privacy` substrates.
- **Blaze NFRs:** default RU/PTT/Smart RTF ≤0.03; hot loop (capture→engine→
  finalize→paste) must stay allocation/lock/blocking-free. Nemotron feeds ONLY
  at the 8960-sample boundary (load-bearing — do not change).
- **Adapter already exists** (`SpeechEngine` trait + `load_engine` dispatch) —
  adding an engine is ~1 file, not a rewrite. Decision: stay curated.
- **Platform-portability doc** (`docs/platform-portability.md`) records the
  multi-platform vision + Vosk as the low-resource engine + the capability-
  filtered engine picker.

## Open execution items (the "finish / ship" track — NOT brainstorm)

- **Task 13 — brand mark** (waveform-`m` monogram). Last Phase-3 plan task.
  Pure UI, no Rust/perf/logging surface. Closes Phase-3 → merge decision.
- **Updater pubkey/endpoint** (`tauri.conf.json`) — RELEASE BLOCKER. Currently a
  placeholder; auto-update can't work until real ed25519 keys + a release feed
  are set. Deployment work, not code.
- **`cargo tauri build`** — NSIS/MSI installer not verified end-to-end this
  session (release profile: lto=thin, codegen-units=1, strip, panic=unwind).
- **Blaze RTF controlled re-measurement** — empirical confirmation the cuts
  didn't regress the default path; fills the AGENTS.md NFR row. Human-run.
- **Remaining 27 locale translations** for snippets.* (en-baseline pending a
  native pass). 9 are native (en/ru/de/es/fr/ja/zh/ko/ar/he).

## Brainstorm menu (the "grow" track — pick one to explore in the fresh session)

These are the strategic forks worth a brainstorm. They are NOT committed plans.

### A. Multi-platform port (most strategically exciting, best-seeded)
molvi is Windows-only; the vision (in `docs/platform-portability.md`) is Linux
(desktop UI + headless daemon), macOS, Raspberry Pi, mobile. Brainstorm angles:
- **Which target first?** (Linux desktop is closest to the current Tauri shell;
  headless daemon is the biggest architecture change; mobile is the biggest
  reach.)
- **Engine:** Vosk integration as the low-resource/mobile engine behind the
  existing `SpeechEngine` trait (~1 file). Streaming contract fits `feed_chunk`.
- **Headless daemon:** what's "molvi with no window"? IPC surface, hotkey on
  Linux (evdev?), audio capture (cpal is cross-platform), config UI via CLI/web?
- **Mobile:** Tauri 2 mobile (Android/iOS) — does the webview-shell + capture +
  clipboard model translate? Or a native rewrite?

### B. v0.2 features
- **Profiles UI editor** — `profiles` exist server-side (per-app post-proc
  override, `profiles.rs`) but there's NO Settings UI to manage them. Building
  the editor completes a shipped-but-unexposed feature (the snippets pattern).
- **Command-mode growth** — user-defined commands, more grammars, the dedicated
  command hotkey (the `CommandModeSettings` that was just cut could come back as
  a real feature).
- **LLM post-proc** — the Polished mode hits an OpenAI-compatible endpoint;
  richer prompt management, model choice, local-LLM integration.
- **Multi-utterance context / vocabulary learning.**

### C. Product differentiation
The competitor research says molvi wins on **speed (blaze) + accuracy + language
coverage + command grammar** — not plugin ecosystems. Brainstorm: which axis to
double down on for v0.2 to widen the lead over Dragon/Talon/Superwhisper.

## How to resume (for the fresh session)

> **Session 2026-08-07 outcome — Track A brainstorm COMPLETE.** Track A
> (multi-platform port, OSS) was chosen and designed to a doc-verified spec.
> Commits: `03348e8` (spec + mobile-strategy + AGENTS.md corrections),
> `d1759c6` (spike #3 paste focus-guard + spec fixes). The design phase is done;
> **the next work is IMPLEMENTATION: Step 0 + CI (below).**

1. Read `AGENTS.md` (project bible, now corrected) + this file.
2. **Read the spec: `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md`**
   — the full Track A design (decisions D1–D6, crate matrix, 3 blockers, 3 spikes,
   inline-cfg architecture, per-platform specifics, Wayland scoping OPEN, NFRs).
   Also `docs/superpowers/specs/2026-08-07-paste-focus-guard-spike.md` (spike #3).
3. **NEXT WORK = Step 0 + CI matrix** (the implementation entry point):
   - **Step 0 — make it compile on Linux/macOS:** move `windows` to
     `[target.'cfg(windows)'.dependencies]` in `src-tauri/Cargo.toml`, then
     cfg-gate the 4 unconditional Win32 import sites (model_store.rs:214 already
     done; main.rs:1 `windows_subsystem` is harmless cross-OS):
     - `src-tauri/src/audio.rs:6-7` (`PlaySoundW`/`SND_*`/`PCWSTR`)
     - `src-tauri/src/ort_affinity.rs:10,14` (`SystemInformation`/`Threading`)
     - `src-tauri/src/profiles.rs:13-18` (`Foundation`/`Threading`/`WindowsAndMessaging`/`PWSTR`)
     - `src-tauri/src/paste.rs:9-10` (`HWND`/`GetForegroundWindow`/`SetForegroundWindow`)
     Per spec D2: inline `#[cfg(target_os=...)]`, NO `mod platform`. Non-Windows
     bodies = stubs returning `None`/no-op for now (real macOS/Linux impls come
     in the port). `foreground_exe()`→`None`, `capture_target()`→`None`,
     `play_sound_file`→no-op, `ort_affinity`→no-op fail-open. **Keep the blaze
     hot loop free of runtime platform branches** (compile-time `#[cfg]` only).
   - **CI matrix** — `.github/workflows/ci.yml`: windows/macos-14/ubuntu runners
     running `cargo fmt --check` + `clippy --all-targets -D warnings` +
     `cargo test --lib` + `npx tsc --noEmit` + `npm run build`. macOS runner =
     Apple Silicon (runs spike #2 engine build — does ort/CoreML accept GigaAM/
     Nemotron?); ubuntu runner = spike #1 (Linux ort-CPU build). The CI **IS**
     the engine-spike mechanism — green CI on mac/linux = spikes #1/#2 passed.
4. **After Step 0 + CI green:** the macOS port is next (spec per-platform
   specifics). Note macOS needs `tauri-nspanel` (overlay `focusable:false` broken
   — tauri#14102) + enigo Accessibility permission; paste = ⌘V (`Key::Command`).
5. Gates for any code work: `cargo fmt` + `clippy --all-targets -D warnings` +
   `cargo test --lib` + (binary-unlocked) `cargo test --test log_privacy` +
   `npx tsc --noEmit` + `npm run build`. Binary-lock note: don't kill a running
   `cargo tauri dev` — use `cargo check --all-targets` + `cargo test --lib` if
   the dev app holds molvi.exe.
6. **Verify crates live (AGENTS.md rule):** use the `find-docs` skill (ctx7) +
   docs.rs/crates.io before coding — IDs `/pykeio/ort` (NOT `/pyke.io/ort`),
   `/enigo-rs/enigo`, `/websites/v2_tauri_app`, `/cjpais/transcribe-rs`,
   `/altunenes/parakeet-rs` (autodocs unreliable — verify against registry source).

### OPEN decisions (resolve during/after macOS port)
- **Wayland scoping** (spec §"Wayland scoping — OPEN"): Wayland is now the
  default/only session on current distros (KDE Plasma 6.8 removed X11; GNOME
  Wayland-default), but global-hotkey is X11-only upstream. Lean: Wayland-in-v1
  via `ashpd` GlobalShortcuts portal, gated on a Wayland-hotkey spike. Decide
  after macOS ships.
- **transcribe-rs ort-pin landmine:** `transcribe-rs 0.3.11` pins `ort ="=2.0.0-rc.12"`
  (exact), `parakeet-rs 0.3.7` wants rc.13 — mutually unsatisfiable; molvi's
  Cargo.lock=rc.13 ⇒ a `[patch]` override exists. **Confirm the override before
  any fresh Cargo.lock re-resolution** (a clean resolve may fail).
