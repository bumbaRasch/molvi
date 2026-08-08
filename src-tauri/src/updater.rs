//! Signed auto-updater check/apply. Privacy (spec §10.1): version strings +
//! endpoint metadata only — never transcript/audio. Manual-smoke verification
//! (needs Tauri runtime + network); no unit tests.

use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::errors::{MolviError, Result};

/// Machine-readable update-check result. Serialized over IPC; the frontend maps
/// `up_to_date` to a localized toast. Privacy: version strings only (metadata).
#[derive(Debug, serde::Serialize)]
pub struct CheckResult {
    /// `true` = already on the latest version; `false` = an update is available.
    pub up_to_date: bool,
    /// The newly-available version (`None` when `up_to_date`).
    pub version: Option<String>,
    /// The currently-installed version.
    pub current_version: String,
}

/// Update check result (`CheckResult`). Metadata-only (version strings).
pub async fn check(app: &AppHandle) -> Result<CheckResult> {
    let u = app
        .updater()
        .map_err(|e| MolviError::Updater(format!("updater ext: {e}")))?;
    let current_version = app.package_info().version.to_string();
    match u
        .check()
        .await
        .map_err(|e| MolviError::Updater(format!("check: {e}")))?
    {
        None => Ok(CheckResult {
            up_to_date: true,
            version: None,
            current_version,
        }),
        Some(update) => Ok(CheckResult {
            up_to_date: false,
            version: Some(update.version),
            current_version,
        }),
    }
}

/// If an update is available, download_and_install + restart. If already up
/// to date, returns `Ok(())` (no-op).
pub async fn apply(app: &AppHandle) -> Result<()> {
    let u = app
        .updater()
        .map_err(|e| MolviError::Updater(format!("updater ext: {e}")))?;
    let update = match u
        .check()
        .await
        .map_err(|e| MolviError::Updater(format!("check: {e}")))?
    {
        Some(update) => update,
        None => return Ok(()),
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| MolviError::Updater(format!("download_and_install: {e}")))?;
    // restart() returns `!` (diverges via process::exit); coerces to Result<()>.
    app.restart()
}
