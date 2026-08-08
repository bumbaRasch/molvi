//! Tauri IPC commands: the frontend ↔ Rust bridge for Settings UI (Tasks 14–16)
//! and the tray's History item. Privacy §10.1: every `tracing::*` call logs
//! metadata only — ids, counts, durations, error strings from sqlite/serde/io.
//! NEVER interpolates entry/replacement/transcript/history text. See R10.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_dialog::DialogExt;

use crate::AppState;
use crate::coordinator;
use crate::dictionary::DictEntry;
use crate::errors::MolviError;
use crate::history::HistoryRow;
use crate::model_store;
use crate::settings::Settings;

// ── Format dispatch (dictionary import/export) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportFormat {
    Csv,
    Json,
}

/// Map a user-picked file path to its dictionary format. Returns None for
/// unknown/missing extensions — caller surfaces an error to the UI.
fn import_format_for_path(path: &std::path::Path) -> Option<ImportFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "csv" => Some(ImportFormat::Csv),
        "json" => Some(ImportFormat::Json),
        _ => None,
    }
}

// ── Settings ──

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), MolviError> {
    // R4: capture diff-relevant old fields, swap in memory, then persist.
    let (old_hotkey, old_altgr, old_autostart, old_hist_enabled, old_ui_lang) = {
        let mut g = state.settings.lock().unwrap();
        let o = (
            g.hotkey.clone(),
            g.hotkey_altgr_mirror,
            g.autostart,
            g.history.enabled,
            g.ui_lang.clone(),
        );
        *g = settings.clone();
        o
    };
    if let Err(e) = settings.save() {
        tracing::warn!("settings save failed: {e}");
        return Err(e);
    }
    tracing::info!("settings saved");

    // R3 live-apply: hotkey + altgr mirror.
    if (old_hotkey != settings.hotkey || old_altgr != settings.hotkey_altgr_mirror)
        && let Some(tx) = state.cmd_tx.lock().unwrap().clone()
    {
        match crate::hotkey::rebind(&app, &settings.hotkey, tx) {
            Ok(()) => tracing::info!("hotkey rebound"),
            Err(e) => tracing::warn!("hotkey rebind failed: {e}"),
        }
    }

    // R3 live-apply: autostart.
    if old_autostart != settings.autostart {
        let r = if settings.autostart {
            app.autolaunch().enable()
        } else {
            app.autolaunch().disable()
        };
        match r {
            Ok(()) => tracing::info!(
                "autostart {}",
                if settings.autostart {
                    "enabled"
                } else {
                    "disabled"
                }
            ),
            Err(e) => tracing::warn!("autostart sync failed: {e}"),
        }
    }

    // R3 + R2 live-apply: history.enabled flip.
    if old_hist_enabled != settings.history.enabled {
        let mut g = state.history.lock().unwrap();
        // history.enabled flip is fully live: AppPipeline reads the writer
        // fresh from AppState at each finalize, so disabling stops new
        // recordings on the next dictation and enabling persists them — both
        // without a restart. IPC queries also work immediately either way.
        if settings.history.enabled {
            match crate::history::History::open(&settings.history) {
                Ok(h) => {
                    *g = Some(Arc::new(h));
                    tracing::info!("history enabled");
                }
                Err(e) => tracing::warn!("history open failed: {e}"),
            }
        } else {
            *g = None;
            tracing::info!("history disabled (no erase — use disable_and_erase)");
        }
    }

    // R3 live-apply: tray menu/tooltip language.
    if old_ui_lang != settings.ui_lang {
        crate::tray::rebuild(&app);
        // Notify other webviews: the settings window loads at startup with the
        // old ui_lang, so onboarding's change wouldn't reach it otherwise. The
        // settings window re-fetches + re-localizes; onboarding handles itself.
        let _ = app.emit("ui-lang-changed", &settings.ui_lang);
        tracing::info!("ui_lang changed -> tray rebuilt + ui-lang-changed emitted");
    }

    // ponytail: post_processing / overlay / vad / audio.input_device /
    // paste_mode / language / model do NOT live-apply — AppPipeline clones
    // settings at construction; changes take effect on next launch (R3). The
    // engine + its CPU affinity both apply once at startup (lib.rs): a live
    // affinity flip would desync from the actually-loaded engine (the engine
    // reload itself needs a restart), so affinity is startup-only too and
    // re-applies for the new model on restart. recognition_mode IS live
    // (hotkey handler reads it at fire time).
    Ok(())
}

