# Model Picker — Design Spec

**Date:** 2026-08-06
**Branch:** `phase3` (Task 14 in the Phase-3 plan)
**Status:** Design — pending implementation plan
**Author:** controller + human (brainstormed)

## 1. Overview

molvi ships two ASR engines behind one `Box<dyn SpeechEngine>` trait:

- **GigaAM-v3** (`gigaam-v3-e2e-ctc`, default) — monolingual Russian CTC via
  `transcribe-rs`. Fast, natively punctuated. ~80–100 MB.
- **Nemotron 3.5 ASR** (`nemotron-3.5-asr-streaming-0.6b`) — multilingual (40
  locales) RNN-T via `parakeet-rs`. Streaming commas-only (no terminal periods
  — settled, see ledger). ~2.6 GB.

### Problem

A basic engine **Select already exists** (`recognition.ts:165`) that writes
`settings.model` + shows "restart needed." But the **download experience is
broken**: post-onboarding, switching to Nemotron (uncached) + restart triggers
a **silent** 2.6 GB download at startup with **no progress UI in the settings
window** (only the onboarding window had a progress bar). The app appears
frozen. There is no cancel, no recovery, no status visibility.

### Goal

Make engine switching a **guided, progress-visible, non-blocking, recoverable**
flow — and keep the default RU/PTT/Smart path untouched (blaze hard rule).

### Success criteria

1. User sees both models with their cache status (active / ready / downloading
   X% / not downloaded) in a 2-card picker inside the Recognition section.
2. Picking an uncached model starts a **background** download (GigaAM keeps
   working — no freeze, no blocking); real `bytes / total — pct%` progress +
   cancel.
3. On completion, a toast notifies "ready"; the user selects + one-click
   restart to activate.
4. `settings.model` **always points to a cached model** — restart never lands
   in a silent startup download (download and activate are separate actions).
5. Partial/canceled downloads are detected and cleanly re-downloaded.
6. No regression to default RU/PTT/Smart RTF/cold-start. No new dependencies.

## 2. Non-goals (YAGNI — explicitly out of scope)

- **Hot-reload** (switch engine without restart). Architecturally clean (new
  `EngineCmd::ReloadEngine` that swaps the `Box<dyn SpeechEngine>` when idle)
  and this design is forward-compatible (the Active-vs-Selected distinction is
  exactly what hot-reload reconciles), but deferred to a follow-up task. See
  §11.
- **Auto-resume** of an interrupted download on app launch (user re-clicks
  Download; hf-hub's content cache resumes — no re-download of completed
  files).
- **Delete cached model / disk management** (future).
- **Eager pre-download** of Nemotron for all users (wastes 2.6 GB for RU-only
  users — rejected).
- Folding the language Select into the Nemotron card (separate Select below
  the picker is simpler).

## 3. Architecture

### 3.1 State model

Each model has a status derived from **disk + `settings.model` + the loaded
engine**:

| Card state | Meaning | UI |
|---|---|---|
| `active` | cached AND `settings.model == this` AND this is the loaded engine | ✓ "Активна", accent border |
| `ready` | cached, not selected | radio dot; click = select |
| `selected` | cached, `settings.model == this`, but NOT yet loaded (restart pending) | "Перезапустите для переключения" + "Перезапустить" button |
| `downloading` | background download in progress | progress bar + "bytes / total — pct%" + "Отмена" |
| `not_downloaded` | files missing OR size-mismatch (partial/corrupt) | "Скачать (size)" button |

**Loaded-engine detection (no new IPC):** reuse the page-open snapshot pattern
already in `recognition.ts` (`initialModel`). At settings-page open,
`settings.model` == the engine loaded at last startup (the only thing that
changes `settings.model` is the picker itself). Divergence (current selection
≠ `initialModel`) = "restart pending." No new telemetry.

**Key invariant:** `settings.model` is only ever set to a model whose status is
`ready` or `active` (i.e. fully cached). Download does NOT change
`settings.model`. This guarantees a restart never hits an uncached-model
startup download.

### 3.2 Data flow

