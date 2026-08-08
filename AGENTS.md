# molvi — agent notes

Push-to-talk dictation app. Tauri 2 webview shell + local CPU ASR (two engines:
**GigaAM-v3** Russian via `transcribe-rs`, **Nemotron 3.5 ASR** multilingual via
`parakeet-rs`). UI fully internationalized (36 languages). **Multi-platform:**
Windows 11 x64 + macOS (Apple Silicon) + Linux — all 3 CI-green. **Auto-updater
wired** (ed25519 signing + GitHub Releases + `release.yml`); first draft release
`v0.1.0` verified end-to-end (3-OS build + `latest.json` + `.sig`, not published).

## Toolchain (pinned)

- Rust: stable, MSRV **rustc 1.97.1 (2026-07-14)** — `rust-toolchain.toml`, edition **2024**.
- Tauri runtime: **tauri 2.11.5** / tauri-build 2.6.3 / tauri-plugin-single-instance 2.4.3 / tauri-plugin-global-shortcut 2.3.2 / tauri-plugin-opener 2.5.4 / tauri-plugin-updater 2.10.1.
- Node: **v24.15.0+**; frontend = Vite 8 + TypeScript 7 (native Go port) + vanilla TS, no framework — `package.json`.
- OS targets: Windows 11 x64 (MSVC + WebView2), macOS aarch64-apple-darwin (Apple Silicon; Intel unsupported — no ort-sys dist.tsv row), Linux x86_64-unknown-linux-gnu. CI matrix (`.github/workflows/ci.yml`): windows-latest + macos-14 + ubuntu-latest.

## Dependencies (policy: latest as of 2026-08, Cargo.lock-verified)

- **Rule:** every crate/npm package targets the latest stable. Never trust plan code blocks or model memory for API signatures — verify against live docs via the `find-docs` skill (ctx7: `npx ctx7@latest …`), docs.rs, crates.io, or the npm registry before coding. ctx7 library IDs that are NOT stale: `/websites/v2_tauri_app` (Tauri 2 — the `/tauri-apps/api` id is stale), `/altunenes/parakeet-rs` (Nemotron), `/cjpais/transcribe-rs` (GigaAM), `/pykeio/ort` (ort — the dotted `/pyke.io/ort` id is STALE/404). WARNING: ctx7 _autodocs are UNRELIABLE for multi-model crates — verify against pinned source in `~/.cargo/registry`.
- **ASR = TWO crates:** `transcribe-rs` 0.3.11 (GigaAM) + `parakeet-rs` 0.3.7 (Nemotron). `ort` resolves to **2.0.0-rc.13**. ⚠ **ort-pin landmine (CONFIRMED load-bearing):** `transcribe-rs 0.3.11` pins `ort = "=2.0.0-rc.12"` (EXACT), `parakeet-rs 0.3.7` requires `2.0.0-rc.13` — mutually unsatisfiable. molvi's `[patch.crates-io]` transcribe-rs git override (Cargo.toml, rev `efc66111…`) relaxes the pin. A clean resolve SUCCEEDS on all 3 OSes. **Do NOT remove the patch.** ort emits tracing directly → see Logs.
- **macOS target deps** (`[target.'cfg(target_os="macos")'.dependencies]`): `tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }` (NOT on crates.io; overlay focus fix), `objc2 = "0.6"`, `objc2-foundation = "0.3"` (features NSURL+NSString), `objc2-app-kit = "0.3"` (features **NSWorkspace+NSRunningApplication REQUIRED** — bare "0.3" does NOT enable them; per-class features).
- **Apple-Silicon CoreML** (`[target.'cfg(all(target_os="macos", target_arch="aarch64"))']`): `transcribe-rs` feature `ort-coreml`, `parakeet-rs` `default-features=false` + `["cpu","ort-defaults","api-28","coreml"]`. ort/coreml compiles on aarch64-apple-darwin.
- **Linux target deps** (`[target.'cfg(all(unix, not(target_os="macos"))']`): `x11rb = "0.13"` (x11.rs foreground-app via `_NET_ACTIVE_WINDOW`/`_NET_WM_PID`), `libc = "0.2"` (statvfs disk-space).
- serde 1.0, serde_json 1.0, tracing 0.1, tracing-subscriber 0.3 (env-filter), tracing-appender 0.2, thiserror 2 (a transitive dep still pulls 1.x), `windows` 0.62 (Win32 APIs, `[target.'cfg(windows)']`).
- hf-hub 1.0.0, tokio 1.53, cpal 0.18 (Linux: ALSA), rtrb 0.3, rubato 4.0 (`Fft`+`audioadapter`; `FftFixedIn` was REMOVED in 4.0), arboard 3, enigo 0.6 (**no 0.7 exists**), rusqlite 0.40 (`bundled`), ureq 3.3, regex 1.13.
- npm: vite ^8, typescript ^7, @tauri-apps/api ^2, @tauri-apps/cli ^2, @tauri-apps/plugin-global-shortcut ^2, @tauri-apps/plugin-opener ^2.
- **No i18n / toast deps:** i18n = plain `Record<string,string>` + `t()` lookup; toasts = vanilla `mountToaster()`/`toast()` in `src/settings/ui.ts`.

