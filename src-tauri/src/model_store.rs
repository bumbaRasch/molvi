//! First-run model download + cache (spec §6.9).
//! Layout on disk (what `GigaAMModel::load` expects):
//!   %APPDATA%\com.molvi.app\models\gigaam-v3-e2e-ctc\
//!     model.int8.onnx   (renamed from v3_e2e_ctc.int8.onnx)
//!     vocab.txt         (renamed from v3_e2e_ctc_vocab.txt)

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hf_hub::progress::{DownloadEvent, ProgressEvent, ProgressHandler};
use tauri::{AppHandle, Emitter};

use crate::errors::{MolviError, Result};
use crate::paths;

pub const MODEL_GIGAAM_V3_E2E_CTC: &str = "gigaam-v3-e2e-ctc";

/// Nemotron-3.5-ASR-0.6B (multilingual RNN-T). Opt-in via `settings.model`;
/// the ~2.6GB download runs only when the user picks it + restarts.
pub const MODEL_NEMOTRON_0_6B: &str = "nemotron-3.5-asr-streaming-0.6b";

const HF_OWNER: &str = "istupakov";
const HF_NAME: &str = "gigaam-v3-onnx";

/// Pinned HF revision (main-branch commit SHA, 2026-02-18). Pinning prevents a
/// transparent swap to a malicious/compromised upstream commit on the next
/// first-run download (supply-chain hardening, spec §10.3). To roll forward,
/// query `https://huggingface.co/api/models/istupakov/gigaam-v3-onnx/revision/main`
/// and update this SHA + date.
const HF_REVISION: &str = "322c3b29492673eb7d0b434bfa9dfb8653e34d02";

/// Files we need from the GigaAM repo, mapped to their on-disk names + the
/// byte size the pinned revision expects. The size turns the old existence-only
/// cache check into a byte-exact one (catches partial/corrupt downloads). The
/// int8 e2e_ctc graph + its vocab. (Other variants exist in the repo but we
/// download only these.)
// ponytail: pinned-revision-stable sizes; recompute if HF_REVISION* rolls forward.
const FILES: &[(&str, &str, u64)] = &[
    ("v3_e2e_ctc.int8.onnx", "model.int8.onnx", 224_893_347),
    ("v3_e2e_ctc_vocab.txt", "vocab.txt", 2_007),
];

// Nemotron-3.5-ASR-0.6B ONNX port. parakeet-rs auto-detects the layout, so files
// keep their repo names (no rename → the stage `rename` below is a no-op skip).
// ponytail: ~2.6GB on first run (encoder.onnx.data weights). License: OpenMDW-1.1
// (nvidia cardData `license_name`; no LICENSE file in the pantinor repo —
// canonical text at openmdw.ai).
const HF_OWNER_NEMO: &str = "pantinor";
const HF_NAME_NEMO: &str = "nemotron-3.5-asr-streaming-0.6b-onnx";
/// Pinned HF revision (main-branch commit SHA, 2026-06-06).
const HF_REVISION_NEMO: &str = "add2e6e84c8c38517457113fa0aaedf8e6df192c";
const FILES_NEMO: &[(&str, &str, u64)] = &[
    ("encoder.onnx", "encoder.onnx", 42_164_972),
    ("encoder.onnx.data", "encoder.onnx.data", 2_454_405_120),
    ("decoder_joint.onnx", "decoder_joint.onnx", 97_590_054),
    ("tokenizer.model", "tokenizer.model", 406_554),
];

/// A model's file manifest: (repo_name, on_disk_name, expected_bytes).
type Manifest = &'static [(&'static str, &'static str, u64)];

/// Resolve a model id to its HF source (owner, name, pinned revision) and file
/// manifest. Single source of truth shared by the download path and the cache
/// check, so the two can never disagree on what "complete" means.
#[allow(clippy::type_complexity)] // owner/name/revision + manifest; a struct would be more code for one caller
fn source(model_id: &str) -> Option<(&'static str, &'static str, &'static str, Manifest)> {
    match model_id {
        MODEL_GIGAAM_V3_E2E_CTC => Some((HF_OWNER, HF_NAME, HF_REVISION, FILES)),
        MODEL_NEMOTRON_0_6B => Some((HF_OWNER_NEMO, HF_NAME_NEMO, HF_REVISION_NEMO, FILES_NEMO)),
        _ => None,
    }
}