// ── Dictionary ──

#[tauri::command]
pub async fn dictionary_list(state: State<'_, AppState>) -> Result<Vec<DictEntry>, MolviError> {
    let list = state.dictionary.lock().unwrap().list()?;
    tracing::info!("dictionary: list returned {} entries", list.len());
    Ok(list)
}

#[tauri::command]
pub async fn dictionary_add(
    state: State<'_, AppState>,
    entry: String,
    replacement: String,
) -> Result<(), MolviError> {
    state.dictionary.lock().unwrap().add(&entry, &replacement)?;
    tracing::info!("dictionary: add");
    Ok(())
}

#[tauri::command]
pub async fn dictionary_remove(
    state: State<'_, AppState>,
    entry: String,
) -> Result<(), MolviError> {
    state.dictionary.lock().unwrap().remove(&entry)?;
    tracing::info!("dictionary: remove");
    Ok(())
}

#[tauri::command]
pub async fn dictionary_import_preview(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<crate::dictionary::ImportPreview>, MolviError> {
    let Some(path) = pick_open_path(&app) else {
        return Ok(None); // user cancelled the picker
    };
    if import_format_for_path(&path).is_none() {
        return Err(MolviError::Dictionary(
            "unsupported format (use .csv or .json)".into(),
        ));
    }
    let d = state.dictionary.lock().unwrap();
    let prev = d.preview_import(&path)?;
    tracing::info!(
        "dictionary: preview total={} new={} conflicts={}",
        prev.total,
        prev.new,
        prev.conflicts
    );
    Ok(Some(prev))
}

#[tauri::command]
pub async fn dictionary_import_apply(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), MolviError> {
    let p = std::path::Path::new(&path);
    let fmt = import_format_for_path(p)
        .ok_or_else(|| MolviError::Dictionary("unsupported format (use .csv or .json)".into()))?;
    let d = state.dictionary.lock().unwrap();
    let before = d.list()?.len();
    match fmt {
        ImportFormat::Csv => d.import_csv(p)?,
        ImportFormat::Json => d.import_json(p)?,
    }
    let after = d.list()?.len();
    tracing::info!(
        "dictionary: imported {} entries",
        after.saturating_sub(before)
    );
    Ok(())
}

#[tauri::command]
pub async fn dictionary_export(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), MolviError> {
    let Some(path) = pick_save_path(&app) else {
        return Ok(()); // user cancelled
    };
    let fmt = import_format_for_path(&path)
        .ok_or_else(|| MolviError::Dictionary("unsupported format (use .csv or .json)".into()))?;
    let d = state.dictionary.lock().unwrap();
    let count = d.list()?.len();
    match fmt {
        ImportFormat::Csv => d.export_csv(&path)?,
        ImportFormat::Json => d.export_json(&path)?,
    }
    tracing::info!("dictionary: exported {} entries", count);
    Ok(())
}

// ── Snippets ──
// Voice-cue → stored-block expansion (mirrors the Dictionary IPC shape). The
// store + `expand()` Smart-step already exist; these commands make it
// user-populable. Privacy §10.1: logs carry counts only — never cue/expansion.

#[tauri::command]
pub async fn snippet_list(
    state: State<'_, AppState>,
) -> Result<Vec<crate::snippets::SnippetEntry>, MolviError> {
    let list = state.snippets.lock().unwrap().list()?;
    tracing::info!("snippets: list returned {} entries", list.len());
    Ok(list)
}

#[tauri::command]
pub async fn snippet_add(
    state: State<'_, AppState>,
    cue: String,
    expansion: String,
) -> Result<(), MolviError> {
    state.snippets.lock().unwrap().add(&cue, &expansion)?;
    tracing::info!("snippets: add");
    Ok(())
}

#[tauri::command]
pub async fn snippet_remove(state: State<'_, AppState>, cue: String) -> Result<(), MolviError> {
    state.snippets.lock().unwrap().remove(&cue)?;
    tracing::info!("snippets: remove");
    Ok(())
}

/// Atomic import (pick → apply), no preview. Snippets are small-scale
/// (signatures/addresses); the dictionary's 2-IPC preview split is dictionary-
/// scale polish. YAGNI here — add a preview if users ever import en masse.
#[tauri::command]
pub async fn snippet_import(app: AppHandle, state: State<'_, AppState>) -> Result<(), MolviError> {
    let Some(path) = pick_open_path(&app) else {
        return Ok(()); // user cancelled the picker
    };
    let fmt = import_format_for_path(&path)
        .ok_or_else(|| MolviError::Snippet("unsupported format (use .csv or .json)".into()))?;
    let s = state.snippets.lock().unwrap();
    let before = s.list()?.len();
    match fmt {
        ImportFormat::Csv => s.import_csv(&path)?,
        ImportFormat::Json => s.import_json(&path)?,
    }
    let after = s.list()?.len();
    tracing::info!(
        "snippets: imported {} entries",
        after.saturating_sub(before)
    );
    Ok(())
}

#[tauri::command]
pub async fn snippet_export(app: AppHandle, state: State<'_, AppState>) -> Result<(), MolviError> {
    let Some(path) = pick_save_path(&app) else {
        return Ok(()); // user cancelled
    };
    let fmt = import_format_for_path(&path)
        .ok_or_else(|| MolviError::Snippet("unsupported format (use .csv or .json)".into()))?;
    let s = state.snippets.lock().unwrap();
    let count = s.list()?.len();
    match fmt {
        ImportFormat::Csv => s.export_csv(&path)?,
        ImportFormat::Json => s.export_json(&path)?,
    }
    tracing::info!("snippets: exported {} entries", count);
    Ok(())
}

// ── History ──

#[tauri::command]
pub async fn history_query(
    state: State<'_, AppState>,
    search: Option<String>,
    lang: Option<String>,
    since: Option<i64>,
    limit: u32,
    offset: u32,
) -> Result<Vec<HistoryRow>, MolviError> {
    let g = state.history.lock().unwrap();
    let Some(h) = g.as_ref() else {
        return Ok(vec![]); // R6: disabled -> empty list
    };
    let rows = h.query(search.as_deref(), lang.as_deref(), since, limit, offset)?;
    tracing::info!("history: query returned {} rows", rows.len());
    Ok(rows)
}

#[tauri::command]
pub async fn history_delete(state: State<'_, AppState>, id: i64) -> Result<(), MolviError> {
    let g = state.history.lock().unwrap();
    if let Some(h) = g.as_ref() {
        h.delete(id)?;
        tracing::info!("history: deleted id={id}");
    }
    Ok(())
}

#[tauri::command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<(), MolviError> {
    let g = state.history.lock().unwrap();
    if let Some(h) = g.as_ref() {
        h.clear()?;
        tracing::info!("history: cleared");
    }
    Ok(())
}

