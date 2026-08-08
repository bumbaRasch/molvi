# Model Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the silent-restart model switch with a 2-card picker that downloads an uncached model in the background (real progress + cancel, default engine never blocks) and activates it on restart.

**Architecture:** `model_store` goes async with a real `ProgressHandler` (hf-hub 1.0); a background `tauri::async_runtime` task streams per-byte progress to a 2-card picker in the Recognition section; download and select are separate actions so `settings.model` always points to a cached model (no silent startup download). Cancel = `JoinHandle::abort` (hf-hub cache resumes).

**Tech Stack:** Rust (tauri 2.11, hf-hub 1.0, windows 0.62), vanilla TypeScript (Vite 8), 36 locale i18n dict.

**Spec:** `docs/superpowers/specs/2026-08-06-model-picker-design.md` (read it — it is the source of truth; this plan implements it).

## Global Constraints

- **No new dependencies.** serde (derive) + hf-hub (async core) + windows are already deps. Task 1 DROPS the now-unused `blocking` hf-hub feature; Task 3 ADDS the `Win32_Storage_FileSystem` windows feature (feature flags, not crates).
- **Backward compat NOT needed** — clean breaks; `settings.json` regenerates via `#[serde(default)]`.
- **Privacy §10.1** — never log transcript/partials/dict/history/audio. Progress events carry only `model_id` + byte counts + % (metadata). Do not log `tracing::*` of file content (there is none).
- **Blaze / no regression** — default RU/PTT/Smart path untouched. The download task runs off the inference hot path; progress events fire ≤4 Hz and only during an active download.
- **Docs-first (MANDATORY)** — re-confirm hf-hub 1.0 API against `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/hf-hub-1.0.0/src/` (progress.rs, repository/download.rs) before Task 1/2. ctx7 ID `/huggingface/hf-hub` if needed (NOTE: ctx7 autodocs are slightly stale — method is `on_progress`, NOT `handle`; the registry source is authoritative).
- **Ponytail FULL** — smallest diff, `// ponytail:` for shortcuts, comments WHY never WHAT.
- **Gates (every task):** `cargo fmt --manifest-path src-tauri/Cargo.toml`; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; `cargo test --manifest-path src-tauri/Cargo.toml --lib`; TS: `npx tsc --noEmit`; `npm run build`. **Binary lock:** if a live `molvi.exe` runs, use `cargo check --all-targets` + `cargo test --lib` (do NOT kill the dev app).
- **i18n set-equality** — every locale's key set must equal `en`'s.

## File Structure

- `src-tauri/src/model_store.rs` — async `ensure_model` + file-size manifest + `model_status` disk check + `ModelProgressEmitter` (ProgressHandler) + disk-space helper. (Tasks 1, 2)
- `src-tauri/src/lib.rs` — `AppState.model_download` handle; update the one `ensure_model` caller (Task 1); spawn wiring (Task 3).
- `src-tauri/src/ipc.rs` — `model_status` / `download_model` / `cancel_model_download` / `restart_app` commands (Task 3).
- `src-tauri/Cargo.toml` — drop `blocking` hf-hub feature (Task 1); add `Win32_Storage_FileSystem` windows feature (Task 3).
- `src-tauri/capabilities/settings.json` — whitelist new commands + event listens (Task 3).
- `src/settings/sections/recognition.ts` — 2-card picker replacing engine Select + event wiring (Task 4).
- `src/settings.css` — `.model-card`, progress bar, badges (Task 4).
- `src/settings/types.ts` — `ModelStatus` TS mirror (Task 4).
- `src/i18n/locales/*.ts` — `models.*` keys (+36), remove `recognition.engine*` (Task 5).

---

## Task 1: model_store — async + file-size manifest + status check

**Files:**
- Modify: `src-tauri/src/model_store.rs`
- Modify: `src-tauri/src/lib.rs:407` (the one `ensure_model` caller)
- Modify: `src-tauri/Cargo.toml:29` (drop `blocking` feature)
- Test: `src-tauri/src/model_store.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub async fn ensure_model(model_id: &str, make_progress: impl Fn(u64) -> Option<hf_hub::progress::Progress>) -> Result<PathBuf>` — the closure receives the byte `offset` of already-completed files and returns a `Progress` for the current file (Task 1's caller passes `|_| None`; Task 3 passes a factory building a `ModelProgressEmitter`); `pub fn model_status() -> Result<Vec<ModelStatus>>`; `#[derive(serde::Serialize)] pub struct ModelStatus { pub model_id: String, pub cached: bool, pub size_bytes: u64 }` (size_bytes = `grand_total`; single source of truth so TS doesn't duplicate the manifest); `pub fn grand_total(model_id: &str) -> u64`; `pub fn has_disk_space(needed: u64) -> Result<bool>` (implemented in Task 3).