fn manifest_for(model_id: &str) -> Option<Manifest> {
    source(model_id).map(|(_, _, _, f)| f)
}

/// All manifest files present AND byte-exact (catches partial/corrupt). Takes
/// an explicit `base` dir so the test can point at a temp dir, never the real
/// models dir.
fn cached_status(base: &std::path::Path, model_id: &str) -> bool {
    match manifest_for(model_id) {
        Some(files) => files.iter().all(|(_, dst, expected)| {
            std::fs::metadata(base.join(model_id).join(dst))
                .map(|m| m.len() == *expected)
                .unwrap_or(false)
        }),
        None => false,
    }
}

/// Total bytes the model occupies on disk when fully downloaded (sum of the
/// manifest sizes). Used for the UI's "X MB" display + the download progress
/// bar's denominator.
pub fn grand_total(model_id: &str) -> u64 {
    manifest_for(model_id)
        .map(|f| f.iter().map(|(_, _, s)| s).sum())
        .unwrap_or(0)
}

/// Min wall-clock gap (ms) between two `model-download-progress` emits (= ≤4 Hz).
const EMIT_INTERVAL_MS: u64 = 250;

/// Throttle decision — pure, unit-tested. `now - last >= EMIT_INTERVAL_MS`,
/// saturating on clock skew (last in the future → no emit).
fn should_emit(last_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_ms) >= EMIT_INTERVAL_MS
}

/// Per-file progress emitter. molvi downloads manifest files sequentially, so
/// each emitter is built for ONE file carrying the byte `offset` of all
/// previously-completed files; the bar shows (offset + this file's cumulative
/// bytes) / grand `total`. Built in `ipc::download_model`'s per-file closure.
pub struct ModelProgressEmitter {
    app: AppHandle,
    model_id: String,
    total: u64,
    offset: u64,
    last_emit_ms: AtomicU64,
}