#[tauri::command]
pub async fn history_disable_and_erase(state: State<'_, AppState>) -> Result<(), MolviError> {
    {
        let g = state.history.lock().unwrap();
        if let Some(h) = g.as_ref() {
            h.drop_table()?;
        }
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.history.enabled = false;
        if let Err(e) = s.save() {
            tracing::warn!("settings save failed: {e}");
        }
    }
    *state.history.lock().unwrap() = None;
    tracing::info!("history: disabled + erased");
    Ok(())
}

#[tauri::command]
pub async fn history_bulk_delete(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> Result<(), MolviError> {
    let g = state.history.lock().unwrap();
    if let Some(h) = g.as_ref() {
        h.bulk_delete(&ids)?;
        tracing::info!("history: bulk deleted {} ids", ids.len());
    }
    Ok(())
}

#[tauri::command]
pub async fn history_distinct_langs(state: State<'_, AppState>) -> Result<Vec<String>, MolviError> {
    let g = state.history.lock().unwrap();
    let Some(h) = g.as_ref() else {
        return Ok(vec![]); // disabled -> empty
    };
    let langs = h.distinct_langs()?;
    Ok(langs)
}

// ── Updater ──

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<crate::updater::CheckResult, MolviError> {
    let r = crate::updater::check(&app).await?;
    tracing::info!(
        "update check: up_to_date={} version={:?} current={}",
        r.up_to_date,
        r.version,
        r.current_version
    );
    Ok(r)
}

#[tauri::command]
pub async fn apply_update(app: AppHandle) -> Result<(), MolviError> {
    crate::updater::apply(&app).await
}

// ── Re-paste ──

#[tauri::command]
pub async fn re_paste(state: State<'_, AppState>, id: i64) -> Result<(), MolviError> {
    let row = {
        let g = state.history.lock().unwrap();
        let Some(h) = g.as_ref() else {
            return Err(MolviError::Db("history disabled".into()));
        };
        h.get(id)?
    };
    let Some(row) = row else {
        tracing::warn!("re_paste: id={id} not found");
        return Ok(());
    };
    let target = crate::paste::capture_target();
    // ponytail: if invoked from Settings UI, target is the settings window's
    // HWND → focus guard accepts it (it IS foreground) → paste attempts into
    // the settings window. UI should advise focusing the target app first.
    let mode = state.settings.lock().unwrap().paste_mode;
    crate::paste::paste_text(&row.text, target, mode)?;
    tracing::info!("re_paste: id={} chars={}", id, row.text.chars().count());
    Ok(())
}

// ── Audio devices (R9) ──

#[tauri::command]
pub fn list_audio_devices() -> Vec<String> {
    use cpal::traits::HostTrait;
    cpal::default_host()
        .input_devices()
        .map(|ds| ds.map(|d| d.to_string()).collect())
        .unwrap_or_default()
}

/// On-demand mic-preview toggle (Settings UI level meter). Sets the poller gate
/// (`AppState.mic_preview`) AND forwards `Command::MicPreview` to the
/// coordinator, which owns the actual capture run-state (`session_active ||
/// preview`). Mirrors `cancel_operation`'s IPC→Command route. Privacy §10.1:
/// metadata-only log; capture feeds the local level scalar only.
#[tauri::command]
pub fn set_mic_preview(state: State<'_, AppState>, enabled: bool) {
    state.mic_preview.store(enabled, Ordering::Relaxed);
    if let Some(tx) = state.cmd_tx.lock().unwrap().as_ref() {
        let _ = tx.send(coordinator::Command::MicPreview(enabled));
        tracing::info!("set_mic_preview: {}", if enabled { "on" } else { "off" });
    } else {
        tracing::warn!("set_mic_preview: coordinator channel not ready");
    }
}

/// Onboarding-practice toggle (Task 10 D6). Mirrors `set_mic_preview`'s shape:
/// flips the AppState atomic only — NO coordinator forward needed (the finalize
/// side-thread reads the atomic directly; the live `begin_session`/`finalize`
/// path is unchanged). While `true`, finalize routes the post-processed text to
/// the onboarding window via `practice-result` instead of pasting, and
/// `begin_session` suppresses `overlay::show`. Privacy §10.1: metadata-only log.
#[tauri::command]
pub fn set_onboarding_practice(state: State<'_, AppState>, enabled: bool) {
    state.onboarding_practice.store(enabled, Ordering::Relaxed);
    tracing::info!(
        "set_onboarding_practice: {}",
        if enabled { "on" } else { "off" }
    );
}

/// Complete onboarding. Set `settings.onboarded = true`, persist, clean-exit any
/// live practice session (Cancel + reset flags), hide the onboarding window.
/// THEN surface the Settings window + emit a context-aware hint so the user
/// (especially a Skip-from-step-1 user who never saw the hotkey) knows what's
/// next. `ready` is the onboarding frontend's `engineReady` — race-free vs
/// Rust-side download-handle detection (Skip kicks off the download and calls
/// this back-to-back; the bg thread may not have stored the handle yet).
/// Privacy §10.1: the payload is a readiness bool + the hotkey config combo
/// (metadata); it crosses the IPC bus as an event, never `tracing::`.
#[tauri::command]
pub fn complete_onboarding(
    app: AppHandle,
    state: State<'_, AppState>,
    ready: bool,
) -> Result<(), MolviError> {
    let hotkey = {
        let mut g = state.settings.lock().unwrap();
        g.onboarded = true;
        if let Err(e) = g.save() {
            tracing::warn!("settings save failed: {e}");
            return Err(e);
        }
        g.hotkey.clone()
    };
    // Clean exit any in-flight practice session (Cancel covers Recording +
    // Processing; a no-op in Idle). Drops a live session so a stale finalize
    // can't emit a practice-result after the window hid.
    if let Some(tx) = state.cmd_tx.lock().unwrap().as_ref() {
        let _ = tx.send(coordinator::Command::Cancel);
    }
    state.onboarding_practice.store(false, Ordering::Relaxed);
    state.mic_preview.store(false, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.hide();
    }
    // Surface the home window + a hint toast in it (the Settings webview loads
    // at startup, so its listener is already registered). The global hotkey is
    // OS-level/focus-independent, so the user can dictate even with Settings open.
    crate::tray::show_settings(&app);
    let _ = app.emit(
        "post-onboarding-hint",
        serde_json::json!({ "ready": ready, "hotkey": hotkey }),
    );
    tracing::info!("onboarding complete");
    Ok(())
}

// ── Dialog helpers ──

fn pick_open_path(app: &AppHandle) -> Option<PathBuf> {
    app.dialog()
        .file()
        .add_filter("CSV/JSON", &["csv", "json"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
}

fn pick_save_path(app: &AppHandle) -> Option<PathBuf> {
    app.dialog()
        .file()
        .add_filter("CSV/JSON", &["csv", "json"])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
}

/// Pick a custom `.wav` for start/stop feedback via the OS file dialog. Returns
/// None when the user cancels (frontend treats None as no change).
#[tauri::command]
pub async fn pick_sound_file(app: AppHandle) -> Result<Option<String>, MolviError> {
    let path = app
        .dialog()
        .file()
        .add_filter("WAV", &["wav"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok());
    Ok(path.map(|p| p.to_string_lossy().into_owned()))
}

// ── Model picker (Task 14) ──
//
// SYNC `pub fn` (bodies do no awaiting; the long work runs on the spawned
// task). Matches `set_mic_preview`/`cancel_operation`. Privacy §10.1: events
// + logs carry only `model_id` + byte counts/error strings — model download
// touches no inference output.

#[tauri::command]
pub fn model_status() -> Result<Vec<model_store::ModelStatus>, MolviError> {
    model_store::model_status()
}

#[tauri::command]
pub fn download_model(
    model_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), MolviError> {
    // Hold the lock through the spawn-assign so the one-at-a-time guard is
    // race-free (two concurrent calls can't both pass is_finished + spawn).
    // Sync fn -> no await while held -> no await_holding_lock / future-Send
    // concern.
    let mut guard = state.model_download.lock().unwrap();
    if let Some(h) = guard.as_ref()
        && !h.inner().is_finished()
    {
        return Err(MolviError::ModelStore(
            "download already in progress".into(),
        ));
    }
    // No-op if already cached.
    if model_store::model_status()?
        .iter()
        .any(|m| m.model_id == model_id && m.cached)
    {
        return Ok(());
    }
    // Disk-space pre-check (avoid a 2.6 GB download that fails at the end).
    let total = model_store::grand_total(&model_id);
    if !model_store::has_disk_space(total)? {
        return Err(MolviError::ModelStore(format!(
            "insufficient disk space: need {total} bytes"
        )));
    }
    let app2 = app.clone();
    let id = model_id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let result = model_store::ensure_model(&id, |offset| {
            Some(hf_hub::progress::Progress::new(
                model_store::ModelProgressEmitter::new(app2.clone(), &id, total, offset),
            ))
        })
        .await;
        match result {
            Ok(_) => {
                let _ = app2.emit("model-download-complete", &id);
            }
            Err(e) => {
                tracing::warn!("model download failed: {e}"); // metadata-only
                let _ = app2.emit("model-download-failed", &id);
            }
        }
    });
    *guard = Some(handle);
    Ok(())
}

#[tauri::command]
pub fn cancel_model_download(state: State<'_, AppState>) -> Result<(), MolviError> {
    if let Some(h) = state.model_download.lock().unwrap().take() {
        h.abort(); // hf-hub content cache retains completed chunks; re-download resumes.
    }
    Ok(())
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    // Marker file → the restarted process reopens the settings window. Without
    // it the relaunch is tray-only (onboarded=true skips the onboarding gate)
    // and the user sees nothing. `app.restart()` takes no args (verified vs
    // Tauri 2 docs), so a side-channel file is the signal; cleared in setup.
    // apply_update (updater.rs) does NOT write this — an update restart follows
    // the normal tray launch.
    if let Ok(marker) = crate::paths::reopen_settings_marker() {
        let _ = std::fs::write(marker, b"");
    }
    app.restart(); // -> !  (diverges via process::exit); updater.rs uses the same call.
}

// ── Onboarding model selection ──
//
// First-run model pick. The returning-user path never calls this (setup
// auto-feeds the choice channel from settings); only the onboarding window
// invokes it after the user taps a model card. Privacy §10.1: the model id is
// a fixed catalog code and the language is a locale code — both metadata, no
// user content crosses tracing.

/// Onboarding model choice (first-run). Validates, persists model + language,
/// then signals the bg thread (via `model_selection_tx`) to download + spawn
/// the engine. The bg thread loops on the channel: a second send (retry /
/// different model) re-enters the loop. Privacy §10.1: id + locale code only.
#[tauri::command]
pub fn onboarding_select_model(
    model_id: String,
    language: String,
    state: State<'_, AppState>,
) -> Result<(), MolviError> {
    if !model_store::source_is_known(&model_id) {
        return Err(MolviError::ModelStore(format!(
            "unknown model id: {model_id}"
        )));
    }
    // Persist first (mirror complete_onboarding's lock-then-save), then signal.
    let snap = {
        let mut s = state.settings.lock().unwrap();
        s.model = model_id.clone();
        s.language = language.clone();
        s.clone()
    };
    snap.save()?;
    tracing::info!("onboarding model selected: {model_id}"); // metadata-only
    let sent = state
        .model_selection_tx
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|tx| tx.send((model_id, language)).ok())
        .is_some();
    if !sent {
        return Err(MolviError::ModelStore("model-choice channel closed".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_format_dispatch() {
        assert_eq!(
            import_format_for_path(std::path::Path::new("a.csv")),
            Some(ImportFormat::Csv)
        );
        assert_eq!(
            import_format_for_path(std::path::Path::new("a.json")),
            Some(ImportFormat::Json)
        );
        assert_eq!(
            import_format_for_path(std::path::Path::new("a.CSV")),
            Some(ImportFormat::Csv)
        );
        assert_eq!(import_format_for_path(std::path::Path::new("a.txt")), None);
        assert_eq!(import_format_for_path(std::path::Path::new("noext")), None);
    }

    #[test]
    fn source_is_known_accepts_known_rejects_unknown() {
        assert!(model_store::source_is_known(
            model_store::MODEL_GIGAAM_V3_E2E_CTC
        ));
        assert!(model_store::source_is_known(
            model_store::MODEL_NEMOTRON_0_6B
        ));
        assert!(!model_store::source_is_known("nemotron-9.9-fake"));
        assert!(!model_store::source_is_known(""));
    }

    #[test]
    fn dictionary_substrate_does_not_log_entry_text() {
        // Privacy regression guard: the Dictionary methods my IPC commands wrap
        // must never interpolate entry/replacement text into tracing logs.
        use std::io::Write;
        use std::sync::Mutex;
        use tracing_subscriber::fmt::MakeWriter;
        use tracing_subscriber::prelude::*;

        let buf = std::sync::Arc::new(Mutex::new(Vec::<u8>::new()));
        struct BufMaker(std::sync::Arc<Mutex<Vec<u8>>>);
        impl<'a> MakeWriter<'a> for BufMaker {
            type Writer = BufWriter;
            fn make_writer(&'a self) -> Self::Writer {
                BufWriter(self.0.clone())
            }
        }
        struct BufWriter(std::sync::Arc<Mutex<Vec<u8>>>);
        impl Write for BufWriter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new("trace"))
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(BufMaker(buf.clone()))
                    .with_ansi(false),
            );

        let sentinel = "СЕКРЕТНОЕСЛОВО_ДИКТ";
        tracing::dispatcher::with_default(&tracing::dispatcher::Dispatch::new(subscriber), || {
            let d = crate::dictionary::Dictionary::open_in_memory().unwrap();
            d.add(sentinel, sentinel).unwrap();
            let _ = d.list();
            let _ = d.apply(&format!("test {sentinel} end"));
        });

        let logs = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();
        assert!(
            !logs.contains(sentinel),
            "PRIVACY: dictionary substrate leaked entry text into logs:\n{logs}"
        );
    }
}