- [ ] **Step 1: Fetch the exact pinned-revision file sizes (docs-first, live data)**

Run both (HF tree API at the pinned revisions) and record the `size` field for each file:

```
https://huggingface.co/api/models/istupakov/gigaam-v3-onnx/tree/322c3b29492673eb7d0b434bfa9dfb8653e34d02
https://huggingface.co/api/models/pantinor/nemotron-3.5-asr-streaming-0.6b-onnx/tree/add2e6e84c8c38517457113fa0aaedf8e6df192c
```
(Use `webfetch` on each URL, or `curl` in a shell, or `ureq`.) Read the `size` (bytes) for exactly:
- GigaAM: `v3_e2e_ctc.int8.onnx`, `v3_e2e_ctc_vocab.txt`
- Nemotron: `encoder.onnx`, `encoder.onnx.data`, `decoder_joint.onnx`, `tokenizer.model`

Record the 6 numbers — they go into the manifest in Step 3.

- [ ] **Step 2: Write the failing test for the status/cache check**

Add to `model_store.rs` `#[cfg(test)] mod tests`:
```rust
use std::fs;
use super::*;

#[test]
fn model_status_reports_cached_and_missing() {
    let dir = std::env::temp_dir().join(format!("molvi-status-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(MODEL_GIGAAM_V3_E2E_CTC)).unwrap();
    // GigaAM: write both files with CORRECT sizes → cached
    let gigaam = manifest_for(MODEL_GIGAAM_V3_E2E_CTC).unwrap();
    for &(_, dst, expected) in gigaam {
        fs::write(dir.join(MODEL_GIGAAM_V3_E2E_CTC).join(dst), vec![0u8; expected as usize]).unwrap();
    }
    // Nemotron: nothing → not cached
    let status = cached_status(&dir, MODEL_GIGAAM_V3_E2E_CTC);
    assert!(status, "gigaam fully present with correct sizes = cached");
    let status = cached_status(&dir, MODEL_NEMOTRON_0_6B);
    assert!(!status, "nemotron absent = not cached");

    // wrong-size file → not cached (partial/corrupt detection)
    fs::write(dir.join(MODEL_GIGAAM_V3_E2E_CTC).join(gigaam[0].1), vec![0u8; 10]).unwrap();
    assert!(!cached_status(&dir, MODEL_GIGAAM_V3_E2E_CTC), "wrong-size file = not cached");

    let _ = fs::remove_dir_all(&dir);
}
```
(The test calls `cached_status(base, model_id)` + `manifest_for(model_id)` — pure helpers extracted in Step 4. They take an explicit `base` dir so the test uses a temp dir, never the real models dir.)

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib model_status_reports`
Expected: FAIL — `cached_status` / `manifest_for` not defined.

- [ ] **Step 4: Implement the manifest + helpers + async ensure_model**

Re-confirm hf-hub 1.0 API first (read `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/hf-hub-1.0.0/src/progress.rs` + `repository/download.rs`: `download_file().progress(Option<Progress>)`, `.send().await`).

Change `FILES` / `FILES_NEMO` to 3-tuples with the sizes from Step 1:
```rust
const FILES: &[(&str, &str, u64)] = &[
    ("v3_e2e_ctc.int8.onnx", "model.int8.onnx", <GIGAAM_ONNX_SIZE>),
    ("v3_e2e_ctc_vocab.txt", "vocab.txt", <GIGAAM_VOCAB_SIZE>),
];
const FILES_NEMO: &[(&str, &str, u64)] = &[
    ("encoder.onnx", "encoder.onnx", <NEMO_ENC_SIZE>),
    ("encoder.onnx.data", "encoder.onnx.data", <NEMO_ENC_DATA_SIZE>),
    ("decoder_joint.onnx", "decoder_joint.onnx", <NEMO_DEC_SIZE>),
    ("tokenizer.model", "tokenizer.model", <NEMO_TOK_SIZE>),
];
```
(`// ponytail: pinned-revision-stable sizes; recompute if HF_REVISION* rolls forward.`)

