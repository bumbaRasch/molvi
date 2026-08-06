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

1. Read `AGENTS.md` (project bible) + this file.
2. Optionally: `docs/platform-portability.md` (the seed for Track A).
3. Invoke the **brainstorming** skill; ask the user which track (A / B / C, or
   "finish/ship v0.1 first"); explore that one to a spec.
4. Gates for any code work: `cargo fmt` + `clippy --all-targets -D warnings` +
   `cargo test --lib` + (binary-unlocked) `cargo test --test log_privacy` +
   `npx tsc --noEmit` + `npm run build`. Binary-lock note: don't kill a running
   `cargo tauri dev` — use `cargo check --all-targets` + `cargo test --lib` if
   the dev app holds molvi.exe.
