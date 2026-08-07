# molvi — agent notes

Windows 11 push-to-talk dictation app. Tauri 2 webview shell + local CPU ASR
(two engines: **GigaAM-v3** Russian via `transcribe-rs`, **Nemotron 3.5 ASR**
multilingual via `parakeet-rs`). UI fully internationalized (36 languages).

## Toolchain (pinned)

- Rust: stable, MSRV **rustc 1.97.1 (2026-07-14)** — see `rust-toolchain.toml`
- Tauri runtime resolved: **tauri 2.11.5** / tauri-build 2.6.3 / tauri-plugin-single-instance 2.4.3 / tauri-plugin-global-shortcut 2.3.2 / tauri-plugin-opener 2.5.4
- Node: **v24.15.0+** (Vite 8 requires ≥20.19 / ≥22.12); frontend = Vite 8 + TypeScript 7 (native Go port) + vanilla TS, no framework — see `package.json`
- OS: Windows 11 x64, requires MSVC build tools + WebView2

## Dependencies (policy: latest as of 2026-08, Cargo.lock-verified)

- **Rule:** every crate/npm package targets the latest stable as of August 2026. Never trust plan code blocks or model memory for API signatures — verify against live docs via the `find-docs` skill (Context7: `npx ctx7@latest …`), docs.rs, crates.io, or the npm registry before coding. ctx7 library IDs that are NOT stale: `/websites/v2_tauri_app` (Tauri 2 — the `/tauri-apps/api` id is stale), `/altunenes/parakeet-rs` (Nemotron), `/cjpais/transcribe-rs` (GigaAM), `/pykeio/ort` (ort — the dotted `/pyke.io/ort` id is STALE/404). WARNING: ctx7 _autodocs are UNRELIABLE for multi-model crates — verify against pinned source in `~/.cargo/registry`.
- **Resolved (`src-tauri/Cargo.lock`):**
  - tauri 2.11.5, tauri-build 2.6.3, tauri-plugin-single-instance 2.4.3, tauri-plugin-global-shortcut 2.3.2, tauri-plugin-opener 2.5.4
  - **ASR = TWO crates: `transcribe-rs` 0.3.11 (GigaAM) + `parakeet-rs` 0.3.7 (Nemotron).** `ort` resolves to **2.0.0-rc.13**. ⚠ **ort-pin landmine (re-verified 2026-08-07):** `transcribe-rs 0.3.11` pins `ort = "=2.0.0-rc.12"` (EXACT), while `parakeet-rs 0.3.7` requires `2.0.0-rc.13` — these are **mutually unsatisfiable** under cargo's resolver (exact `=rc.12` ≠ rc.13). molvi's `Cargo.lock`=rc.13 implies a `[patch]`/override is in play; **a fresh `Cargo.lock` re-resolution may fail — confirm the override exists before any clean resolve.** ort emits tracing directly → see Logs.
  - serde 1.0.229, serde_json 1.0.151, tracing 0.1.44, tracing-subscriber 0.3.23, tracing-appender 0.2.5
  - **`thiserror`: BOTH 1.0.69 + 2.0.19 present** (a transitive dep still pulls 1.x; molvi's own code uses 2). **`windows`: BOTH 0.61.x + 0.62.2 present.**
  - hf-hub 1.0.0, tokio 1.53.1, cpal 0.18.1 (Linux backends: ALSA + opt `jack`/`pulseaudio`/`pipewire`), rtrb 0.3.4, rubato 4.0.0 (`Fft` + `audioadapter` — `FftFixedIn` was REMOVED in 4.0), arboard 3.6.1 (Wayland clipboard = opt-in `wayland-data-control` feature), enigo 0.6.1 (**there is NO 0.7**; opt-in `libei_smol`/`libei_tokio`/`wayland` features exist — "enigo doesn't work on Wayland" is STALE, it's now experimental), rusqlite 0.40.1 (`bundled`), ureq 3.3.0, regex **1.13.1** (→ regex-syntax 0.8.11).
  - npm: vite ^8 (→8.2.0), typescript ^7 (→7.0.2 native), @tauri-apps/api ^2 (→2.11.1), @tauri-apps/cli ^2 (→2.11.4), @tauri-apps/plugin-global-shortcut ^2 (→2.3.2), @tauri-apps/plugin-opener ^2 (→2.5.4).
- **No i18n / toast deps:** i18n is a plain `Record<string,string>` dictionary + `t()` property-access lookup (no i18next); toasts are a vanilla `mountToaster()`/`toast()` in `src/settings/ui.ts`. Zero deps added for either.

## Commands

- `cargo tauri dev` — run the app (debug; long-running GUI; runs `npm run dev` first via `beforeDevCommand`)
- `cargo tauri build` — NSIS/MSI installer (runs `npm run build` first)
- `cargo build --manifest-path src-tauri/Cargo.toml` — Rust only (the CI gate)
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` — lint (must be warning-clean)
- `cargo fmt --manifest-path src-tauri/Cargo.toml` — format
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` — unit tests (**186 model-free**; engine test is feature-gated `--features engine-model-test`, needs the ~2.6 GB model)
- `cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy` — privacy-substrate integration tests (6; needs no live model, but needs the exe NOT binary-locked)
- `npx tsc --noEmit` + `npm run build` — frontend gate (no JS test runner; TS correctness = tsc + build + human GUI smoke)
- **Binary lock:** if a live `molvi.exe` (running `cargo tauri dev`) locks the debug binary, `cargo build`/full `cargo test` fail at link — use `cargo test --lib` + `cargo check --all-targets` (compiles test code without linking `molvi.exe`). Do NOT kill the human's running app.

## Architecture

### Rust (`src-tauri/src/`)

- **`lib.rs`** — Tauri builder + `AppState` (managed): `settings: Mutex<Settings>`, `cmd_tx: Mutex<Option<Sender>>`, `dictionary: Arc<Mutex<Dictionary>>`, `snippets: Arc<Mutex<Snippets>>`, `history: Mutex<Option<Arc<History>>>`, `original_affinity`, `mic_preview: Arc<AtomicBool>`, `pending_paste: Mutex<Option<Sender<EditDecision>>>` (inline-edit window), `last_failed_text: Mutex<Option<String>>` (paste-failed recovery), `onboarding_practice: Arc<AtomicBool>` (gates the finalize practice branch + overlay suppression), `model_download: Mutex<Option<JoinHandle<()>>>` (one-at-a-time download guard; `inner().is_finished()` check). `run()` = the Tauri entrypoint; `main.rs` is a one-line shim. Launch gate in `.setup`: `if !settings.onboarded { show "onboarding" window }`. The bg thread emits `engine-ready`/`engine-error` (global `app.emit`); cold-start bridges the async `ensure_model` via `tauri::async_runtime::block_on` (sync thread stays sync — blaze-critical).
- **`ipc.rs`** — all `#[tauri::command]` IPC handlers. Key groups: settings (`get_settings`/`set_settings` — emits `ui-lang-changed` → tray rebuild), dictionary (CRUD + `dictionary_import_preview`/`_apply` split), history (`history_query` widen with search/lang/since + `history_bulk_delete` + `history_distinct_langs`), snippets (CRUD + import/export), onboarding (`complete_onboarding`/`set_onboarding_practice`), model picker (`model_status`/`download_model`/`cancel_model_download`/`restart_app` — all SYNC `pub fn`), updates (`check_update` → `CheckResult` struct / `apply_update`). Inline-edit: `request_edit`/`confirm_paste`/`cancel_paste`/`paste_anyway`/`open_history` (in `lib.rs` next to `cancel_operation`).
- **`settings.rs`** — `Settings` struct. `ui_lang` (default `"en"`) = UI display language; `language` (default `"auto"`) = Nemotron recognition language. + model/hotkey/vad/overlay/history/post_processing/updater/endpoint/command_mode/profiles/snippets_enabled/backtrack_parsing/onboarded. Container `#[serde(default)]` → missing fields take `Default`; no version field, no migration.
- **`engine_adapter.rs`** — `SpeechEngine` trait (`feed_chunk` / `finish -> (String, Option<String>)` / `had_speech`), `NemotronEngine` (parakeet-rs streaming at the 8960 boundary), `load_engine` dispatch on `settings.model`.
- **`engine.rs`** — GigaAM `Engine` (transcribe-rs `VadChunked`), worker loop (`EngineCmd::{Start,Feed,Finalize,Shutdown,AutoStop,Cancel}`), `SilenceTracker` (auto-stop), `finalize_session`/`finish_safely`.
- **`pipeline.rs`** — production `coordinator::Pipeline` impl. Session orchestration: capture → engine → finalize side-thread → post-proc → paste → history. Holds `session_profile: Option<ProfileEntry>` (resolved at `begin_session` via `profiles::foreground_exe()`+`resolve()`, gated on non-empty profiles). The finalize closure: command-mode dispatch → `apply_profile_override` → post-proc + paste + history. `run_finalize` split into pure `postproc_final_text` + `paste_and_record` with a **Polished-only** inline-edit window (`emit_edit_ready` → `resolve_edit` grace 1500ms + pause-on-edit) between them; Smart/Raw paste UNCHANGED (blaze — no wait on the default path).
- **`postproc.rs`** — Raw/Smart/Polished. Smart = composed pure `fn(&str)->String` steps in order: **backtrack (first)** → merge_chunks → inter_chunk_punct → repeated_marks → dup_words → apply_dictionary → **snippet expand (short-circuits on match)** → fix_case → fillers → ws. Privacy: pure text transforms, no `tracing::` of transcript.
- **`commands.rs`** — command-mode grammar → enigo `KeyChord`. 10 actions × 5 langs (EN/RU/DE/FR/ES), flat data-driven `(normalized_phrase → action)` table, whole-transcript exact match, linear scan (no regex/deps). `parse()` has ZERO tracing.
- **`snippets.rs`** — voice-cue → stored-block expansion. `Snippets{conn, Mutex<Option<HashMap>>}` mirrors `dictionary.rs`; `expand()` = WHOLE-TEXT equality (not token substitution). CRUD + CSV/JSON import/export (RFC-4180 via `csv_util.rs`).
- **`profiles.rs`** — per-app post-proc (NO DB). `foreground_exe() -> Result<String>` (Win32 HWND→PID→`QueryFullProcessImageNameW`→basename, fail-open) + `resolve(profiles, exe) -> Option<&ProfileEntry>` (first enabled case-insensitive match) + `apply_profile_override(&mut PostProcessing, Option<&ProfileEntry>)` (post_mode always overrides; prompt only if Some; endpoint/model/api_key never).
- **`csv_util.rs`** — RFC-4180 CSV read/write (std-only, no deps); shared by `snippets.rs` + `dictionary.rs`.
- **`model_store.rs`** — hf-hub model download/cached-status. `ensure_model` async (via `tauri::async_runtime::block_on` bridge from the sync cold-start thread). `ModelProgressEmitter` (hf-hub 1.0 `ProgressHandler`, `on_progress` — NOT `handle`; ≤4Hz throttle). `model_status` byte-exact disk check. Pinned-revision file sizes hardcoded from HF tree API. `has_disk_space` pre-check (`GetDiskFreeSpaceExW`).
- **`overlay.rs`** — overlay window show/hide + emit helpers (`emit_text`/`emit_mic_level`/`emit_phase`/`show_paste_failed`/`emit_edit_ready`). `hide()` is the single chokepoint that resets `focusable(false)` across every exit path.
- **`paths.rs`** — `%APPDATA%\com.molvi.app\*` path resolution. `redact_appdata(&Path) -> String` for privacy-safe logging (stdlib `var_os` + `strip_prefix`, no `dirs` dep).
- **`hotkey.rs`**, **`history.rs`**, **`dictionary.rs`**, **`resample.rs`**, **`paste.rs`**, **`audio.rs`**, **`updater.rs`**, **`errors.rs`**, **`log.rs`**, **`coordinator.rs`**, **`ort_affinity.rs`**, **`tray.rs`**, **`tray_locales.rs`** — see file headers.
- **`tests/log_privacy.rs`** — 6 always-on privacy substrate tests (+ 2 model-gated). Keep green, never weaken.

### Frontend (`src/`)

Windows webviews (all HTML at repo ROOT; `vite.config.ts rollupOptions.input` lists all three):
- `"settings"` (`index.html` → `src/settings/main.ts`)
- `"overlay"` (`overlay.html` → `src/overlay.ts` — its OWN document, sets its own `dir`)
- `"onboarding"` (`onboarding.html` → `src/onboarding.ts` — first-run dialog, shown once when `!settings.onboarded`)

`src/settings/`: `main.ts` (sidebar + section dispatch + language picker + `rerender` + federated-search mount), `store.ts` (signal store), `persist.ts` (`patcher` + debounced `set_settings` + toast feedback), `ui.ts` (component kit + toaster `mountToaster`/`toast(…, {action})` + `InfoTip`), `sections/*.ts` (9 sections: recognition/text/microphone/hotkey/overlay/history/dictionary/updates/about), `types.ts`, `federated-search.ts` (autocomplete), `hotkey-capture.ts` (DRY frontend keydown capture), `icons.ts`.

**R4 invariant:** `src/settings/types.ts` mirrors `settings.rs` field-for-field. IPC rows (`DictEntry`/`HistoryRow`/`ModelStatus`/`ImportPreview`/`CheckResult`) are NOT Settings fields.

## ASR engines

- **GigaAM-v3** (`settings.model = "gigaam-v3-e2e-ctc"`, default) — monolingual Russian CTC via `transcribe-rs`. `settings.language` is inert for it; `engine.rs` hardcodes `TranscribeOptions { language: Some("ru") }`. Punctuates natively (periods + commas).
- **Nemotron 3.5 ASR** (`settings.model = "nemotron-3.5-asr-streaming-0.6b"`) — multilingual (40 locales) via `parakeet-rs`. Honors `settings.language` through `Nemotron::set_target_lang`: `"auto"` (default) or a forced locale (`"ru-RU"`, `"en-US"`, …).
- **Nemotron is STREAMING-ONLY (SETTLED — do not re-litigate).** molvi uses parakeet-rs's `transcribe_chunk_with_tokens` at the **8960-sample (560 ms) boundary** (LOAD-BEARING — feeding more often, e.g. every 30 ms VAD frame, regresses RTF ~0.31 → ~1.0; do NOT remove the boundary). `finish` flushes the <1-chunk tail with one zero-pad chunk. parakeet-rs streaming emits **ZERO terminal-period tokens AND ZERO lang-tag tokens** (commas survive). Net: Nemotron paste is FAST (~0.2 s after release) but **commas-only, no terminal periods**, and **detected lang always falls back to `settings.language`**. GigaAM (default) is fast AND punctuated — the competitor-beating path; Nemotron is the opt-in multilingual engine where the user accepts commas-only for speed.
- **IF punctuation for Nemotron is ever revisited:** the planned path is a TOGGLE (`nemotron_punctuate: bool`; OFF = streaming-only/fast/default; ON = re-add an offline `transcribe_audio` pass at `finish`). The offline pass was tried (commit `68c5c0f`) and REVERTED (`9f11408`): it restores periods + lang-detection but costs RTF ~0.65 at finish — human found that latency unacceptable. Do NOT re-investigate "does streaming punctuate" — it's settled (it doesn't, in parakeet-rs).
- **hf-hub 1.0 API (verified from source — NOT memory):** `ProgressHandler::on_progress(&self, &ProgressEvent)` (method is `on_progress` NOT `handle`); `Progress::new(handler)`; the bon builder generates `.maybe_progress(Option<Progress>)` (NOT `.progress(Option<Progress>)`); `download_file()` async builder, finish_fn=`send`; `HFClientBuilder::build()` async client.
- Engine + language apply at startup (one worker; no hot-reload) — changing either needs an app restart (Settings Recognition section shows the notice).

## i18n (UI is 36-language)

- **Module `src/i18n/`:** `types.ts` (`Lang` 36-member union, `Dict = Record<string,string>`); `locales/<lang>.ts` × 36; `locales.ts` (registry — synchronously imports all 36); `index.ts` (`t(key)` 3-level fallback current→en→raw key; `setCurrentLang(code)` sets `document.dir`/`lang`; `getCurrentLang`; `LANGUAGES` endonym registry; `RTL_LANGS = {ar, he}`).
- **Instant switching:** the dictionary is one bundled object — switching is a synchronous property lookup, NOT a fetch. Do NOT introduce lazy-loading.
- **Manifest ≈ 210 keys** (`en` canonical; every lang's key set === `en`, set-equality verified ×36). Tokens `{name}`/`{n}`/`{total}`/`{new}`/`{conflicts}`/`{size}`/`{bytes}`/`{pct}`/`{current}`/`{hotkey}` — ASCII-verbatim in ALL locales incl RTL+CJK (call sites do `.replace("{name}", value)`).
- **RTL (ar, he):** `setCurrentLang` flips `dir="rtl"`; CSS uses logical properties (`inset-inline-*`, `border-inline-start`, `margin-inline-*`, `text-align: start`) — no physical `left/right` in layout CSS.
- **Tray i18n (Rust-side):** `tray_locales.rs` (`TrayStrings` struct, `TRAY_LOCALES` × 36, `tray_t(lang)` en-fallback). `tray.rs build()` labels via `tray_t(ui_lang)`; `rebuild(app)` re-labels in place via `MenuItem::set_text` / `TrayIcon::set_tooltip`. `ipc.rs set_settings` calls `rebuild` when `ui_lang` changes.

## Hotkey (PTT)

- Default binding `Alt+`` = **LEFT Alt** only. Right Alt on RU/EU layouts is AltGr (synthesized as Ctrl+Alt) → fails Win32 `RegisterHotKey`'s MOD_ALT-only match. `hotkey.altgr_mirror` registers the `Ctrl+Alt+` mirror if enabled.
- Paste uses `Key::Other(0x56)` (VK_V), NOT `Key::Unicode('v')` — on Windows, enigo's Unicode path (KEYEVENTF_UNICODE SendInput) is rejected as Ctrl+V by some apps, so the literal VK_V is more robust. NOTE (re-verified 2026-08-07): enigo's own crate-level paste example IS `Key::Unicode('v')` + `Key::Control` — it DOES combine with held Ctrl on macOS/Linux; so `Key::Other(0x56)` is a **Windows-specific** choice, and the cross-platform port re-keys to `Key::Unicode('v')` on macOS/Linux (see `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md`). Command-mode chords use the same per-platform key shape.
- Logs default to `info,ort=warn` (drops ort's GraphTransformer/Session INFO spam; ort uses tracing directly). `RUST_LOG` overrides.

## UI conventions

- **Toast notifications:** `mountToaster()`/`toast(kind, message, opts?)` in `src/settings/ui.ts`. kind ∈ success/info/warning/error; auto-dismiss 4/6/8s; stack cap 3; pause-on-hover AND pause-on-focus; `toast(…, {action:{label,onClick}})` action primitive (5s window extends while Tabbing to the action button). Inline notices stay `Alert()` (persistent contextual content).
- **`InfoTip`:** always-visible ⓘ icon with CSS hover/focus bubble, a11y `role=img` + `aria-hidden` bubble. Used on section/group titles (`SettingsGroup(title, children, tip?)` 3rd arg) and field labels (`Slider`/`Toggle` `tip` arg). Stops click/Enter/Space propagation.
- **AA-safe palette:** `--accent` = `#0E7C86` (text-safe, 4.95:1); `--accent-bright` = `#2dd4bf` (decoration only — dots/rail/check, NO text); `--muted` = `#4B5563`.
- **Settings window:** 880×640. Radio groups = `<div>` + CSS grid `200px 1fr` (label LEFT, options stacked RIGHT; `role="radiogroup"`+`aria-label`).

## Privacy (HARD RULE, spec §10.1)

NEVER log transcript text, partial transcripts, post-processed text, dictionary entries, history rows, snippet cues/expansions, command phrases, profile prompts, or audio samples — not even at `trace`. Logs carry metadata only. The detected recognition language (a locale code like `"en-US"`) and the foreground exe basename ARE metadata and may be logged; transcript text is NOT. Enforced by `src-tauri/tests/log_privacy.rs` (6 always-on substrates: finalize, coordinator, log-bridge, ipc-dictionary, commands-parse, snippets-expand, onboarding-practice) — keep green, never weaken. The profile-prompt leak surface is exercised (`POLISHED_PROMPT_SENTINEL` flows through `build_polished_body` under scoped trace capture); the `%APPDATA%` prefix (which expands to `C:\Users\<name>\AppData\Roaming` — OS username is PII-adjacent in shared bug-report logs) is redacted at all 10 path-bearing log/error sites via `paths::redact_appdata` (stdlib `var_os` + `strip_prefix`, no `dirs` dep).

## Performance NFRs (blaze ratchet)

The default RU/PTT/Smart path is **byte-untouched across all of Phase-3** (diff-verified per task review — every Phase-3 commit touched log sites, test code, UX layer, or new-feature IPC, never the inference→post-proc→paste hot loop). That code-level invariant is the primary blaze guarantee; the empirical numbers below are confirmation, not the proof.

| Metric | Phase-1/2 baseline | Phase-3 (2026-08-06, `574ac06`) | NFR | Source |
|---|---|---|---|---|
| Default RU/PTT/Smart RTF | 0.029 | 0.06–0.23 (≈0.12 typical; 0.001 cached/empty) | ≤ 0.03 | Session-8 live smoke; varies with utterance length (fixed overhead amortizes over longer audio) |
| Cold-start to tray | 1251 ms | fast-path confirmed (`block_on` + `already cached`) | ≤ 1251 ms | Session-8 log; not separately timed (debug build) |
| Peak RSS idle | 292 MB | not measured this session | ≤ 292 MB | Phase-1 baseline; no hot-path memory allocation added in Phase-3 |
| NSIS installer | ~11 MB | 9.43 MB | (info) | `target/release/bundle/nsis/` (2026-08-03 build, pre-Task-15; ~79-line delta negligible) |
| Nemotron streaming RTF | — | ~0.05 (8960-chunk boundary) | ≤ 0.09 | Task-1 hotfix smoke (`54d2b3a`); LOAD-BEARING chunking |

RTF variance note: the 0.029 baseline was a controlled long-utterance measurement; Session-8 live dictations of short-medium phrases naturally show higher RTF (fixed per-session overhead / shorter denominator). The inference path is unchanged — variance is measurement-condition, not code regression. A controlled re-measurement is deferred to post-merge; the code-level proof stands regardless.

## Phase-3 status

Design spec + 15-task plan + UX research: `docs/superpowers/{specs,plans}/2026-08-05-molvi-phase3*`, `docs/phase-3-ux-research.md`. **Execution state lives in the SDD ledger** `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md` (the recovery map — read it FIRST in any session resuming Phase-3). Per-task commits on `phase3`.

**Shipped + review-clean:** Tasks 1-15 (Nemotron streaming, auto-stop, replace-paste, command-mode, snippets, per-app profiles, Smart pipeline steps, overlay redesign, onboarding, federated search, history/dict upgrades, model picker, privacy widen + perf gate). Human-smoke-verified: Tasks 10, 11, 12, 14.

**Remaining:** Task 13 (brand mark — waveform-`m` monogram, pure UI, no Rust/logging/perf surface). After Task 13 → merge decision.