Extract a single source-of-truth match + helpers:
```rust
fn source(model_id: &str) -> Option<(&'static str, &'static str, &'static str, &'static [(&'static str, &'static str, u64)])> {
    match model_id {
        MODEL_GIGAAM_V3_E2E_CTC => Some((HF_OWNER, HF_NAME, HF_REVISION, FILES)),
        MODEL_NEMOTRON_0_6B => Some((HF_OWNER_NEMO, HF_NAME_NEMO, HF_REVISION_NEMO, FILES_NEMO)),
        _ => None,
    }
}
fn manifest_for(model_id: &str) -> Option<&'static [(&'static str, &'static str, u64)]> {
    source(model_id).map(|(_, _, _, f)| f)
}
/// All manifest files present AND byte-exact (catches partial/corrupt).
fn cached_status(base: &std::path::Path, model_id: &str) -> bool {
    match manifest_for(model_id) {
        Some(files) => files.iter().all(|(_, dst, expected)| {
            std::fs::metadata(base.join(model_id).join(dst)).map(|m| m.len() == *expected).unwrap_or(false)
        }),
        None => false,
    }
}
pub fn grand_total(model_id: &str) -> u64 {
    manifest_for(model_id).map(|f| f.iter().map(|(_, _, s)| s).sum()).unwrap_or(0)
}
#[derive(serde::Serialize)]
pub struct ModelStatus { pub model_id: String, pub cached: bool, pub size_bytes: u64 }
pub fn model_status() -> Result<Vec<ModelStatus>> {
    let base = paths::models_dir()?;
    Ok([MODEL_GIGAAM_V3_E2E_CTC, MODEL_NEMOTRON_0_6B].iter().map(|&id| ModelStatus {
        model_id: id.to_string(),
        cached: cached_status(&base, id),
        size_bytes: grand_total(id),
    }).collect())
}
```
Convert `ensure_model` to async + `make_progress` closure + the size-check fast path:
```rust
pub async fn ensure_model(
    model_id: &str,
    make_progress: impl Fn(u64) -> Option<hf_hub::progress::Progress>,
) -> Result<std::path::PathBuf> {
    let (hf_owner, hf_name, hf_revision, files) = source(model_id)
        .ok_or_else(|| MolviError::ModelStore(format!("unknown model id: {model_id}")))?;
    let base = paths::models_dir()?;
    let dir = base.join(model_id);
    std::fs::create_dir_all(&dir).map_err(|e| MolviError::ModelStore(format!("create model dir: {e}")))?;
    if cached_status(&base, model_id) {
        tracing::info!("model {model_id} already cached at {}", dir.display());
        return Ok(dir);
    }
    tracing::info!("downloading model {model_id} from hf:{hf_owner}/{hf_name}");
    let client = hf_hub::HFClientBuilder::new()
        .cache_dir(base.join("_hf"))
        .build()
        .map_err(|e| MolviError::ModelStore(format!("hf client: {e}")))?;
    let repo = client.model(hf_owner, hf_name);
    let mut offset: u64 = 0;
    for &(src, dst, size) in files {
        // Each file gets a Progress built with the cumulative offset of files
        // already completed → the bar shows (offset + this file's bytes) / total.
        let staged = repo.download_file().filename(src).revision(hf_revision)
            .local_dir(dir.clone()).progress(make_progress(offset))
            .send().await
            .map_err(|e| MolviError::ModelStore(format!("download {src}: {e}")))?;
        if src != dst {
            std::fs::rename(&staged, dir.join(dst)).map_err(|e| MolviError::ModelStore(format!("stage {dst}: {e}")))?;
        }
        offset = offset.saturating_add(size);
    }
    tracing::info!("model {model_id} ready at {}", dir.display());
    Ok(dir)
}
```
(The old coarse `progress(0,0)`/`progress(done,done)` calls are gone — Task 2 supplies real progress via the closure; onboarding/startup pass `|_| None`.)

- [ ] **Step 5: Update the one caller (`lib.rs:407`) + drop the `blocking` feature**

`lib.rs:407` is inside an `async` block (the bg thread uses `tauri::async_runtime::spawn`). Change:
```rust
let model_dir = match model_store::ensure_model(&settings.model, |_, _| {}) {
```
to:
```rust
let model_dir = match model_store::ensure_model(&settings.model, |_| None).await {
```
(The enclosing block is already async — confirm by reading the surrounding ~10 lines; the `.await` must land in an `async move {}`.)

`Cargo.toml:29`:
```toml
hf-hub = { version = "1.0", features = ["blocking"] }
```
to:
```toml
hf-hub = { version = "1.0" }
```
(`// ponytail: blocking feature was for build_sync(); async HFClient is core — dropped after the sync→async switch.`)

- [ ] **Step 6: Run tests + gates**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib model_status_reports` → PASS.
Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → clean.
(Binary lock fallback: `cargo check --all-targets` + `cargo test --lib`.)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/model_store.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "refactor(model_store): async ensure_model + file-size manifest + status check"
```