impl ModelProgressEmitter {
    pub fn new(app: AppHandle, model_id: &str, total: u64, offset: u64) -> Self {
        Self {
            app,
            model_id: model_id.to_string(),
            total,
            offset,
            last_emit_ms: AtomicU64::new(0),
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn emit(&self, bytes: u64) {
        // hf-hub contract: on_progress must NOT block — atomics + one emit only.
        // last_emit_ms starts at 0 → first event always emits immediately.
        let now = Self::now_ms();
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if should_emit(last, now) {
            self.last_emit_ms.store(now, Ordering::Relaxed);
            // bytes ≤ total by contract → pct ∈ 0..=100; checked_div guards
            // div-by-zero; .min(100) clamps a HF re-pack overshoot so the bar
            // never exceeds full.
            let pct = (bytes * 100).checked_div(self.total).unwrap_or(0).min(100);
            let _ = self.app.emit(
                "model-download-progress",
                serde_json::json!({ "model": self.model_id, "bytes": bytes, "total": self.total, "pct": pct }),
            );
        }
    }
}

impl ProgressHandler for ModelProgressEmitter {
    fn on_progress(&self, event: &ProgressEvent) {
        // Two progress channels (hf-hub 1.0, progress.rs:268-300):
        //  • `Progress { files }` — per-file delta; `files.last()` is the single
        //    active file in a per-`download_file()` call.
        //  • `AggregateProgress { bytes_completed }` — xet-batch cumulative bytes
        //    (~10Hz), reported with no per-file breakdown. Large LFS weights
        //    (e.g. the 2.4GB encoder.onnx.data) are typically xet-backed and
        //    surface ONLY through this variant — handling it keeps the bar moving
        //    instead of freezing for the whole multi-GB transfer. `bytes_completed`
        //    is cumulative for the in-flight batch (= the one file molvi downloads
        //    per call), so `offset + bytes_completed` is the grand cumulative.
        match event {
            ProgressEvent::Download(DownloadEvent::Progress { files }) => {
                if let Some(f) = files.last() {
                    self.emit(self.offset.saturating_add(f.bytes_completed));
                }
            }
            ProgressEvent::Download(DownloadEvent::AggregateProgress {
                bytes_completed, ..
            }) => {
                self.emit(self.offset.saturating_add(*bytes_completed));
            }
            _ => {}
        }
    }
}

/// Disk-state snapshot for the Model Picker UI: is each known model fully
/// cached (byte-exact) and how large is it.
#[derive(serde::Serialize)]
pub struct ModelStatus {
    pub model_id: String,
    pub cached: bool,
    pub size_bytes: u64,
}

pub fn model_status() -> Result<Vec<ModelStatus>> {
    let base = paths::models_dir()?;
    Ok([MODEL_GIGAAM_V3_E2E_CTC, MODEL_NEMOTRON_0_6B]
        .iter()
        .map(|&id| ModelStatus {
            model_id: id.to_string(),
            cached: cached_status(&base, id),
            size_bytes: grand_total(id),
        })
        .collect())
}

/// Free-disk-space pre-check before a multi-GB download. Returns true if at
/// least `needed` bytes are available to the caller on the models-dir volume
/// (quota-aware on Windows via `GetDiskFreeSpaceExW`'s caller-free figure). The
/// download path calls this to fail fast instead of running ~2.6GB into ENOSPC.
/// Privacy §10.1: a byte count, no content.
#[cfg(target_os = "windows")]
pub fn has_disk_space(needed: u64) -> Result<bool> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;
    let dir = paths::models_dir()?;
    // ponytail: wide-encode the path (NUL-terminated); models_dir is canonical.
    let wide: Vec<u16> = dir
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let pcw = PCWSTR(wide.as_ptr());
    let mut avail: u64 = 0;
    let mut total: u64 = 0;
    let mut free: u64 = 0;
    let r = unsafe {
        GetDiskFreeSpaceExW(
            pcw,
            Some(&mut avail as *mut u64),
            Some(&mut total as *mut u64),
            Some(&mut free as *mut u64),
        )
    };
    r.map_err(|e| MolviError::ModelStore(format!("disk free space: {e}")))?;
    // `avail` = free bytes available to the caller (the quota-aware figure); that's
    // what a new write consumes against. (`total`/`free` ignored.)
    Ok(avail >= needed)
}

/// Fail-open stub for non-Windows (Step 0). Phase 2 adds `statfs` (macOS),
/// Phase 3 `statvfs` (Linux); until then assume enough space (the download
/// itself fails cleanly on ENOSPC via hf-hub's io error). Privacy §10.1: a
/// byte count, no content.
#[cfg(not(target_os = "windows"))]
pub fn has_disk_space(_needed: u64) -> Result<bool> {
    Ok(true)
}

/// Ensure the model is present on disk (download if missing), returning the
/// model directory. `make_progress(cumulative_offset)` is called per file with
/// the bytes already completed by prior files in the manifest; it returns an
/// `Option<Progress>` the hf-hub async client drives (None = no progress
/// reporting, e.g. the cold-start path that only signals start/complete via
/// `engine-ready`/`engine-error`).
///
/// NOTE: `.local_dir` mode re-creates each file from scratch on failure (no
/// HTTP Range resume — hf-hub's resume lives on the cache path, not local_dir).
/// A dropped connection on the 2.4GB Nemotron weights restarts that file. The
/// byte-exact cache check rejects any partial, so the next run re-downloads
/// cleanly; correctness is never at risk, only bandwidth on a flaky link.
pub async fn ensure_model(
    model_id: &str,
    make_progress: impl Fn(u64) -> Option<hf_hub::progress::Progress>,
) -> Result<PathBuf> {
    let (hf_owner, hf_name, hf_revision, files) = source(model_id)
        .ok_or_else(|| MolviError::ModelStore(format!("unknown model id: {model_id}")))?;

    let base = paths::models_dir()?;
    let dir = base.join(model_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::ModelStore(format!("create model dir: {e}")))?;

    // Fast path: byte-exact check (catches partial/corrupt, not just absence).
    if cached_status(&base, model_id) {
        tracing::info!(
            "model {model_id} already cached at {}",
            crate::paths::redact_appdata(&dir)
        );
        return Ok(dir);
    }

    tracing::info!("downloading model {model_id} from hf:{hf_owner}/{hf_name}");
    // `.local_dir` is the download destination; the client's own cache_dir is
    // not consulted in local-dir mode, so we don't set one (default suffices).
    let client = hf_hub::HFClientBuilder::new()
        .build()
        .map_err(|e| MolviError::ModelStore(format!("hf client: {e}")))?;
    let repo = client.model(hf_owner, hf_name);

    let mut offset: u64 = 0;
    for &(src, dst, size) in files {
        // Each file's Progress is built with the cumulative offset of files
        // already completed -> the bar shows (offset + this file's bytes) /
        // grand total, so a multi-file model (Nemotron) advances across files.
        // maybe_progress (bon) accepts the Option<Progress> from make_progress:
        // None leaves the field unset (no handler), Some drives per-byte events.
        let staged = repo
            .download_file()
            .filename(src)
            .revision(hf_revision)
            .local_dir(dir.clone())
            .maybe_progress(make_progress(offset))
            .send()
            .await
            .map_err(|e| MolviError::ModelStore(format!("download {src}: {e}")))?;
        // Rename only when the on-disk name differs (GigaAM renames; Nemotron
        // keeps repo names → file is already at its target path).
        if src != dst {
            std::fs::rename(&staged, dir.join(dst))
                .map_err(|e| MolviError::ModelStore(format!("stage {dst}: {e}")))?;
        }
        offset = offset.saturating_add(size);
    }

    tracing::info!(
        "model {model_id} ready at {}",
        crate::paths::redact_appdata(&dir)
    );
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn model_status_reports_cached_and_missing() {
        let dir = std::env::temp_dir().join(format!("molvi-status-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(MODEL_GIGAAM_V3_E2E_CTC)).unwrap();
        // GigaAM: write both files with CORRECT sizes -> cached
        let gigaam = manifest_for(MODEL_GIGAAM_V3_E2E_CTC).unwrap();
        for &(_, dst, expected) in gigaam {
            fs::write(
                dir.join(MODEL_GIGAAM_V3_E2E_CTC).join(dst),
                vec![0u8; expected as usize],
            )
            .unwrap();
        }
        // Nemotron: nothing -> not cached
        let status = cached_status(&dir, MODEL_GIGAAM_V3_E2E_CTC);
        assert!(status, "gigaam fully present with correct sizes = cached");
        let status = cached_status(&dir, MODEL_NEMOTRON_0_6B);
        assert!(!status, "nemotron absent = not cached");

        // wrong-size file -> not cached (partial/corrupt detection)
        fs::write(
            dir.join(MODEL_GIGAAM_V3_E2E_CTC).join(gigaam[0].1),
            vec![0u8; 10],
        )
        .unwrap();
        assert!(
            !cached_status(&dir, MODEL_GIGAAM_V3_E2E_CTC),
            "wrong-size file = not cached"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn has_disk_space_is_sane() {
        // 0 needed -> always true (nothing to download)
        assert!(has_disk_space(0).unwrap(), "0 bytes needed -> true");
        // a huge number (u64::MAX) -> false on any real disk
        assert!(
            !has_disk_space(u64::MAX).unwrap(),
            "u64::MAX needed -> false"
        );
        // a small number (1 KB) -> true on the system drive
        assert!(
            has_disk_space(1024).unwrap(),
            "1 KB needed -> true on system drive"
        );
    }

    #[test]
    fn throttle_emits_at_most_every_250ms() {
        // should_emit(last, now) = now - last >= 250
        assert!(!should_emit(1000, 1100), "100ms since last -> no emit");
        assert!(!should_emit(1000, 1249), "249ms -> no emit");
        assert!(should_emit(1000, 1250), "250ms -> emit");
        assert!(should_emit(1000, 2000), "1000ms -> emit");
        // saturating: last in the future (clock skew) -> no emit
        assert!(!should_emit(2000, 1000));
    }
}