```
User clicks "Скачать (2.6 ГБ)" on not_downloaded Nemotron card
  → invoke("download_model", {model_id})
  → ipc: guard (one-at-a-time, not active) → spawn async ensure_model
  → AppState.model_download = Some(JoinHandle)
  → model_store: per-file download_file().progress(handler).send().await
     handler.on_progress (≤4Hz throttle):
       → app.emit("model-download-progress", {model, bytes, total, pct})
  → on Ok:  emit("model-download-complete", {model})  → card → ready + toast
  → on Err: emit("model-download-failed",    {model}) → card → "Повторить"
  → AppState.model_download = None

User clicks the ready Nemotron card (select)
  → patch settings.model = nemotron  (card → selected, restart pending)
  → langSel shows (Nemotron-only)

User clicks "Перезапустить"
  → invoke("restart_app") → app.restart()  (already used in updater.rs)
```

Cancel: `invoke("cancel_model_download")` → `AppState.model_download.abort()`
→ task aborts; hf-hub content cache retains completed chunks; card →
`not_downloaded` (size-mismatch detected). Re-click Download resumes.

## 4. Backend

### 4.1 `model_store.rs` — sync → async + progress + manifest

**Verified against hf-hub 1.0.0 source** (`~/.cargo/registry/src/.../hf-hub-1.0.0/src/progress.rs` + `repository/download.rs`) — this is the ground truth, NOT memory:

- `ProgressHandler` trait: `fn on_progress(&self, event: &ProgressEvent)`,
  bound `Send + Sync`. **Must not block** (called on the stream-read path; a
  slow handler slows the transfer), must not panic, must be idempotent
  (ordering guaranteed, deduplication NOT).
- `Progress` wrapper: `Progress(Arc<dyn ProgressHandler>)`. Construct via
  `Progress::new(handler)` or `impl Into<Progress>` from owned/`Arc`.
- `download_file()` builder accepts `.progress(Option<Progress>)`.
- `DownloadEvent`:
  - `Start { total_files: usize, total_bytes: u64 }`
  - `Progress { files: Vec<FileProgress> }` — **per-file DELTA** (only changed
    files; consumer accumulates by filename)
  - `AggregateProgress { bytes_completed: u64, total_bytes: u64, bytes_per_sec: Option<f64> }`
  - `Complete` (success only; on failure rely on the returned `Result`)
- `FileProgress { filename: String, bytes_completed: u64, total_bytes: u64, status: FileStatus }`.

**Changes:**

1. **File-size manifest.** `FILES` / `FILES_NEMO` become
   `&[(repo_name, on_disk_name, expected_bytes)]`. The implementer fetches the
   exact pinned-revision sizes once (HF API at the pinned SHA) and hardcodes.
   Pinned revisions (`HF_REVISION` / `HF_REVISION_NEMO`) are immutable commit
   SHAs → sizes never change. Comment: "pinned-revision-stable; recompute if
   `HF_REVISION*` rolls forward." Used for (a) status completeness check, (b)
   progress grand-total.

2. **`ensure_model` → `async fn`.** Switch `HFClientBuilder::build_sync()` →
   `.build()` (async `HFClient`) + `download_file().filename().revision()
   .local_dir().progress(Some(handler)).send().await`. The existing callers
   (onboarding bg thread, startup) already run inside
   `tauri::async_runtime::spawn` — they `await` it.

3. **`ProgressHandler` impl** (`ModelProgressEmitter`):
   - Holds `AppHandle` (Clone+Send+Sync) + `model_id: String` +
     `Arc<AtomicU64> bytes_completed` + `Arc<AtomicU64> last_emit_ms`.
   - `on_progress`: on `Progress`/`AggregateProgress`, `bytes_completed
     .fetch_add(delta, Relaxed)`. **Throttle ≤4 Hz:** compare `now_ms` to
     `last_emit_ms`; if ≥250ms, `fetch_max` + `app.emit("model-download-
     progress", {model, bytes, total, pct})`. Non-blocking (atomics + emit
     only). `total` = the manifest grand-total (constant); `pct = bytes /
     total * 100`.
   - **Privacy §10.1:** emits carry only `model_id` + byte counts + %. No file
     content, no transcript. The `filename` in `FileProgress` is the model
     artifact name (e.g. `encoder.onnx.data`) — metadata, safe. Do NOT emit
     or log anything beyond bytes/%/artifact-name.