## Commands

- `cargo tauri dev` — run the app (debug GUI; runs `npm run dev` first via `beforeDevCommand`). Long-running.
- `cargo tauri build` — NSIS/MSI/dmg/deb/AppImage (runs `npm run build` first).
- `cargo build --manifest-path src-tauri/Cargo.toml` — Rust only.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` — lint (must be warning-clean).
- `cargo fmt --manifest-path src-tauri/Cargo.toml` — format.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib` — unit tests (**189 model-free**; engine test is feature-gated `--features engine-model-test`, needs the ~2.6 GB model).
- `cargo test --manifest-path src-tauri/Cargo.toml --test log_privacy` — privacy-substrate integration tests (6).
- `npx tsc --noEmit` + `npm run build` — frontend gate (no JS test runner).
- **Binary lock:** if a live `molvi.exe` (running `cargo tauri dev`) locks the debug binary, `cargo build`/full `cargo test` fail at link — use `cargo test --lib` + `cargo check --all-targets`. Do NOT kill the human's running app.
- **Release workflow:** `.github/workflows/release.yml` — manual `workflow_dispatch` only, builds all 3 OSes + signs (ed25519) + creates a DRAFT release. Trigger: `gh workflow run release.yml` (default branch = main). See "Release pipeline" below.

## Architecture

### Rust (`src-tauri/src/`)