---

## Task 2: ModelProgressEmitter (ProgressHandler) + ≤4 Hz throttle

**Files:**
- Modify: `src-tauri/src/model_store.rs` (add `ModelProgressEmitter`)
- Test: `src-tauri/src/model_store.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `pub struct ModelProgressEmitter { ... }` implementing `hf_hub::progress::ProgressHandler`; constructed per-file with an `offset` (bytes of already-completed files) + `total` (grand total). `fn should_emit(last_ms: u64, now_ms: u64) -> bool` (pure, the throttle decision — tested).
- Consumes: Task 1's `grand_total` + manifest sizes.

- [ ] **Step 1: Write the failing test for the throttle decision (pure)**

```rust
#[test]
fn throttle_emits_at_most_every_250ms() {
    // should_emit(last, now) = now - last >= 250
    assert!(!should_emit(1000, 1100), "100ms since last → no emit");
    assert!(!should_emit(1000, 1249), "249ms → no emit");
    assert!(should_emit(1000, 1250), "250ms → emit");
    assert!(should_emit(1000, 2000), "1000ms → emit");
    // saturating: last in the future (clock skew) → no emit
    assert!(!should_emit(2000, 1000));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib throttle_emits`
Expected: FAIL — `should_emit` not defined.

- [ ] **Step 3: Implement `ModelProgressEmitter` + `should_emit`**

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use hf_hub::progress::{DownloadEvent, ProgressEvent, ProgressHandler};

const EMIT_INTERVAL_MS: u64 = 250;

/// Throttle decision — pure, unit-tested.
fn should_emit(last_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_ms) >= EMIT_INTERVAL_MS
}

/// Per-file progress emitter. molvi downloads manifest files sequentially, so
/// each emitter is constructed for ONE file with the byte `offset` of all
/// previously-completed files; the bar shows (offset + this file's cumulative
/// bytes) / grand `total`. Created in Task 3's download loop.
pub struct ModelProgressEmitter {
    app: AppHandle,
    model_id: String,
    total: u64,
    offset: u64,
    last_emit_ms: AtomicU64,
}

impl ModelProgressEmitter {
    pub fn new(app: AppHandle, model_id: &str, total: u64, offset: u64) -> Self {
        Self { app, model_id: model_id.to_string(), total, offset, last_emit_ms: AtomicU64::new(0) }
    }
    fn now_ms() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
    }
    fn emit(&self, bytes: u64) {
        // hf-hub contract: on_progress must NOT block — atomics + one emit only.
        let now = Self::now_ms();
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if should_emit(last, now) {
            self.last_emit_ms.store(now, Ordering::Relaxed);
            let pct = if self.total > 0 { (bytes * 100) / self.total } else { 0 };
            let _ = self.app.emit("model-download-progress",
                serde_json::json!({ "model": self.model_id, "bytes": bytes, "total": self.total, "pct": pct }));
        }
    }
}

impl ProgressHandler for ModelProgressEmitter {
    fn on_progress(&self, event: &ProgressEvent) {
        // Single active file per download_file() call → files.last() is it.
        if let ProgressEvent::Download(DownloadEvent::Progress { files }) = event {
            if let Some(f) = files.last() {
                self.emit(self.offset.saturating_add(f.bytes_completed));
            }
        }
    }
}
```
(`serde_json` is already a dep. `tauri::Emitter` is the 2.x emit trait. Privacy: payload is model_id + bytes/total/pct only.)

- [ ] **Step 4: Run tests + gates**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib throttle_emits` → PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/model_store.rs
git commit -m "feat(model_store): ProgressHandler emitter with 4Hz throttle"
```

---

## Task 3: AppState download handle + IPC commands + cancel + disk-space

**Files:**
- Modify: `src-tauri/src/model_store.rs` (add `has_disk_space`)
- Modify: `src-tauri/src/lib.rs` (`AppState.model_download`; register commands in `invoke_handler`)
- Modify: `src-tauri/src/ipc.rs` (the 4 commands)
- Modify: `src-tauri/Cargo.toml:45` (add `Win32_Storage_FileSystem` windows feature)
- Modify: `src-tauri/capabilities/settings.json` (whitelist)
- Test: `src-tauri/src/model_store.rs` (`has_disk_space`)

**Interfaces:**
- Produces (ipc.rs, all `#[tauri::command]`): `async fn model_status() -> Result<Vec<model_store::ModelStatus>, MolviError>`; `async fn download_model(model_id: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), MolviError>`; `fn cancel_model_download(state: State<'_, AppState>) -> Result<(), MolviError>`; `fn restart_app(app: AppHandle)`.
- Consumes: Task 1 `model_status`/`grand_total`/`ensure_model`; Task 2 `ModelProgressEmitter`.

- [ ] **Step 1: Write the failing test for `has_disk_space`**

```rust
#[test]
fn has_disk_space_is_sane() {
    // 0 needed → always true (nothing to download)
    assert!(has_disk_space(0).unwrap(), "0 bytes needed → true");
    // a huge number (u64::MAX) → false on any real disk
    assert!(!has_disk_space(u64::MAX).unwrap(), "u64::MAX needed → false");
    // a small number (1 KB) → true on the system drive
    assert!(has_disk_space(1024).unwrap(), "1 KB needed → true on system drive");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib has_disk_space`
Expected: FAIL — `has_disk_space` not defined.

- [ ] **Step 3: Implement `has_disk_space` (Windows GetDiskFreeSpaceExW)**

Verify the `windows` crate API first (docs.rs `windows` 0.62 `Win32_Storage_FileSystem::GetDiskFreeSpaceExW`). Add the feature to `Cargo.toml:45`:
```toml
windows = { version = "0.62", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation", "Win32_System_Threading", "Win32_System_SystemInformation", "Win32_Media_Audio", "Win32_Storage_FileSystem"] }
```
In `model_store.rs`:
```rust
#[cfg(target_os = "windows")]
pub fn has_disk_space(needed: u64) -> Result<bool> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let dir = paths::models_dir()?;
    let mut free: u64 = 0;
    let mut avail: std::mem::MaybeUninit<u64> = std::mem::MaybeUninit::uninit();
    let mut total: std::mem::MaybeUninit<u64> = std::mem::MaybeUninit::uninit();
    // ponytail: PCWSTR from a wide-encoded path; models_dir always exists (created earlier).
    let wide: Vec<u16> = dir.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let pcw = windows::core::PCWSTR(wide.as_ptr());
    let r = unsafe { GetDiskFreeSpaceExW(pcw, Some(avail.as_mut_ptr()), Some(total.as_mut_ptr()), Some(&mut free)) };
    r.map_err(|e| MolviError::ModelStore(format!("disk free space: {e}")))?;
    // avail (caller's free) is what matters; fall back to total-free if avail reads 0.
    let avail = unsafe { avail.assume_init() };
    Ok(avail >= needed)
}
#[cfg(not(target_os = "windows"))]
pub fn has_disk_space(_needed: u64) -> Result<bool> { Ok(true) }
```
(Verify the exact `GetDiskFreeSpaceExW` signature against docs.rs before finalizing — the 4-pointer form: `directory, free_bytes_available_to_caller, total_bytes, total_free_bytes`. The implementer re-confirms pointer/Option shapes vs the 0.62 binding.)

- [ ] **Step 4: Run the test + gates**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib has_disk_space` → PASS.
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` → clean.

- [ ] **Step 5: Add `AppState.model_download` + the 4 IPC commands**

`lib.rs` AppState struct — add:
```rust
pub model_download: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
```
and in the `AppState::new`/`Default`/wherever it's constructed, initialize `model_download: std::sync::Mutex::new(None)` (mirror how `cmd_tx` etc. are initialized — read the existing AppState construction).

`ipc.rs` — add:
```rust
use tauri::{AppHandle, Emitter, State};
use crate::model_store;
use crate::AppState;
use crate::errors::{MolviError, Result};

#[tauri::command]
pub async fn model_status() -> Result<Vec<model_store::ModelStatus>> {
    model_store::model_status()
}

#[tauri::command]
pub async fn download_model(model_id: String, app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    // Guard: one download at a time.
    {
        let guard = state.model_download.lock().unwrap();
        if let Some(h) = &*guard {
            if !h.is_finished() {
                return Err(MolviError::ModelStore("download already in progress".into()));
            }
        }
    }
    // Guard: no-op if already cached.
    if model_store::model_status()?.iter().find(|m| m.model_id == model_id).map(|m| m.cached).unwrap_or(false) {
        return Ok(());
    }
    // Disk-space pre-check.
    let total = model_store::grand_total(&model_id);
    if !model_store::has_disk_space(total)? {
        return Err(MolviError::ModelStore(format!("insufficient disk space: need {total} bytes")));
    }
    let app2 = app.clone();
    let id = model_id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        // Per-file Progress factory: each file's emitter carries the byte offset
        // of files already completed (Task 1's ensure_model passes `offset` in).
        let total = model_store::grand_total(&id);
        let result = model_store::ensure_model(&id, |offset| {
            Some(hf_hub::progress::Progress::new(
                model_store::ModelProgressEmitter::new(app2.clone(), &id, total, offset),
            ))
        }).await;
        match result {
            Ok(_) => { let _ = app2.emit("model-download-complete", &id); }
            Err(e) => { tracing::warn!("model download failed: {e}"); let _ = app2.emit("model-download-failed", &id); }
        }
    });
    state.model_download.lock().unwrap().replace(handle);
    Ok(())
}

#[tauri::command]
pub fn cancel_model_download(state: State<'_, AppState>) -> Result<()> {
    if let Some(h) = state.model_download.lock().unwrap().take() {
        h.abort(); // hf-hub content cache resumes completed chunks on next attempt
    }
    Ok(())
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart(); // diverges (!) — coerces to ()
}
```
(`ModelProgressEmitter::new` and `grand_total` are `pub` in `model_store.rs` from Tasks 1/2 — the spawn block constructs the per-file emitter via the closure Task 1's `ensure_model` calls. Privacy: emits carry only model_id + bytes/total/pct.)

- [ ] **Step 6: Register commands + whitelist capability**

`lib.rs` `invoke_handler` — add (next to existing `crate::ipc::*`):
```rust
crate::ipc::model_status,
crate::ipc::download_model,
crate::ipc::cancel_model_download,
crate::ipc::restart_app,
```
`src-tauri/capabilities/settings.json` — add to the permissions array (mirror existing entries):
```json
"core:event:allow-listen",
```
(is likely already present from the audit fix; the events `model-download-progress/complete/failed` are emitted globally and listened via `@tauri-apps/api/event` `listen()` — confirm `core:event:allow-listen` + `core:event:allow-unlisten` are in the settings capability; if the event names need explicit allow-listing in your Tauri config, add them, else `allow-listen` suffices.)

- [ ] **Step 7: Run gates**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml`; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; `cargo test --manifest-path src-tauri/Cargo.toml --lib` → all green.
(Binary lock fallback: `cargo check --all-targets` + `cargo test --lib`.)

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/model_store.rs src-tauri/src/lib.rs src-tauri/src/ipc.rs src-tauri/Cargo.toml src-tauri/capabilities/settings.json
git commit -m "feat(ipc): model download/status/cancel/restart commands + disk-space guard"
```

---

## Task 4: Frontend — 2-card picker replacing the engine Select

**Files:**
- Modify: `src/settings/sections/recognition.ts` (replace engine Select; add cards + event wiring)
- Modify: `src/settings.css` (`.model-card`, progress bar, badges)
- Modify: `src/settings/types.ts` (`ModelStatus` mirror)
- Test: `npx tsc --noEmit`; `npm run build` (no JS test runner — TS correctness = tsc + build + human GUI smoke)

**Interfaces:**
- Consumes (Rust IPC, Task 3): `invoke<ModelStatus[]>("model_status")`; `invoke("download_model", {modelId})`; `invoke("cancel_model_download")`; `invoke("restart_app")`; events `model-download-progress {model,bytes,total,pct}` / `model-download-complete {model}` / `model-download-failed {model}`.
- NOTE: Tauri IPC uses camelCase args — pass `{modelId: "..."}` (Rust `model_id` ↔ TS `modelId`).

- [ ] **Step 1: Add the `ModelStatus` TS mirror (`types.ts`)**

```ts
export interface ModelStatus { model_id: string; cached: boolean; size_bytes: number }
```
(Strict field-for-field mirror of the Rust struct. Keep snake_case to match the wire format.)

- [ ] **Step 2: Replace the engine Select with the 2-card picker (`recognition.ts`)**

Remove the `engine` Select block (`recognition.ts:165-176`). Replace with a picker builder. Sketch (fill in i18n keys from Task 5; `t("models.*")` renders the raw key until Task 5 lands — tsc still passes since `t()` takes a string):

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
// (add to existing imports)

const MODELS = [
  { id: "gigaam-v3-e2e-ctc", name: "GigaAM", desc: "models.gigaam_desc" },
  { id: NEMOTRON_ID, name: "Nemotron", desc: "models.nemotron_desc" },
] as const;

// initialModel = loaded engine (page-open snapshot) — reuse existing var.
// status fetched on render:
let statuses: Record<string, ModelStatus> = {}; // model_id -> status
void invoke<ModelStatus[]>("model_status").then((s) => {
  statuses = Object.fromEntries(s.map((m) => [m.model_id, m]));
  renderPicker();
});

const pickerHost = document.createElement("div");
pickerHost.className = "model-picker";

function renderPicker(downloading?: { id: string; pct: number; bytes: number; total: number }): void {
  pickerHost.replaceChildren();
  for (const m of MODELS) {
    const active = s.model === m.id;
    const selected = active; // selected == settings.model (restart pending if != initialModel)
    const restartPending = active && m.id !== initialModel;
    const cached = statuses[m.id]?.cached ?? false;
    const dl = downloading && downloading.id === m.id ? downloading : undefined;

    const card = document.createElement("div");
    card.className = "model-card" + (active ? " selected" : "");
    const head = document.createElement("div");
    head.className = "model-card-head";
    const nameEl = document.createElement("span");
    nameEl.className = "model-name";
    nameEl.textContent = m.name;
    head.append(nameEl);

    const status = document.createElement("div");
    status.className = "model-card-status";
    if (dl) {
      const bar = document.createElement("div"); bar.className = "progress-bar";
      const fill = document.createElement("div"); fill.className = "progress-fill";
      fill.style.width = `${dl.pct}%`; bar.append(fill);
      const pctTxt = document.createElement("span"); pctTxt.className = "progress-text";
      pctTxt.textContent = t("models.downloading").replace("{bytes}", fmtBytes(dl.bytes)).replace("{total}", fmtBytes(dl.total)).replace("{pct}", String(dl.pct));
      const cancel = Button(t("models.cancel"), () => void invoke("cancel_model_download"));
      status.append(bar, pctTxt, cancel);
    } else if (restartPending) {
      const note = document.createElement("span"); note.textContent = t("models.restart_to_activate");
      const restart = Button(t("models.restart_btn"), () => void invoke("restart_app"));
      restart.classList.add("primary");
      status.append(note, restart);
    } else if (cached) {
      if (active) { const a = document.createElement("span"); a.className = "badge-active"; a.textContent = "✓ " + t("models.active"); status.append(a); }
      else { card.classList.add("clickable"); card.addEventListener("click", () => { patch((n) => { n.model = m.id; }); sync(m.id, langSel.get()); renderPicker(); }); }
    } else {
      const dl2 = Button(t("models.download").replace("{size}", fmtBytes(statuses[m.id]?.size_bytes ?? 0)), () => void invoke("download_model", { modelId: m.id }));
      status.append(dl2);
    }
    head.append(status);
    const desc = document.createElement("div"); desc.className = "model-desc"; desc.textContent = t(m.desc);
    card.append(head, desc);
    pickerHost.append(card);
  }
}
// helpers: fmtBytes (bytes → human "1.2 ГБ"/"80 МБ" — localize unit). (size comes
// from ModelStatus.size_bytes over the wire — no TS-side size constant needed.)
renderPicker();

// Event wiring:
const unlistens: (() => void)[] = [];
void listen<{ model: string; bytes: number; total: number; pct: number }>("model-download-progress", (e) => {
  renderPicker({ id: e.payload.model, pct: e.payload.pct, bytes: e.payload.bytes, total: e.payload.total });
});
void listen<{ model: string }>("model-download-complete", () => {
  void invoke<ModelStatus[]>("model_status").then((s) => { statuses = Object.fromEntries(s.map((m) => [m.model_id, m])); });
  toast("success", t("models.download_complete"));
  renderPicker();
});
void listen<{ model: string }>("model-download-failed", () => { toast("error", t("models.download_failed")); renderPicker(); });

// Cleanup: unlisten the 3 event listeners on section teardown. recognition.ts
// currently returns `{ el: root }`; change the return to include cleanup (mirrors
// hotkey.ts which returns { el, cleanup }):
//   const unlistens: Array<() => void> = [];
//   unlistens.push(await listen("model-download-progress", ...));  // (push each)
//   return { el: root, cleanup: () => unlistens.forEach((u) => u()) };
// (`listen` is async (returns Promise<UnlistenFn>); await each before pushing,
// or push the Promise resolution — see how other sections handle it. If the
// SectionBuilder return type has no `cleanup`, check `types.ts` — hotkey.ts
// already uses it, so the type supports it.)
```
Update the `group` append: replace `engine.wrap` with `pickerHost` in the `SettingsGroup(...)` children list (position 2).

- [ ] **Step 3: Add CSS (`settings.css`)**

```css
.model-picker { display: flex; flex-direction: column; gap: 8px; }
.model-card {
  border: 1px solid var(--border);
  border-radius: var(--radius-card);
  padding: 12px;
  background: var(--bg);
}
.model-card.selected { border-color: var(--accent); }
.model-card.clickable { cursor: pointer; }
.model-card.clickable:hover { border-color: var(--accent); }
.model-card-head { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
.model-name { font-weight: 600; }
.model-card-status { display: flex; align-items: center; gap: 8px; }
.model-desc { color: var(--muted); font-size: 13px; margin-block-start: 4px; }
.badge-active { color: var(--accent); font-weight: 500; }
.progress-bar { flex: 1; min-inline-size: 120px; height: 6px; background: var(--border); border-radius: 3px; overflow: hidden; }
.progress-fill { height: 100%; background: var(--accent); transition: width 250ms ease; }
.progress-text { font-size: 12px; color: var(--muted); white-space: nowrap; }
```
(All logical properties; `--accent` = `#0E7C86` AA-safe; transition on `.progress-fill` smooths the ≤4Hz steps.)

- [ ] **Step 4: Run TS gates**

Run: `npx tsc --noEmit` → clean. `npm run build` → clean.

- [ ] **Step 5: Commit**

```bash
git add src/settings/sections/recognition.ts src/settings.css src/settings/types.ts
git commit -m "feat(recognition): 2-card model picker with live download progress"
```

---

## Task 5: i18n — add `models.*` keys × 36, remove `recognition.engine*`

**Files:**
- Modify: all 36 `src/i18n/locales/*.ts`

**Interfaces:** none (string values only).

- [ ] **Step 1: Add the canonical EN keys (`en.ts`)**

In the `updates.*` cluster area (alphabetical; place a `models.*` block before `nav.*` or after `history.*`):
```ts
"models.active": "Active",
"models.cancel": "Cancel",
"models.download": "Download ({size})",
"models.download_complete": "Model ready — select it and restart.",
"models.download_failed": "Download failed",
"models.downloading": "{bytes} / {total} — {pct}%",
"models.gigaam_desc": "Russian — fast, with punctuation.",
"models.insufficient_space": "Not enough disk space (need {need}, {free} free).",
"models.nemotron_desc": "Multilingual — 40 languages.",
"models.restart_btn": "Restart",
"models.restart_to_activate": "Restart to activate",
"models.retry": "Retry",
```

- [ ] **Step 2: Remove the now-unused `recognition.engine*` keys from `en.ts`**

Delete:
```ts
"recognition.engine",
"recognition.engine_gigaam",
"recognition.engine_nemotron",
```
(engine Select is gone — clean break.)

- [ ] **Step 3: Replicate both changes across all 35 other locale files**

For each of the 36 locales: add the 12 `models.*` keys (translate the MEANING; "GigaAM"/"Nemotron" stay verbatim; `{size}`/`{bytes}`/`{total}`/`{pct}`/`{need}`/`{free}` tokens stay verbatim; size unit localizes — ГБ/GB/MB/etc.), and remove the 3 `recognition.engine*` keys. Mirror the RU/Slavic formal-pronoun convention where applicable.

- [ ] **Step 4: Verify i18n set-equality**

Run (PowerShell):
```powershell
$en = (Select-String -Path src\i18n\locales\en.ts -Pattern '"[^"]+":').Count
Get-ChildItem src\i18n\locales\*.ts | ForEach-Object { $c = (Select-String -Path $_.FullName -Pattern '"[^"]+":').Count; if ($c -ne $en) { "$($_.BaseName): $c (en=$en) MISMATCH" } }
```
Expected: NO mismatches (every locale == en). Also `Select-String -Path src\i18n\locales\*.ts -Pattern 'models\.download' | Measure-Object | Select -ExpandProperty Count` == 36.

- [ ] **Step 5: Run gates + commit**

Run: `npx tsc --noEmit`; `npm run build` → clean.
```bash
git add src/i18n/locales/*.ts
git commit -m "i18n: add models.* keys (x36) + remove obsolete recognition.engine* (x36)"
```

---

## Human smoke (after all 5 tasks; NOT code gaps)

1. Settings → Recognition: two model cards render; active GigaAM shows ✓ "Активна".
2. Nemotron card → "Скачать (2.6 ГБ)" → click → progress bar fills (real %), GigaAM still dictating (background).
3. Cancel mid-download → card back to "Скачать"; re-click resumes (hf-hub cache — % jumps to prior progress).
4. Complete → toast "Model ready" → Nemotron card now "ready" (radio). Click → "Перезапустите для переключения" + "Перезапустить". Click Restart → app restarts → Nemotron active.
5. Switch back to GigaAM (cached) → restart → GigaAM active.
6. `cargo test --test log_privacy` after closing the dev app (binary-locked during dev).