4. **`model_status(model_id) -> ModelStatus`** (pure, reads disk): returns
   `Active`/`Ready`/`NotDownloaded` based on whether all manifest files exist
   AND match `expected_bytes` (catches partial/corrupt). `Active` vs `Ready`
   is resolved frontend-side from `settings.model` + `initialModel` (no IPC
   for "loaded"). Used by the `model_status` IPC on page open.

### 4.2 `lib.rs` — AppState + spawn

- `AppState` += `model_download: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>`.
- Download spawns via `tauri::async_runtime::spawn`; the `JoinHandle` stored in
  `model_download`. On completion/failure the task clears it (or the IPC
  clears on next call).
- The existing onboarding/startup `ensure_model` call is UNCHANGED (it just
  `await`s the now-async fn). It passes `progress(None)` — onboarding listens
  to `engine-ready`/`engine-error` (Task 10's indeterminate bar), not per-byte
  progress; the real-progress `ProgressHandler` is used ONLY by the picker's
  `download_model` path.

### 4.3 `ipc.rs` — new commands + events

```rust
#[derive(Serialize)]
pub struct ModelStatus { pub model_id: String, pub cached: bool }  // cached = files present + size-ok

#[tauri::command] async fn model_status(state) -> Result<Vec<ModelStatus>>
#[tauri::command] async fn download_model(model_id, app, state) -> Result<()>  // guard + spawn
#[tauri::command] async fn cancel_model_download(state) -> Result<()>           // abort
```

Events (global `app.emit`):
- `model-download-progress  { model, bytes, total, pct }` (≤4 Hz)
- `model-download-complete  { model }`
- `model-download-failed    { model }`

Guards in `download_model`:
- If `model_download` is `Some` → `Err(MolviError::ModelStore("download already in progress"))` → frontend toast.
- If the model is already cached (status check) → no-op (or `Err`).
- If the model is the currently-active+loaded one → no-op (it's cached).

### 4.4 Disk-space pre-check (Windows)

Before spawning the 2.6 GB download, call `GetDiskFreeSpaceExW` on the models
dir's drive. If `free < grand_total + slack` → `Err(MolviError::ModelStore(
"insufficient disk space"))` → card shows "Недостаточно места (нужно {total},
свободно {free})". Avoids a 10-minute-then-fail. Verified via `windows` crate
already in `Cargo.toml` (the `Win32_Storage_FileSystem` feature — implementer
confirms the feature flag is present, else adds it; NO new crate).

## 5. Frontend — picker UI

### 5.1 Placement (`recognition.ts`)

Replaces the engine `Select` (`recognition.ts:165-176`) at the same position.
Recognition order becomes:

1. `modeGroup` (PTT/Toggle/Command radio + ⓘ) — unchanged
2. **MODEL PICKER** (2 cards) — new, replaces engine Select
3. `langSel` (Nemotron-only) + `langWarnAlert` — unchanged logic, `sync()`
   now keys off the picker's selected model
4. `restartAlert` — repurposed ("restart to activate")
5. `advanced` (VAD `<details>`) — unchanged

### 5.2 Card structure

Two `.model-card` elements stacked vertically. Each card:
- **Model name** (proper noun, untranslated: "GigaAM" / "Nemotron") + tagline.
- **One-line description** (localized): GigaAM = "Русский — быстрый, с
  пунктуацией"; Nemotron = "Мультиязычный — 40 языков".
- **Size** (localized unit): "~80 МБ" / "~2.6 ГБ".
- **Status / action** (right or below):
  - `active`: ✓ + "Активна", accent border (selected styling).
  - `ready`: radio dot; whole card clickable → select.
  - `selected`: "Перезапустите для переключения" + "Перезапустить" button
    (`invoke("restart_app")`); revert = click the other card.
  - `not_downloaded`: "Скачать (size)" button → `invoke("download_model")`.
  - `downloading`: thin full-width progress bar + "bytes / total — pct%" +
    "Отмена" button → `invoke("cancel_model_download")`.

**Layout stability ("nothing jumps"):** card height is content-driven and
fixed per state; the `not_downloaded → downloading` transition swaps the
button for a same-row progress bar; `ready → selected` swaps only the
badge/border. No reflow of siblings.

### 5.3 Event wiring

- On render: `invoke("model_status")` → set each card's cached/not_downloaded;
  `active`/`ready`/`selected` from `settings.model` vs `initialModel`.
- Listen `model-download-progress` → update the downloading card's bar + text.
- Listen `model-download-complete` → card → `ready` + toast "Nemotron готов —
  выберите и перезапустите".
- Listen `model-download-failed` → card → "Ошибка загрузки — Повторить".

### 5.4 Cleanup (backward compat NOT needed — clean break)

Remove now-unused keys: `recognition.engine`, `recognition.engine_gigaam`,
`recognition.engine_nemotron` (× 36 locales). The engine `Select` is gone.

## 6. Edge cases & error handling

- **Partial/corrupt file after cancel/crash:** `model_status` size-check →
  `not_downloaded`; re-download via hf-hub cache resumes (completed chunks
  retained).
- **Download fails (network/disk):** task `Err` → `model-download-failed` →
  card "Повторить" (re-invokes `download_model`; hf-hub retries transient
  failures internally).
- **App killed mid-download:** task dies; next launch `model_status` shows
  `not_downloaded` (size-mismatch); user re-clicks Download; hf-hub resumes.
- **Settings window closed mid-download:** download continues (lives on the
  app runtime, not the window); reopening → `model_status` + live events.
- **User selects model A, then model B (both ready) before restart:** each
  click sets `settings.model`; last one wins; only one restart needed.
  Non-issue.
- **Concurrent `download_model` calls:** guard rejects the 2nd (one
  `JoinHandle` in `AppState`).
- **Disk full mid-download:** hf-hub write fails → `model-download-failed` →
  retry (pre-check in §4.4 catches most cases upfront).

## 7. Privacy (§10.1)

- Progress events: `model_id` + byte counts + %. No content. Safe.
- `FileProgress.filename` (artifact name) is metadata — may be used in
  `on_progress` if needed for per-file tracking, but NOT logged at `tracing`
  (keep logs metadata-only: model_id + pct, as the existing `ensure_model`
  `tracing::info!` does).
- No transcript/partials/dict/history/audio involved anywhere. The model
  picker never touches inference output.

## 8. i18n (~12 new keys × 36 locales, `models.*` cluster)

Canonical EN (translate meaning; "GigaAM"/"Nemotron" stay verbatim):

```
"models.gigaam_desc":   "Russian — fast, with punctuation."
"models.nemotron_desc": "Multilingual — 40 languages."
"models.active":        "Active"
"models.download":      "Download ({size})"
"models.downloading":   "{bytes} / {total} — {pct}%"
"models.cancel":        "Cancel"
"models.restart_to_activate": "Restart to activate"
"models.restart_btn":   "Restart"
"models.download_complete": "Model ready — select it and restart."
"models.download_failed":  "Download failed — Retry"
"models.retry":         "Retry"
"models.insufficient_space": "Not enough disk space (need {need}, {free} free)"
"models.already_downloading": "A download is already in progress."
```

(`{size}`, `{bytes}`, `{total}`, `{pct}`, `{need}`, `{free}` are runtime
tokens — verbatim in every locale.) Size unit localizes (ГБ/GB/MB etc.).
Consider reusing `recognition.restart_notice` for `restart_to_activate` if the
wording fits — else new key.

## 9. Files changed

**Rust:**
- `src-tauri/src/model_store.rs` — sync→async, `ProgressHandler` impl,
  file-size manifest, `model_status` disk check, disk-space pre-check.
- `src-tauri/src/lib.rs` — `AppState.model_download`; spawn wiring; events.
- `src-tauri/src/ipc.rs` — `model_status` / `download_model` /
  `cancel_model_download` / `restart_app` commands; register in
  `invoke_handler`.
- `src-tauri/Cargo.toml` — IF the `windows` feature for
  `GetDiskFreeSpaceExW` (`Win32_Storage_FileSystem`) isn't already enabled
  (implementer verifies `profiles.rs` uses nearby Win32 features), add it. No
  new crate.
- `src-tauri/capabilities/settings.json` — whitelist the new
  commands + event listens.

**Frontend:**
- `src/settings/sections/recognition.ts` — picker (replaces engine Select),
  event wiring, `sync()` adaptation.
- `src/settings.css` — `.model-card`, progress bar, badges.
- `src/settings/types.ts` — `ModelStatus` TS mirror type (R4).
- `src/i18n/locales/*.ts` — +`models.*` keys, −`recognition.engine*` keys.

## 10. Testing

- **Rust unit (`--lib`):** `model_status` disk-check logic (present+correct
  size → cached; missing/wrong-size → not) against a temp dir with fixture
  files. `is_nemotron` already tested. The progress-handler throttle (≤4 Hz)
  is testable with a fake clock/atomic. The disk-space guard is testable by
  mocking the free-space read (or feature-gated like the engine tests).
- **No live 2.6 GB download in CI** — the actual `ensure_model` network path
  is manual-smoke (as today). Unit tests cover the pure logic (status check,
  manifest, throttle, guards).
- **Privacy:** no new `log_privacy.rs` substrate needed (picker emits
  metadata only; verify no `tracing::*` interpolates content — there is none
  to interpolate).
- **TS:** `tsc --noEmit` + `vite build` green. Card state transitions are
  human-smoke (GUI).
- **Gates:** `cargo fmt` + `cargo clippy --all-targets -- -D warnings` +
  `cargo test --lib` (binary-lock fallback: `cargo check --all-targets`).

## 11. Future enhancements (deferred)

- **Hot-reload** (switch engine without restart): new
  `EngineCmd::ReloadEngine { model_id, language }`; the worker, when idle,
  drops the old `Box<dyn SpeechEngine>` and calls `load_engine` again. The
  `SpeechEngine` trait + worker channel already abstract this. This design's
  Active-vs-Selected distinction is exactly what hot-reload reconciles → no
  rework; "restart to activate" becomes "auto-activate."
- **Delete cached model** (disk management) — a "Удалить" action per cached
  non-active model.
- **Auto-resume on launch** if a download was interrupted (persist a
  pending-download flag).

## 12. Verification TODOs for the implementer (docs-first)

1. **hf-hub 1.0.0 API** — re-confirm against
   `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/hf-hub-1.0.0/src/`
   (progress.rs, repository/download.rs): `ProgressHandler::on_progress`
   signature, `Progress::new`, `download_file().progress(Some(...))`,
   `DownloadEvent` variants, the must-not-block contract. (Controller already
   verified 2026-08-06; re-confirm if any uncertainty.)
2. **File sizes** — fetch the exact pinned-revision sizes for both models (HF
   API: `https://huggingface.co/api/models/{owner}/{name}/revision/{sha}` or
   `tree?recursive=true`) and hardcode the manifest.
3. **`windows` feature flag** — confirm `Win32_Storage_FileSystem` is enabled
   for `GetDiskFreeSpaceExW`; if not, add to `Cargo.toml` `[target.'cfg(windows)'.dependencies]`.
4. **`app.restart()`** — confirm the `restart_app` command path (updater.rs
   uses `app.restart()` which diverges; expose via a small IPC command or
   reuse `tauri_plugin_opener`/the existing restart if any).

## 13. Blaze impact (hard rule check)

- Default RU/PTT/Smart path: **untouched.** The picker is settings UI
  (rendered once). The bg download runs on `tauri::async_runtime::spawn`,
  off the inference hot path. Progress events fire ≤4 Hz ONLY during an
  active download (not during dictation).
- Cold-start: unchanged (the startup `ensure_model` for the cached default is
  a fast no-op via the size-check fast path).
- No new deps (serde already present; `windows` already present; hf-hub async
  client is core, no feature needed).