- **`lib.rs`** — Tauri builder + `AppState` (managed): `settings`, `cmd_tx`, `dictionary`, `snippets`, `history`, `original_affinity`, `mic_preview`, `pending_paste`, `last_failed_text`, `onboarding_practice`, `model_download`. `run()` = entrypoint; `main.rs` = one-line shim. Launch gate: `!settings.onboarded` → show "onboarding" window. Bg thread emits `engine-ready`/`engine-error`. **macOS-only:** `mod macos_overlay` (tauri-nspanel NSPanel, `can_become_key_window:false`), `secure_input_held()` (Carbon `IsSecureEventInputEnabled`, edition-2024 `unsafe extern "C"`), `.plugin(tauri_nspanel::init())`, `set_activation_policy(Accessory)`.
- **`ipc.rs`** — all `#[tauri::command]` IPC handlers: settings (`get_settings`/`set_settings` → emits `ui-lang-changed` → tray rebuild), dictionary (CRUD + import preview/apply), history (query + bulk_delete + distinct_langs), snippets (CRUD + import/export), onboarding, model picker (`model_status`/`download_model`/`cancel_model_download`/`restart_app`), updates (`check_update`/`apply_update`), inline-edit (`request_edit`/`confirm_paste`/`cancel_paste`/`paste_anyway`/`open_history`).
- **`settings.rs`** — `Settings` struct. `ui_lang` (default `"en"`) = UI language; `language` (default `"auto"`) = Nemotron recognition language. + model/hotkey/vad/overlay/history/post_processing/updater/endpoint/command_mode/profiles/snippets_enabled/backtrack_parsing/onboarded. `#[serde(default)]` → missing fields take `Default`; no version/migration.
- **`engine_adapter.rs`** — `SpeechEngine` trait (`feed_chunk`/`finish -> (String, Option<String>)`/`had_speech`), `NemotronEngine` (parakeet-rs streaming at the **8960-sample boundary**), `load_engine` dispatch on `settings.model`.
- **`engine.rs`** — GigaAM `Engine` (transcribe-rs `VadChunked`), worker loop (`EngineCmd::{Start,Feed,Finalize,Shutdown,AutoStop,Cancel}`), `SilenceTracker`, `finalize_session`/`finish_safely`.
- **`pipeline.rs`** — production `coordinator::Pipeline`. Session orchestration: capture → engine → finalize side-thread → post-proc → paste → history. `session_profile` resolved at `begin_session`. Finalize closure: command-mode dispatch → `apply_profile_override` → post-proc + paste + history. `run_finalize` = pure `postproc_final_text` + `paste_and_record`; Polished-only inline-edit window (`emit_edit_ready` → `resolve_edit` grace 1500ms).
- **`postproc.rs`** — Raw/Smart/Polished. Smart = pure `fn(&str)->String` steps in order: **backtrack (first)** → merge_chunks → inter_chunk_punct → repeated_marks → dup_words → apply_dictionary → **snippet expand (short-circuits on match)** → fix_case → fillers → ws. Privacy: pure text transforms, no `tracing::` of transcript.
- **`commands.rs`** — command-mode grammar → enigo `KeyChord`. 10 actions × 5 langs, flat data-driven table, whole-transcript exact match, linear scan (no regex/deps). `parse()` has ZERO tracing. **`letter_key(char)`** is `pub(crate) const fn` — per-platform: Windows VK (`Key::Other(0x41..)`, A=0x41); macOS `Key::Other(kVK_ANSI_*)` (A=0x00,C=0x08,V=0x09,X=0x07,Y=0x10,Z=0x06) — NOT `Key::Unicode` (AZERTY bug); Linux `Key::Unicode(char)`.
- **`paste.rs`** — per-platform paste: `paste_modifier()` (Win/Linux=`Key::Control`; macOS=**`Key::Meta`** — no `Key::Command` variant exists) + `paste_key()` (Win=`Key::Other(0x56)` VK_V; macOS=**`Key::Other(9)`** vkey 9=physical V; Linux=`Key::Unicode('v')`). macOS focus-guard = verify-only.
- **`snippets.rs`** / **`dictionary.rs`** — `Snippets`/`Dictionary` (conn + Mutex cache); `expand()`/apply = WHOLE-TEXT equality/token-sub. CRUD + CSV/JSON import/export (RFC-4180 via `csv_util.rs`).
- **`profiles.rs`** — per-app post-proc (NO DB). `foreground_exe()` (Win32 HWND→PID→basename; macOS NSWorkspace; Linux `_NET_ACTIVE_WINDOW` via x11rb) + `macos_frontmost_pid()` + `resolve()` + `apply_profile_override()`. UPPERCASED basename contract. `basename_upper(s)` helper (was triplicated, deduped).
- **`model_store.rs`** — hf-hub model download/cached-status. `ensure_model` async (block_on bridge from sync cold-start). `ModelProgressEmitter` (hf-hub 1.0 `ProgressHandler::on_progress` — NOT `handle`; ≤4Hz throttle). `model_status` byte-exact disk check. `has_disk_space` pre-check (Win `GetDiskFreeSpaceExW` / Linux `statvfs`). **Models cache to `%APPDATA%\com.molvi.app\models\<model-id>\` (NOT the HF cache).** ~2.6 GB total (gigaam ~214 MB + nemotron ~2.4 GB).
- **`updater.rs`** — `check`/`apply` via `tauri_plugin_updater::UpdaterExt`. Reads `pubkey`/`endpoints` from `tauri.conf.json` at runtime.
- **`x11.rs`** — Linux X11 EWMH: `_NET_ACTIVE_WINDOW`/`_NET_WM_PID` → foreground pid (x11rb; request methods on `xproto::ConnectionExt`, import explicitly; `GetPropertyReply.value` is `Vec<u8>` parse LE u32).
- **`overlay.rs`**, **`paths.rs`** (cross-platform appdata + `redact_appdata` stdlib-only, no `dirs`), **`hotkey.rs`**, **`history.rs`**, **`resample.rs`**, **`audio.rs`**, **`errors.rs`**, **`log.rs`**, **`coordinator.rs`** (trait + Mock test seam), **`ort_affinity.rs`**, **`tray.rs`** + **`tray_locales.rs`** (`TrayStrings`, 36-locale, en-fallback; `rebuild` re-labels in place), **`csv_util.rs`** (RFC-4180 std-only) — see file headers.
- **`tests/log_privacy.rs`** — 6 always-on privacy substrate tests (+ 2 model-gated). Keep green, never weaken.

### Frontend (`src/`)

Windows webviews (all HTML at repo ROOT; `vite.config.ts rollupOptions.input` lists settings/overlay/onboarding):
- `"settings"` (`index.html` → `src/settings/main.ts`)
- `"overlay"` (`overlay.html` → `src/overlay.ts` — own document, own `dir`)
- `"onboarding"` (`onboarding.html` → `src/onboarding.ts` — first-run dialog)

`src/settings/`: `main.ts` (sidebar + section dispatch + language picker + federated-search), `store.ts` (signal store), `persist.ts` (`patcher` + debounced `set_settings` + toast), `ui.ts` (component kit + toaster + `InfoTip`), `listen-safe.ts` (shared TOCTOU-safe `listen()`/`on()`), `sections/*.ts` (about/dictionary/history/hotkey/microphone/overlay/recognition/snippets/text/updates), **`sections/list-section.ts`** (shared list-editor factory — dictionary+snippets dedup), `types.ts`, `federated-search.ts`, `hotkey-capture.ts`, `icons.ts`.

`src/i18n/`: `types.ts` (`Lang` 36-member union, `Dict=Record<string,string>`), `locales/<lang>.ts` × 36, `locales.ts` (synchronous registry), `index.ts` (`t(key)` 3-level fallback current→en→raw; `setCurrentLang` sets `document.dir`/`lang`; `RTL_LANGS={ar,he}`).

**R4 invariant:** `src/settings/types.ts` mirrors `settings.rs` field-for-field. IPC rows are NOT Settings fields.

## ASR engines

- **GigaAM-v3** (`settings.model = "gigaam-v3-e2e-ctc"`) — monolingual Russian CTC via `transcribe-rs`. `settings.language` inert; `engine.rs` hardcodes `TranscribeOptions { language: Some("ru") }`. Punctuates natively (periods + commas). **Fastest + punctuated Russian path** (the blaze benchmark; see Performance NFRs).
- **Nemotron 3.5 ASR** (`settings.model = "nemotron-3.5-asr-streaming-0.6b"`, **default**) — multilingual (40 locales) via `parakeet-rs`. Honors `settings.language` (`"auto"` default or forced locale). **The default engine** (multilingual). Tradeoff vs GigaAM: streaming emits commas-only (no terminal periods) + ~2.4 GB first download (vs GigaAM ~214 MB).
- **Nemotron is STREAMING-ONLY (SETTLED — do not re-litigate).** Uses `transcribe_chunk_with_tokens` at the **8960-sample (560 ms) boundary** (LOAD-BEARING — feeding more often, e.g. every 30 ms VAD frame, regresses RTF ~0.31 → ~1.0; do NOT remove the boundary). `finish` flushes the <1-chunk tail with one zero-pad chunk. parakeet-rs streaming emits **ZERO terminal-period tokens AND ZERO lang-tag tokens** (commas survive). Net: Nemotron paste is fast (~0.2 s after release) but **commas-only, no terminal periods**, and **detected lang always falls back to `settings.language`**.
- **IF punctuation for Nemotron is ever revisited:** the planned path is a TOGGLE (`nemotron_punctuate: bool`; OFF = streaming-only/fast/default; ON = re-add an offline `transcribe_audio` pass at `finish`). The offline pass was tried (commit `68c5c0f`) and REVERTED (`9f11408`): restores periods + lang-detection but costs RTF ~0.65 at finish — human found that latency unacceptable. Do NOT re-investigate.
- **hf-hub 1.0 API (verified from source):** `ProgressHandler::on_progress(&self, &ProgressEvent)` (NOT `handle`); `Progress::new(handler)`; bon builder generates `.maybe_progress(Option<Progress>)` (NOT `.progress`); `download_file()` async, finish_fn=`send`; `HFClientBuilder::build()` async.
- Engine + language apply at startup (one worker; no hot-reload) — changing either needs an app restart.

## i18n (UI is 36-language)

- Module `src/i18n/`: the dictionary is one bundled object — switching is a synchronous property lookup, NOT a fetch. Do NOT introduce lazy-loading.
- Manifest ≈ 210 keys (`en` canonical; every lang's key set === `en`, set-equality verified ×36). Tokens `{name}`/`{n}`/`{total}`/etc. — ASCII-verbatim in ALL locales incl RTL+CJK.
- RTL (ar, he): `setCurrentLang` flips `dir="rtl"`; CSS uses logical properties (`inset-inline-*`, `border-inline-start`, `text-align: start`) — no physical `left/right` in layout CSS.
- Tray i18n (Rust): `tray_locales.rs` (`TrayStrings`, `TRAY_LOCALES` × 36, `tray_t(lang)`). `tray.rs build()` labels via `tray_t(ui_lang)`; `rebuild(app)` re-labels in place. `set_settings` calls `rebuild` when `ui_lang` changes.

## Hotkey (PTT)

- Default `Alt+`` = **LEFT Alt** only (Right Alt on RU/EU layouts = AltGr = Ctrl+Alt → fails Win32 `RegisterHotKey` MOD_ALT match). `hotkey.altgr_mirror` registers the `Ctrl+Alt+` mirror if enabled.
- **Paste + command-chord keys are per-platform** (`paste.rs` + `commands.rs::letter_key`) — see Architecture.

## UI conventions

- **Toasts:** `mountToaster()`/`toast(kind, message, opts?)` in `src/settings/ui.ts`. kind ∈ success/info/warning/error; auto-dismiss; `toast(…, {action:{label,onClick}})`.
- **`InfoTip`:** always-visible ⓘ with CSS hover/focus bubble. Used on `SettingsGroup(title, children, tip?)` 3rd arg + field labels.
- **AA-safe palette:** `--accent` = `#0E7C86` (text-safe 4.95:1); `--accent-bright` = `#2dd4bf` (decoration only); `--muted` = `#4B5563`.
- **Settings window:** 880×640, `resizable: false` (min==max clamps to fixed).
- **`fmtBytes`** (recognition.ts): `Intl.NumberFormat` unit formatting — MUST divide bytes by the unit factor (1e9/1e6/1e3) before formatting (Intl `unit:"gigabyte"` expects the value already in GB). Localized → ГБ/GB/Go per UI lang.

## Privacy (HARD RULE, spec §10.1)

NEVER log transcript text, partial transcripts, post-processed text, dictionary entries, history rows, snippet cues/expansions, command phrases, profile prompts, or audio samples — not even at `trace`. Logs carry metadata only. The detected recognition language (a locale code like `"en-US"`) and the foreground exe basename ARE metadata and may be logged; transcript text is NOT. Enforced by `src-tauri/tests/log_privacy.rs` (6 always-on substrates) — keep green, never weaken. `%APPDATA%` prefix (expands to `C:\Users\<name>\AppData\Roaming` — OS username is PII-adjacent) redacted at all path-bearing log/error sites via `paths::redact_appdata` (stdlib).

## Performance NFRs (blaze ratchet)

The default RU/PTT/Smart path's hot loop (capture→engine→finalize→paste) is the prime directive — RTF ≤ 0.03. The code-level invariant (that path byte-untouched across refactors) is the primary guarantee; empirical numbers are confirmation.

| Metric | Value | Source |
|---|---|---|
| Default RU/PTT/Smart RTF | ≤ 0.03 (varies with utterance length; fixed per-session overhead amortizes over longer audio) | live smoke |
| Cold-start to tray | ≤ 1251 ms (fast-path `block_on` + cached) | log |
| Nemotron streaming RTF | ~0.05 (8960-chunk boundary) | hotfix smoke |
| Release NSIS installer | ~11 MB | bundle |

## Release pipeline (DONE — updater-only, no OS code-signing)

- **ed25519 keypair** at `~/.tauri/molvi.key` (.pub) — generated, **back up offline** (it's the update-trust root; losing it = installed copies can't update).
- **`tauri.conf.json` `plugins.updater`:** real `pubkey` (byte-exact vs `~/.tauri/molvi.key.pub`) + `endpoints[0]` = `https://github.com/bumbaRasch/molvi/releases/latest/download/latest.json`. `createUpdaterArtifacts:true`, `windows.installMode:"passive"`.
- **GitHub secret** `TAURI_SIGNING_PRIVATE_KEY` = private key content (its presence → build emits `.sig` → tauri-action uploads `latest.json`; absence → silent skip).
- **`.github/workflows/release.yml`:** `workflow_dispatch` (manual), matrix windows-latest + macos-14 + ubuntu-latest (NO `--target`; macos-14 default host IS aarch64-apple-darwin), `tauri-action@v1`, `releaseDraft:true`, `updaterJsonPreferNsis:true`, `concurrency:{group:release, cancel-in-progress:false}`. Ubuntu deps mirror ci.yml + `patchelf xdg-utils` (AppImage bundling).
- **`latest.json` race:** 3 matrix jobs do read-modify-write in parallel — a snapshot can lose a platform. Verify all 3 platforms present after a release; re-run if incomplete.
- **Unsigned builds:** macOS Gatekeeper (right-click → Open), Windows SmartScreen (More info → Run anyway), Linux AppImage unsigned. OS code-signing = separate later effort (Apple Developer ID $99/yr; Azure Trusted Signing).

## Load-bearing DO-NOT-TOUCH (removing/regressing these breaks the build or blaze)

1. **`[patch.crates-io] transcribe-rs` git override** (Cargo.toml) — ort-pin landmine. Removing breaks the resolve.
2. **8960-sample Nemotron chunk boundary** (engine_adapter.rs) — removing regresses RTF ~0.31→~1.0.
3. **Privacy substrate** (log.rs + tests/log_privacy.rs) — never log transcript/audio/dict content.
4. **`[profile.release]`** (opt-level 3, codegen-units 1, lto "thin", strip, panic "unwind" — catch_unwind is load-bearing).
5. **Per-platform paste keys / `letter_key`** (paste.rs, commands.rs) — cross-platform correctness (macOS `Key::Other(9)`+`Key::Meta`, NOT `Key::Unicode`).
6. **macOS CoreML features + tauri-nspanel git dep + objc2 per-class features** (Cargo.toml target deps).
7. **Plain-object i18n + vanilla toaster** (no i18next/toast deps) — by design.
8. **Single `cargo tauri build` worker / no engine hot-reload** — engine+language apply at startup only.

## Doc verification (anti-stale rule)

Never trust model memory or ctx7 _autodocs for API signatures. Verify against live docs via `find-docs` (ctx7 `npx ctx7@latest docs <id> "<query>"`, max 3 calls/question), docs.rs, crates.io, npm registry, or pinned source in `~/.cargo/registry`. Verified ctx7 IDs: `/websites/v2_tauri_app`, `/pykeio/ort`, `/altunenes/parakeet-rs`, `/cjpais/transcribe-rs`.
