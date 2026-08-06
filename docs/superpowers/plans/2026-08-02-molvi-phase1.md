# molvi — Phase-1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a Windows 11 push-to-talk dictation app that captures mic, streams partial Russian transcription into a floating overlay, and pastes the finalized transcript into the previously-focused window — all local CPU inference, no cloud.

**Architecture:** Tauri 2 webview shell wrapping four cooperating threads (Tauri main, coordinator state machine, cpal audio, inference worker) that communicate through narrow channels (mpsc commands, SPSC audio ring, Tauri events). One inference dependency (`transcribe-rs`) provides the GigaAM-v3 `e2e_ctc` model, `VadChunked` streaming, VAD, and mel feature extraction. The overlay is `focusable:false` so the paste target retains keyboard focus throughout.

**Tech Stack:** Rust (Tauri 2.11.5, transcribe-rs 0.3.11 / ort 2.0.0-rc.12 pinned transitively, cpal 0.18.1, rubato 4.0.0, rtrb, enigo 0.6.1, arboard 3.6.1, hf-hub 1.0.0, tokio, the `windows` crate, thiserror, tracing); frontend = Vite + vanilla TypeScript (no framework).

**Spec:** [`docs/superpowers/specs/2026-08-02-molvi-push-to-talk-design.md`](../specs/2026-08-02-molvi-push-to-talk-design.md)

---

## Global Constraints

Copied verbatim from the spec — every task inherits these:

- **App identity:** `molvi`, identifier `com.molvi.app`. App-data dir: `%APPDATA%\com.molvi.app\`.
- **Target platform (Phase 1):** Windows 11 x64. WebView2 is the webview runtime.
- **Toolchain:** Rust stable; MSRV dictated by Tauri 2.11 + transcribe-rs 0.3.11 (expected ≥ 1.80), recorded in `rust-toolchain.toml`. MSVC C/C++ build tools required (native `ort`/Tauri build).
- **Inference dependency:** `transcribe-rs = { version = "0.3.11", features = ["onnx", "audio-features"] }`. `ort` is **not** pinned manually — transcribe-rs 0.3.11 pins it to `=2.0.0-rc.12` (Task-0-confirmed).
- **Phase-1 model:** GigaAM-v3 `v3_e2e_ctc` int8 (`v3_e2e_ctc.int8.onnx` + `vocab.txt`), loaded **natively** via `transcribe_rs::onnx::gigaam::GigaAMModel::load`. `e2e_rnnt` 3-file path is **out of scope** (Task 0 PASS — WER covered by official 12% avg, RTF 0.067).
- **Audio pipeline:** 16 kHz mono f32 in `[-1,1]` at the model. cpal captures device-native rate (often 48 kHz) → `rubato` band-limited resample to 16 kHz. 16 kHz device = no-op fast path.
- **VAD/chunker:** `EnergyVad::new(480, 0.01)` wrapped in `SmoothedVad::new(_, 15, 15, 2)`, inside `VadChunked` with `min_chunk_secs: 1.0`, `max_chunk_secs: 20.0`, `padding_secs: 0.1`, `smart_split_search_secs: Some(3.0)`, `merge_separator: " "`.
- **Overlay window flags:** `decorations:false, transparent:true, always_on_top:true, skip_taskbar:true, resizable:false, focused:false, focusable:false`, plus `noRedirectionBitmap:true` (mitigate white flash).
- **Paste invariant:** overlay is `focusable:false` so paste target keeps focus. Before `Ctrl+V`, assert `GetForegroundWindow() == target`; on mismatch route to clipboard + toast (never mispaste).
- **Privacy / logging (§10.1):** NEVER log transcript text, partial transcripts, or audio samples at any level. Logs carry metadata only (stage, chunk counts, durations ms, errors, model id, RTF). Enforced by an assertion test (Task 12).
- **Naming/copy:** the app is `molvi` (lowercase) everywhere user-facing.
- **YAGNI / ponytail:** no speculative abstractions, no factory-for-one, no settings for values that never change. Calibration knobs that survive (resampling, VAD thresholds, chunk sizes) stay; everything else is a constant.

---

## Task 0 — Verification gate (PASSED 2026-08-02)

Retained for context; no work remains. `v3_e2e_ctc` loads natively, RTF **0.067** (< 0.7), punctuation present, transcript visibly correct. The Task-0 binary (`molvi-task0/src/main.rs`) migrates verbatim into Task 8 (`engine.rs`) — no duplicate work. `e2e_rnnt` 3-file path **deleted from Phase-1 scope**. Full results recorded at the bottom of this file.

---

## File Structure (target layout)

```
molvi/                                 # repo root
├── rust-toolchain.toml                # Task 1
├── package.json                       # Task 1 (Vite frontend build)
├── AGENTS.md                          # Task 1 (canonical commands)
├── src-tauri/
│   ├── Cargo.toml                     # Task 1, grown each task
│   ├── tauri.conf.json                # Task 1, windows+tray config
│   ├── build.rs                       # Task 1
│   └── src/
│       ├── main.rs                    # Task 1 skeleton → Task 13 full wiring
│       ├── paths.rs                   # Task 2  (app data dir resolution)
│       ├── errors.rs                  # Task 2  (thiserror enum)
│       ├── log.rs                     # Task 3  (tracing init + privacy)
│       ├── settings.rs                # Task 4  (JSON config, defaults)
│       ├── model_store.rs             # Task 5  (hf-hub download + cache)
│       ├── resample.rs                # Task 6  (rubato 48k→16k + fast path)
│       ├── audio.rs                   # Task 7  (cpal capture → SPSC ring)
│       ├── engine.rs                  # Task 8  (transcribe-rs worker loop)
│       ├── coordinator.rs             # Task 9  (state machine)
│       ├── hotkey.rs                  # Task 10 (global-shortcut PTT)
│       ├── paste.rs                   # Task 11 (clipboard-paste + fallback)
│       └── overlay.rs                 # Task 12 (overlay window control)
├── src/                               # Vite frontend root
│   ├── settings.html / settings.ts    # Task 1 (placeholder) → Phase 2 (full UI)
│   └── overlay.html / overlay.ts      # Task 12 (caption bubble)
├── tests/
│   ├── fixtures/ru/                   # RU golden clips (from Task 0)
│   └── (integration tests)            # Task 13
└── molvi-task0/                       # verified gate binary (reference)
```

Dependency order: `paths/errors → log → settings → model_store → resample → audio → engine → coordinator → {hotkey, paste, overlay} → main.rs integration`. Each task lists exact Consumes/Produces interfaces.

---

## Task 1: Tauri 2 shell + project bootstrap

**Role:** a runnable `cargo tauri dev` that opens a (blank) settings window and a tray, with single-instance enforced. Every later task grows this skeleton.

**Files:**
- Create: `rust-toolchain.toml`, `package.json`, `vite.config.ts`, `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src/settings.html`, `src/settings.ts`, `AGENTS.md`
- Test: `cargo tauri dev` launches without error; tray icon appears; second launch forwards to the first.

**Interfaces:**
- Produces: the `molvi` Tauri app (`com.molvi.app`), a managed `AppState` (empty now, widened each task), the two webview window labels `"settings"` and `"overlay"`, and `AGENTS.md` canonical commands.

- [ ] **Step 1: Scaffold the Tauri 2 project (vanilla TS, no framework)**

From repo root (Windows PowerShell):
```powershell
# create-tauri-app writes a Vite + vanilla TS scaffold
npm create tauri-app@latest molvi-app-tmp -- --template vanilla-ts --manager npm --identifier com.molvi.app
# move the scaffold's contents into the repo root so src/ and src-tauri/ sit at root
Get-ChildItem -Path .\molvi-app-tmp | Copy-Item -Recurse -Force -Destination .
Remove-Item -Recurse -Force .\molvi-app-tmp
```
Then hand-edit `src-tauri/tauri.conf.json` so `productName = "molvi"`, `identifier = "com.molvi.app"`, and the app window is labeled `"settings"` (title "molvi", width 520, height 360, not resizable yet, `visible:false` on start — we show it from the tray). Keep the Vite dev path `/` and frontend dist `../src`.

- [ ] **Step 2: Pin the toolchain and record MSRV**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```
Run `rustc --version` and record the actual MSRV line into `AGENTS.md` (Task 1, Step 6).

- [ ] **Step 3: Add core dependencies to `src-tauri/Cargo.toml`**

```toml
[package]
name = "molvi"
version = "0.1.0"
edition = "2024"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2.11", features = ["tray-icon"] }
tauri-plugin-single-instance = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"

[features]
default = []
```
(Engine/audio/paste deps are added in their own tasks — keep this skeleton minimal.)

- [ ] **Step 4: Minimal `main.rs` with tray + single-instance + placeholder state**

`src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, TrayIconBuilder, WebviewWindowBuilder};

#[derive(Default)]
pub struct AppState; // grown each task

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // second launch: surface the settings window if it exists
            if let Some(w) = app.get_webview_window("settings") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .manage(AppState::default())
        .setup(|app| {
            // Settings window exists in config; create lazily from tray instead.
            TrayIconBuilder::new()
                .tooltip("molvi")
                .icon(app.default_window_icon().cloned().expect("no icon"))
                .on_tray_icon_event(|app, _event| {
                    if let Some(w) = app.get_webview_window("settings") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                })
                .build(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running molvi");
}
```

- [ ] **Step 5: Configure `tauri.conf.json` for one window + tray capability**

Remove the auto-created `"settings"` from the `"windows"` array (we build it from the tray / single-instance handler), or leave it with `"visible": false`. Add the `"trayIcon"` / single-instance to `app.security.csp` only if a CSP error appears (default permissive for Phase 1 is fine). Ensure `"identifier": "com.molvi.app"`.

- [ ] **Step 6: Write `AGENTS.md` with canonical commands**

`AGENTS.md`:
```markdown
# molvi — agent notes

## Commands
- `cargo tauri dev`        — run the app (debug)
- `cargo tauri build`      — produce NSIS/MSI installer
- `cargo test`             — run unit tests (model-free; engine test is feature-gated, see Task 8)
- `cargo clippy --all-targets -- -D warnings`  — lint
- `cargo fmt`              — format

## Platform
Windows 11 x64. Requires MSVC build tools + WebView2. Rust stable (MSRV recorded in rust-toolchain.toml).

## Privacy (HARD RULE, spec §10.1)
NEVER log transcript text, partial transcripts, or audio samples — not even at `trace`. Logs carry metadata only. Enforced by the log-privacy test in Task 12.
```

- [ ] **Step 7: Verify it runs**

Run: `cargo tauri dev`
Expected: app launches, tray icon present, no console errors. Launch the binary a second time while the first runs → second instance exits and the first's settings window shows (manual check; automated in Task 13).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: scaffold Tauri 2 shell with tray + single-instance"
```

---

## Task 2: `paths.rs` + `errors.rs` (foundation)

**Files:**
- Create: `src-tauri/src/paths.rs`, `src-tauri/src/errors.rs`
- Modify: `src-tauri/src/main.rs` (add `mod paths; mod errors;`)
- Test: `src-tauri/src/paths.rs` (inline `#[test]`), `src-tauri/src/errors.rs` (inline `#[test]`)

**Interfaces:**
- Produces:
  - `paths::app_data_dir() -> PathBuf` (resolves `%APPDATA%\com.molvi.app\`, creating it).
  - `paths::models_dir() -> PathBuf` (`app_data_dir/models/`).
  - `paths::settings_path() -> PathBuf` (`app_data_dir/settings.json`).
  - `paths::log_path() -> PathBuf` (`app_data_dir/logs/`).
  - `errors::MolviError` (thiserror enum) + `pub type Result<T> = std::result::Result<T, MolviError>`.

- [ ] **Step 1: Write the failing test for `paths`**

Append to `src-tauri/src/paths.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_data_dir_ends_with_identifier() {
        let dir = app_data_dir().unwrap();
        assert!(dir.ends_with("com.molvi.app"));
        assert!(dir.exists(), "dir should be created on call");
    }

    #[test]
    fn subpaths_are_nested() {
        let base = app_data_dir().unwrap();
        assert_eq!(models_dir().unwrap(), base.join("models"));
        assert_eq!(settings_path().unwrap(), base.join("settings.json"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml paths`
Expected: FAIL (module/functions not defined).

- [ ] **Step 3: Implement `paths.rs`**

```rust
use std::path::PathBuf;
use crate::errors::{MolviError, Result};

const IDENTIFIER: &str = "com.molvi.app";

/// `%APPDATA%\com.molvi.app\`, created if missing. Ponytail: %APPDATA% via
/// the `dirs` crate would add a dep; std::env::var("APPDATA") is sufficient
/// on Windows (the only Phase-1 platform).
pub fn app_data_dir() -> Result<PathBuf> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| MolviError::Paths("APPDATA env var not set".into()))?;
    let dir = PathBuf::from(appdata).join(IDENTIFIER);
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::Paths(format!("create app data dir: {e}")))?;
    Ok(dir)
}

pub fn models_dir() -> Result<PathBuf> {
    let dir = app_data_dir()?.join("models");
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::Paths(format!("create models dir: {e}")))?;
    Ok(dir)
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("settings.json"))
}

pub fn log_dir() -> Result<PathBuf> {
    let dir = app_data_dir()?.join("logs");
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::Paths(format!("create log dir: {e}")))?;
    Ok(dir)
}
```

- [ ] **Step 4: Implement `errors.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MolviError {
    #[error("paths: {0}")]
    Paths(String),

    #[error("settings: {0}")]
    Settings(String),

    #[error("model store: {0}")]
    ModelStore(String),

    #[error("audio: {0}")]
    Audio(String),

    #[error("engine: {0}")]
    Engine(String),

    #[error("inference: {0}")]
    Inference(String),

    #[error("hotkey: {0}")]
    Hotkey(String),

    #[error("paste: {0}")]
    Paste(String),

    #[error("overlay: {0}")]
    Overlay(String),
}

pub type Result<T> = std::result::Result<T, MolviError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_with_category() {
        let e = MolviError::Audio("mic denied".into());
        assert_eq!(e.to_string(), "audio: mic denied");
    }
}
```

- [ ] **Step 5: Wire modules into `main.rs`**

Add after the `use` block in `main.rs`:
```rust
mod errors;
mod paths;
```
Add `dirs`-free resolution confirmed (uses `APPDATA`). Update `Cargo.toml` only if `thiserror`/`serde` not already present (they are from Task 1).

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS (paths + errors tests green).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/paths.rs src-tauri/src/errors.rs src-tauri/src/main.rs
git commit -m "feat: paths + errors foundation"
```

---

## Task 3: `log.rs` — tracing init + privacy discipline

**Files:**
- Create: `src-tauri/src/log.rs`
- Modify: `src-tauri/Cargo.toml` (add `tracing`, `tracing-subscriber`, `tracing-appender`), `src-tauri/src/main.rs` (`mod log;`, call `log::init()`)

**Interfaces:**
- Produces: `log::init() -> Result<WorkerGuard>` (must be held in `main` for the lifetime of the app so the appender flushes). The module exports a doc-comment restating §10.1.

- [ ] **Step 1: Add logging deps**

In `src-tauri/Cargo.toml` `[dependencies]`:
```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
```

- [ ] **Step 2: Implement `log.rs`**

```rust
//! Tracing init. PRIVACY (spec §10.1): NEVER log transcript text, partial
//! transcripts, or audio samples — not even at trace. This module only sets
//! up the appender + filter; the discipline is enforced at every call site
//! and by the log-privacy assertion test in Task 12.

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::errors::Result;
use crate::paths;

/// Initialize file+stderr logging. The returned `WorkerGuard` MUST be held
/// for the lifetime of the app (keep it in `main`), or buffered logs are lost.
pub fn init() -> Result<WorkerGuard> {
    let log_dir = paths::log_dir()?;
    let file_appender = tracing_appender::rolling::daily(&log_dir, "molvi.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(true))
        .init();

    tracing::info!("molvi logging initialized (dir = {})", log_dir.display());
    Ok(guard)
}
```

- [ ] **Step 3: Wire into `main.rs`**

```rust
mod log;
// ...
fn main() {
    let _log_guard = log::init().expect("log init");
    // ... existing tauri::Builder::default()...
}
```

- [ ] **Step 4: Verify it builds and writes a log line**

Run: `cargo tauri dev`, then quit.
Expected: a file `%APPDATA%\com.molvi.app\logs\molvi.log.<date>` exists containing `molvi logging initialized`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/log.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: tracing init with file appender"
```

---

## Task 4: `settings.rs` — JSON config, defaults, corrupt recovery

**Files:**
- Create: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/main.rs` (`mod settings;`, load settings into `AppState`)

**Interfaces:**
- Produces:
  - `settings::Settings` (serde struct, `#[serde(default)]` everywhere so any missing key/section defaults — no schema version, no migration path; backward compat is explicitly not a goal) with `Settings::default()`, `Settings::load() -> Result<Settings>`, `Settings::save(&self) -> Result<()>`.
  - Field names exactly per spec §6.8: `hotkey`, `push_to_talk`, `model`, `language`, `paste_mode`, `overlay.{enabled,position,show_waveform,show_timer}`, `audio.{input_device,buffer_frames}`, `vad.{min_chunk_secs,max_chunk_secs,padding_secs,energy_threshold}`, `logging.level`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_expected_values() {
        let s = Settings::default();
        assert_eq!(s.hotkey, "Alt+`");
        assert!(s.push_to_talk);
        assert_eq!(s.model, "gigaam-v3-e2e-ctc");
        assert_eq!(s.language, "ru");
        assert_eq!(s.paste_mode, PasteMode::Clipboard);
        assert!((s.vad.min_chunk_secs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn missing_keys_default_via_serde() {
        // Only hotkey provided; every other field (incl. nested vad.*) defaults.
        let json = r#"{"hotkey":"Ctrl+Space"}"#;
        let s = Settings::from_json_str(json).unwrap();
        assert_eq!(s.hotkey, "Ctrl+Space");
        assert_eq!(s.paste_mode, PasteMode::Clipboard); // defaulted
        assert!((s.vad.max_chunk_secs - 20.0).abs() < 1e-6); // deep-defaulted
    }

    #[test]
    fn corrupt_json_recovers_to_defaults() {
        // Structurally invalid JSON must not panic; load defaults instead.
        let s = Settings::from_json_str("{ not valid json").unwrap();
        assert_eq!(s.hotkey, "Alt+`"); // fell back to defaults
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempdir_for_test();
        let mut s = Settings::default();
        s.hotkey = "Ctrl+Space".into();
        s.save_to(&dir.join("settings.json")).unwrap();
        let loaded = Settings::load_from(&dir.join("settings.json")).unwrap();
        assert_eq!(loaded.hotkey, "Ctrl+Space");
    }
}
```
(`tempdir_for_test()` is a tiny helper using `std::env::temp_dir()` + a unique subdir; define it inline in the test module — no `tempfile` dep.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings`
Expected: FAIL (types not defined).

- [ ] **Step 3: Implement `settings.rs`**

```rust
use serde::{Deserialize, Serialize};

use crate::errors::{MolviError, Result};
use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PasteMode { Clipboard, Type, Auto }

impl Default for PasteMode { fn default() -> Self { PasteMode::Clipboard } }

// `#[serde(default)]` on every struct → any missing key at any depth picks up
// the field's Default. No version field, no migration: backward compat is not a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    pub enabled: bool,
    pub position: String,        // "bottom" (Phase 1); "top" arrives Phase 3
    pub show_waveform: bool,
    pub show_timer: bool,
}
impl Default for OverlaySettings {
    fn default() -> Self {
        Self { enabled: true, position: "bottom".into(), show_waveform: true, show_timer: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub input_device: Option<String>,
    pub buffer_frames: Option<u32>,
}
impl Default for AudioSettings {
    fn default() -> Self { Self { input_device: None, buffer_frames: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VadSettings {
    pub min_chunk_secs: f32,
    pub max_chunk_secs: f32,
    pub padding_secs: f32,
    pub energy_threshold: f32,
}
impl Default for VadSettings {
    fn default() -> Self {
        Self { min_chunk_secs: 1.0, max_chunk_secs: 20.0, padding_secs: 0.1, energy_threshold: 0.01 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    pub push_to_talk: bool,
    pub model: String,
    pub language: String,
    pub paste_mode: PasteMode,
    pub overlay: OverlaySettings,
    pub audio: AudioSettings,
    pub vad: VadSettings,
    pub logging: LoggingSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingSettings { pub level: String }

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Alt+`".into(),
            push_to_talk: true,
            model: "gigaam-v3-e2e-ctc".into(),
            language: "ru".into(),
            paste_mode: PasteMode::Clipboard,
            overlay: OverlaySettings::default(),
            audio: AudioSettings::default(),
            vad: VadSettings::default(),
            logging: LoggingSettings { level: "info".into() },
        }
    }
}

impl Settings {
    /// Load from the canonical path. Missing → default; corrupt → default (logged).
    pub fn load() -> Result<Self> {
        let path = paths::settings_path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json_str(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(MolviError::Settings(format!("read {}: {e}", path.display()))),
        }
    }

    /// Parse. Missing keys default via `#[serde(default)]`. Invalid JSON → default.
    pub fn from_json_str(text: &str) -> Result<Self> {
        match serde_json::from_str::<Settings>(text) {
            Ok(s) => Ok(s),
            Err(e) => {
                tracing::warn!("settings JSON invalid ({e}); using defaults");
                Ok(Self::default())
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&paths::settings_path()?)
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| MolviError::Settings(format!("serialize: {e}")))?;
        std::fs::write(path, text)
            .map_err(|e| MolviError::Settings(format!("write {}: {e}", path.display())))
    }
}
```

- [ ] **Step 4: Add the tempdir helper + run tests**

Add inside the test module:
```rust
fn tempdir_for_test() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("molvi-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
```
Run: `cargo test --manifest-path src-tauri/Cargo.toml settings`
Expected: PASS (all four tests).

- [ ] **Step 5: Wire into `main.rs` AppState**

```rust
mod settings;
// in AppState:
#[derive(Default)]
pub struct AppState {
    pub settings: std::sync::Mutex<settings::Settings>,
}
// in setup: load
let cfg = settings::Settings::load().unwrap_or_default();
app.manage(AppState { settings: std::sync::Mutex::new(cfg) });
```
(Remove the previous `#[derive(Default)]` on `AppState`; manage an instance explicitly.)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/settings.rs src-tauri/src/main.rs
git commit -m "feat: settings with default-merge + corrupt recovery"
```

---

## Task 5: `model_store.rs` — hf-hub download + cache

**Files:**
- Create: `src-tauri/src/model_store.rs`
- Modify: `src-tauri/Cargo.toml` (add `hf-hub = "1.0"`, `tokio = { version = "1", features = ["rt-multi-thread", "sync"] }`), `src-tauri/src/main.rs` (`mod model_store;`)

**Interfaces:**
- Produces:
  - `model_store::ensure_model(model_id: &str, progress: impl FnMut(u64, u64)) -> Result<PathBuf>` — downloads (resume-capable) into `%APPDATA%\com.molvi.app\models\<id>\` laid out exactly as `GigaAMModel::load` expects (`model.int8.onnx` + `vocab.txt`), returns the dir.
  - `MODEL_GIGAAM_V3_E2E_CTC: &str = "gigaam-v3-e2e-ctc"` (the Phase-1 model id).
  - Constants for the HF repo + filenames.

- [ ] **Step 1: Add deps**

`src-tauri/Cargo.toml`:
```toml
hf-hub = "1.0"
tokio = { version = "1", features = ["rt-multi-thread", "sync"] }
```

- [ ] **Step 2: Implement `model_store.rs`**

```rust
//! First-run model download + cache (spec §6.9).
//! Layout on disk (what `GigaAMModel::load` expects):
//!   %APPDATA%\com.molvi.app\models\gigaam-v3-e2e-ctc\
//!     model.int8.onnx   (renamed from v3_e2e_ctc.int8.onnx)
//!     vocab.txt         (renamed from v3_e2e_ctc_vocab.txt)

use std::path::PathBuf;

use crate::errors::{MolviError, Result};
use crate::paths;

pub const MODEL_GIGAAM_V3_E2E_CTC: &str = "gigaam-v3-e2e-ctc";

const HF_REPO: &str = "istupakov/gigaam-v3-onnx";

/// Files we need from the repo, mapped to their on-disk names. The int8 e2e_ctc
/// graph + its vocab. (Other variants exist in the repo but we download only these.)
const FILES: &[(&str, &str)] = &[
    ("v3_e2e_ctc.int8.onnx", "model.int8.onnx"),
    ("v3_e2e_ctc_vocab.txt", "vocab.txt"),
];

/// Ensure the model is present on disk (download if missing), returning the
/// model directory. `progress(downloaded_bytes, total_bytes)` fires as the
/// download proceeds (best-effort; 0 total = unknown). Resume is handled by
/// hf-hub's cache when partial files exist.
pub fn ensure_model<F: FnMut(u64, u64)>(model_id: &str, mut progress: F) -> Result<PathBuf> {
    if model_id != MODEL_GIGAAM_V3_E2E_CTC {
        return Err(MolviError::ModelStore(format!("unknown model id: {model_id}")));
    }
    let dir = paths::models_dir()?.join(model_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::ModelStore(format!("create model dir: {e}")))?;

    // Fast path: both files present → nothing to do.
    let all_present = FILES.iter().all(|(_, dst)| dir.join(dst).exists());
    if all_present {
        tracing::info!("model {model_id} already cached at {}", dir.display());
        return Ok(dir);
    }

    tracing::info!("downloading model {model_id} from hf:{HF_REPO}");
    let api = hf_hub::api::sync::ApiBuilder::new()
        .with_cache_dir(paths::models_dir()?.join("_hf"))
        .build()
        .map_err(|e| MolviError::ModelStore(format!("hf api: {e}")))?;

    for (src, dst) in FILES {
        let cached = api
            .download_with_progress(HF_REPO, src, Some(&mut progress))
            .map_err(|e| MolviError::ModelStore(format!("download {src}: {e}")))?;
        let target = dir.join(dst);
        // hf-hub caches by hash; copy/symlink to the name the loader expects.
        // Ponytail: copy (not symlink) to keep it simple + cross-fs safe.
        std::fs::copy(&cached, &target)
            .map_err(|e| MolviError::ModelStore(format!("stage {dst}: {e}")))?;
    }
    tracing::info!("model {model_id} ready at {}", dir.display());
    Ok(dir)
}
```

> **In-task verify (spec §16.7):** the exact `hf-hub` `download_with_progress` signature varies by 1.0.x point release. If the call above doesn't compile, consult `hf-hub` docs (use the find-docs skill) for the 1.0 sync API and adapt the download+progress call — keep the on-disk layout (`model.int8.onnx` + `vocab.txt`) and the cache fast-path unchanged. This is the one place in this task where a doc-check is expected.

- [ ] **Step 3: Smoke-verify the layout (manual, needs ~225 MB download)**

Run a throwaway check from a temp binary or `cargo test`:
```powershell
# from src-tauri
$cargo run  # (only after main.rs optionally exposes a CLI hook; otherwise defer to Task 13)
```
Verify (deferred to Task 13 integration): `%APPDATA%\com.molvi.app\models\gigaam-v3-e2e-ctc\model.int8.onnx` (~225 MB) and `vocab.txt` exist after first run.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/model_store.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: hf-hub model download + cache layout"
```

---

## Task 6: `resample.rs` — rubato 48k→16k + 16k fast path

**Files:**
- Create: `src-tauri/src/resample.rs`
- Modify: `src-tauri/Cargo.toml` (add `rubato = "4.0"`), `src-tauri/src/main.rs` (`mod resample;`)

> **⚠️ AMENDMENT (2026-08-02, dep audit):** `rubato 4.0.0` IS released (2026-07-09) and is the latest stable — the original note below is VOID. **Task 6 targets `rubato = "4.0"`.** `FftFixedIn` was REMOVED in 4.0 (replaced by `Fft` + the `audioadapter` buffer API); the Step-4 code block is preserved only as intent reference and WILL NOT compile against 4.0. The behavior contract (3:1 downsample, bounded peak, 16k no-op passthrough) is unchanged — see the amended API note under Step 4 for the replacement and verify exact signatures via the `find-docs` skill (`/henquist/rubato`) or docs.rs/rubato/4.0.0.

**Interfaces:**
- Produces:
  - `resample::Resampler` struct owning the rubato resampler (or a no-op when input is already 16 kHz).
  - `Resampler::new(in_rate: u32, out_rate: u32, channels: usize) -> Result<Self>`
  - `Resampler::process(&mut self, input: &[f32]) -> Result<Vec<f32>>` — consumes any number of input frames; internally buffers sub-block remainders; emits resampled frames.
  - `Resampler::is_noop(&self) -> bool`

- [ ] **Step 1: Add dep**

`src-tauri/Cargo.toml`: `rubato = "4.0"`.

- [ ] **Step 2: Write the failing test (sine wave, known ratio)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_rates_equal() {
        let mut r = Resampler::new(16000, 16000, 1).unwrap();
        assert!(r.is_noop());
        let out = r.process(&[0.5; 480]).unwrap();
        assert_eq!(out.len(), 480);
    }

    #[test]
    fn downsamples_48k_to_16k_ratio() {
        // 48000 -> 16000 is a 3:1 downsample. Feed 1 second of 1 kHz sine.
        let mut r = Resampler::new(48000, 16000, 1).unwrap();
        let n = 48000;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48000.0).sin() * 0.5)
            .collect();
        let out = r.process(&input).unwrap();
        // Allow a few samples of rubato latency slop.
        let expected = 16000f64;
        let actual = out.len() as f64;
        assert!((actual - expected).abs() < 64.0, "expected ~{expected} got {actual}");

        // Anti-aliasing sanity: output amplitude stays within input envelope.
        let peak = out.iter().cloned().fold(0.0f32, f32::max).abs();
        assert!(peak > 0.3 && peak < 0.7, "unexpected peak {peak}");
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml resample`
Expected: FAIL.

- [ ] **Step 4: Implement `resample.rs`**

> **⚠️ AMENDMENT (2026-08-02): the code block below is OBSOLETE under `rubato = "4.0"`.** `FftFixedIn` was removed in rubato 4.0 (replaced by `Fft` + the `audioadapter` buffer API). Preserve the *structure* — a `Resampler` struct wrapping `Option<inner>`, a 16k no-op fast path (`is_noop()`), and a `process(&mut self, &[f32]) -> Result<Vec<f32>>` that buffers sub-chunk remainders and drains via `input_frames_next()` each iteration — but rebuild the inner against the 4.0 API:
> - `use rubato::{Resampler, Fft, FixedSync, Indexing};` + `use audioadapter_buffers::direct::InterleavedSlice;` (re-exported by rubato 4.0).
> - Constructor: `Fft::<f32>::new(in_rate as f64, out_rate as f64, chunk_size, channels, FixedSync::Input)` — note `Fft::new` takes rates as `f64` and DROPPED the old `sub_chunks` arg (4.0 picks it automatically; `Fft::new_custom` if you need control).
> - Per-iteration streaming (realtime-safe, no alloc): read `needed = input_frames_next()`, wrap input in `InterleavedSlice::new(&in_buf, channels, needed)`, allocate an output adapter once (sized to `output_frames_max()`), call `process_into_buffer(&in, &mut out, Some(&Indexing::new()))`, collect `frames_written` output frames.
> - The 16k no-op fast path (`in_rate == out_rate` → return input unchanged, `inner = None`) is unchanged.
> - The Step-2 sine-wave test (3:1 ratio, ~16000 out ±64 slop, peak in (0.3, 0.7)) is the acceptance gate — match its behavior, not the obsolete code line-for-line. Verify exact 4.0 signatures via the `find-docs` skill or docs.rs/rubato/4.0.0 (migration guide enumerates all 3.x→4.0 changes).

The block below is kept for intent only:

```rust
use rubato::{FftFixedIn, Resampler as RubatoResampler};

use crate::errors::{MolviError, Result};

/// Band-limited resampler (spec D7). No-op fast path when `in_rate == out_rate`.
/// One block of input may produce zero or multiple blocks of output; sub-block
/// remainders are buffered internally so callers can feed arbitrary lengths.
pub struct Resampler {
    in_rate: u32,
    out_rate: u32,
    channels: usize,
    inner: Option<RubatoResampler<f32>>, // None = no-op passthrough
    leftover: Vec<Vec<f32>>,             // per-channel pending input frames
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32, channels: usize) -> Result<Self> {
        if in_rate == out_rate {
            return Ok(Self { in_rate, out_rate, channels, inner: None, leftover: vec![Vec::new(); channels] });
        }
        // chunk_size = a convenient input frame count (e.g. 480 at 16k = 30ms).
        // For 48k input this is 1440 samples; sub-frames buffer in `leftover`.
        let chunk = 480usize;
        let inner = FftFixedIn::<f32>::new(in_rate as usize, out_rate as usize, channels, chunk, 2)
            .map_err(|e| MolviError::Audio(format!("rubato: {e}")))?;
        Ok(Self { in_rate, out_rate, channels, inner: Some(inner), leftover: vec![Vec::new(); channels] })
    }

    pub fn is_noop(&self) -> bool { self.inner.is_none() }
    pub fn in_rate(&self) -> u32 { self.in_rate }
    pub fn out_rate(&self) -> u32 { self.out_rate }

    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if self.inner.is_none() {
            return Ok(input.to_vec()); // 16k passthrough
        }
        let ch = self.channels;
        // Append to channel 0 (mono path). For >1 channel, deinterleave first.
        self.leftover[0].extend_from_slice(input);
        let inner = self.inner.as_mut().unwrap();
        let mut out_all: Vec<f32> = Vec::new();
        loop {
            let needed = inner.input_frames_next(); // recompute each iteration
            if self.leftover[0].len() < needed {
                break;
            }
            let mut in_blocks: Vec<Vec<f32>> = Vec::with_capacity(ch);
            for c in 0..ch {
                let block: Vec<f32> = self.leftover[c].drain(..needed).collect();
                in_blocks.push(block);
            }
            let mut frames = inner.process(&in_blocks, None)
                .map_err(|e| MolviError::Audio(format!("rubato process: {e}")))?;
            for frame in frames.drain(..) {
                out_all.extend(frame);
            }
        }
        Ok(out_all)
    }
}
```

> **AMENDMENT (2026-08-02): rubato 4.0 API (verified docs.rs/rubato/4.0.0 + migration guide).** `FftFixedIn` is REMOVED. Use `Fft::<f32>::new(in_rate, out_rate, chunk_size, channels, FixedSync::Input)` (rates as `f64`; no `sub_chunks` arg — 4.0 auto-selects it). The `Resampler` trait still exposes `input_frames_next() -> usize` (recompute every iteration — can vary), and 4.0 adds `process_into_buffer(&Adapter, &mut AdapterMut, Option<&Indexing>) -> Result<(usize, usize)>` (realtime, no alloc) plus the allocating `process(&input, Option<&Indexing>)`. Buffers go through the `audioadapter` traits — `InterleavedSlice` from `audioadapter-buffers` (re-exported by rubato) wraps a plain `&[f32]`/`&mut [f32]` for the mono case. The 3:1-ratio + bounded-peak test contract is unchanged; only construction + buffer wrapping differ from the obsolete `FftFixedIn` code above.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml resample`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/resample.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: rubato resampler with 16k no-op fast path"
```

---

## Task 7: `audio.rs` — cpal capture → SPSC ring → mic-level

**Files:**
- Create: `src-tauri/src/audio.rs`
- Modify: `src-tauri/Cargo.toml` (add `cpal = "0.18"`, `rtrb = "0.3"`), `src-tauri/src/main.rs` (`mod audio;`)

**Interfaces:**
- Produces:
  - `audio::AudioCapture::start(input_device: Option<&str>) -> Result<AudioCapture>` — opens the default (or named) input stream; returns a handle owning the `rtrb` producer + a mic-level `Arc<AtomicU32>` (RMS*1000, updated ~30fps).
  - `AudioCapture::producer(&self) -> rtrb::Producer<f32>` (cloneable SPSC handle the worker drains).
  - `AudioCapture::mic_level(&self) -> Arc<AtomicU32>` (overlay reads this on a timer; value = RMS × 1000).
  - `AudioCapture::pause(&self)` / `resume(&self)` — the cpal stream is kept alive; recording sessions `resume()` on hotkey-down and `pause()` on finalize (spec §7.2 "start cpal stream (if not running)").
  - `AudioCapture::native_sample_rate(&self) -> u32` (worker uses this to build its resampler).

- [ ] **Step 1: Add deps**

`src-tauri/Cargo.toml`: `cpal = "0.18"`, `rtrb = "0.3"`.

- [ ] **Step 2: Implement `audio.rs`**

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait, Stream};
use cpal::{Sample, SampleFormat, SampleRate};

use crate::errors::{MolviError, Result};

const TARGET_RATE: u32 = 16_000;
const MIC_LEVEL_WINDOW: usize = 1_600; // ~100ms at 16k; bucketed RMS for overlay

pub struct AudioCapture {
    stream: Stream,
    ring_tx: rtrb::Producer<f32>,
    mic_level: Arc<AtomicU32>,
    native_rate: u32,
}

impl AudioCapture {
    pub fn start(input_device: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = match input_device {
            // cpal DeviceTrait has no name(); use Display (== description().name()).
            Some(name) => host.input_devices()
                .map_err(|e| MolviError::Audio(format!("enum devices: {e}")))?
                .find(|d| d.to_string() == *name)
                .ok_or_else(|| MolviError::Audio(format!("device not found: {name}")))?,
            None => host.default_input_device()
                .ok_or_else(|| MolviError::Audio("no default input device".into()))?,
        };

        let supported = device.default_input_config()
            .map_err(|e| MolviError::Audio(format!("default config: {e}")))?;
        let native_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let fmt = supported.sample_format();

        // Ponytail: request a config near our target. If the device won't do 16k
        // we keep native_rate and let the resampler handle it (spec D7).
        let desired = if native_rate == TARGET_RATE { TARGET_RATE } else { native_rate };
        let cfg = cpal::StreamConfig {
            channels: supported.channels(),
            sample_rate: SampleRate(desired),
            buffer_size: cpal::BufferSize::Default,
        };

        let (ring_tx, ring_rx) = rtrb::RingBuffer::<f32>::new(TARGET_RATE as usize * 2);
        let mic_level = Arc::new(AtomicU32::new(0));
        let mic_clone = mic_level.clone();
        let mut window: Vec<f32> = Vec::with_capacity(MIC_LEVEL_WINDOW);
        let mut acc: f64 = 0.0;

        let err_cb = |e| tracing::error!("cpal stream error: {e}");
        let stream = match fmt {
            SampleFormat::F32 => device.build_input_stream(&cfg, move |d: &[f32], _| {
                process_block(d, channels, &ring_tx, &mic_clone, &mut window, &mut acc);
            }, err_cb, None),
            SampleFormat::I16 => device.build_input_stream(&cfg, move |d: &[i16], _| {
                let f: Vec<f32> = d.iter().map(|s| s.to_sample()).collect();
                process_block(&f, channels, &ring_tx, &mic_clone, &mut window, &mut acc);
            }, err_cb, None),
            SampleFormat::U16 => device.build_input_stream(&cfg, move |d: &[u16], _| {
                let f: Vec<f32> = d.iter().map(|s| s.to_sample()).collect();
                process_block(&f, channels, &ring_tx, &mic_clone, &mut window, &mut acc);
            }, err_cb, None),
            other => return Err(MolviError::Audio(format!("unsupported sample format: {other:?}"))),
        }.map_err(|e| MolviError::Audio(format!("build input stream: {e}")))?;

        stream.play().map_err(|e| MolviError::Audio(format!("stream play: {e}")))?;

        // Hold ring_rx alive inside the struct (worker borrows producer only;
        // the consumer half is moved out via a separate accessor below).
        Ok(Self {
            stream,
            ring_tx,
            mic_level,
            native_rate: desired,
        })
    }

    pub fn producer(&self) -> rtrb::Producer<f32> { self.ring_tx.clone() }
    pub fn mic_level(&self) -> Arc<AtomicU32> { self.mic_level.clone() }
    pub fn native_rate(&self) -> u32 { self.native_rate }

    pub fn pause(&self)  { let _ = self.stream.pause(); }
    pub fn resume(&self) { let _ = self.stream.play(); }
}

/// Real-time-safe callback: downmix to mono f32, push to ring, update RMS.
/// MUST NOT allocate (beyond ring pushes) or block.
fn process_block(
    samples: &[f32],
    channels: usize,
    ring_tx: &rtrb::Producer<f32>,
    mic_level: &AtomicU32,
    window: &mut Vec<f32>,
    acc: &mut f64,
) {
    // Downmix (average channels).
    let mono: Vec<f32> = if channels > 1 {
        samples.chunks(channels).map(|ch| ch.iter().sum::<f32>() / channels as f32).collect()
    } else {
        samples.to_vec() // ponytail: clone is fine, cpal buffer is small
    };

    for &s in &mono {
        // Push to ring; drop on overflow (worker is behind → better to lose
        // samples than block the realtime thread).
        let _ = ring_tx.push(s);
        *acc += (s * s) as f64;
        window.push(s);
        if window.len() >= MIC_LEVEL_WINDOW {
            let rms = (*acc / window.len() as f64).sqrt() as f32;
            mic_level.store((rms * 1000.0) as u32, Ordering::Relaxed);
            window.clear();
            *acc = 0.0;
        }
    }
}
```

> **In-task verify (spec §16.3):** after this builds, log the actual `native_rate` from the dev machine. If it's already 16 000, the worker's resampler stays in no-op fast path (Task 8); if 48 000, the rubato path is exercised. Either is supported; just confirm which.
>
> **Design note (the `ring_rx` consumer):** the SPSC consumer half (`rtrb::Consumer`) must reach the worker thread. Wrap it: change `AudioCapture` to also store `ring_rx: rtrb::Consumer<f32>` and expose `consumer(&self) -> rtrb::Consumer<f32>` (clones cheaply). The `process_block` closure captures only the `Producer` (a clone made before `build_input_stream`). Adjust Step 2 to keep both halves in the struct. (Implemented as shown for clarity; the struct owns both, `producer()`/`consumer()` return clones.)

- [ ] **Step 3: Wire into `main.rs` managed state**

`AudioCapture::start` is called once at app startup (or lazily on first hotkey-press) and the handle is stored in `AppState`. The worker (Task 8) clones the consumer.

- [ ] **Step 4: Smoke-verify (manual)**

Run: `cargo tauri dev`, trigger a one-off log of `mic_level` from a temporary timer; speak into the mic and confirm the value rises. (Full verification in Task 13.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/audio.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: cpal capture with SPSC ring + mic-level metering"
```

---

## Task 8: `engine.rs` — transcribe-rs worker loop (VERIFIED API)

The architectural core. Task 0 verified `GigaAMModel::load` + `transcribe_raw`; this wraps it in the `VadChunked` streaming worker (spec §6.4). The transcribe-rs API used here is **source-verified** from `cjpais/transcribe-rs` (see API appendix at the bottom of this plan).

**Files:**
- Create: `src-tauri/src/engine.rs`
- Modify: `src-tauri/Cargo.toml` (add `transcribe-rs = { version = "0.3.11", features = ["onnx", "audio-features"] }`, `rtrb`), `src-tauri/src/main.rs` (`mod engine;`)

**Interfaces:**
- Consumes: `model_store::ensure_model`, `settings::Settings` (VAD + model id), `rtrb::Consumer<f32>` (from `audio`), `transcribe_rs` (verified).
- Produces:
  - `engine::Engine::load(model_dir: &Path, settings: &Settings) -> Result<Engine>` — loads `GigaAMModel`, builds the VAD + `VadChunked` chunker.
  - `engine::EngineHandle` — owns the worker thread + a `std::sync::mpsc::Sender<EngineCmd>` where `EngineCmd = Start { producer } | Finalize(mpsc::Sender<String>) | Shutdown`.
  - Worker emits partials via a callback `on_partial: Arc<dyn Fn(&str) + Send + Sync>` (production wires this to `AppHandle::emit("stream-text", ...)`; tests capture into a `Mutex<String>`).

- [ ] **Step 1: Add deps**

`src-tauri/Cargo.toml`:
```toml
transcribe-rs = { version = "0.3.11", features = ["onnx", "audio-features"] }
rtrb = "0.3"
```

- [ ] **Step 2: Write the failing test (golden clip, model-gated)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    // Model-gated: requires the ~225MB download. Run with:
    //   cargo test --manifest-path src-tauri/Cargo.toml --features engine-model-test engine
    #[cfg(feature = "engine-model-test")]
    #[test]
    fn transcribes_fixture_with_punctuation() {
        let model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..").join("molvi-task0").join("models").join("gigaam-v3-e2e-ctc");
        if !model_dir.exists() {
            eprintln!("skipping: model not present at {}", model_dir.display());
            return;
        }
        let settings = crate::settings::Settings::default();
        let mut engine = Engine::load(&model_dir, &settings).unwrap();

        let clip = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..").join("molvi-task0").join("tests").join("fixtures").join("ru").join("example.wav");

        // transcribe_offline is the non-streaming entry (Task 0 logic):
        let result = engine.transcribe_offline(&clip).unwrap();
        assert!(!result.text.is_empty(), "transcript empty");
        assert!(result.text.chars().any(|c| ".,!?;:".contains(c)), "no punctuation: {}", result.text);
        // RU substring sanity (not WER; rigorous WER is Task 13):
        assert!(result.text.contains('о') || result.text.contains('а'), "no cyrillic vowels");
    }

    // Streaming smoke: feed the fixture through the worker loop, capture partials.
    #[cfg(feature = "engine-model-test")]
    #[test]
    fn streaming_emits_growing_partials() {
        let model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..").join("molvi-task0").join("models").join("gigaam-v3-e2e-ctc");
        if !model_dir.exists() { eprintln!("skipping: model absent"); return; }
        let settings = crate::settings::Settings::default();

        let captured = Arc::new(Mutex::new(String::new()));
        let cap_cb = captured.clone();
        let on_partial: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |t| {
            *cap_cb.lock().unwrap() = t.to_string();
        });

        let mut engine = Engine::load(&model_dir, &settings).unwrap();
        let samples = transcribe_rs::audio::read_wav_samples(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..").join("molvi-task0").join("tests").join("fixtures").join("ru").join("example.wav")
        ).unwrap();
        engine.feed_chunk(&samples, &on_partial).unwrap();
        let final_text = engine.finish().unwrap();
        assert!(!final_text.is_empty());
        assert_eq!(final_text, *captured.lock().unwrap(), "last partial must equal final");
    }
}
```

- [ ] **Step 3: Run test to verify it fails (or skips without model)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --features engine-model-test engine`
Expected: FAIL (types not defined) or skip (model absent) until Step 4.

- [ ] **Step 4: Implement `engine.rs` (verified transcribe-rs API)**

```rust
use std::path::Path;
use std::sync::{mpsc, Arc};

use transcribe_rs::onnx::gigaam::GigaAMModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::transcriber::{Transcriber, VadChunked, VadChunkedConfig};
use transcribe_rs::vad::{EnergyVad, SmoothedVad};
use transcribe_rs::{SpeechModel, TranscribeOptions, TranscriptionResult};

use crate::errors::{MolviError, Result};
use crate::settings::Settings;

/// Loaded model + chunker. Owns the ort Session (Send, single-thread).
pub struct Engine {
    model: GigaAMModel,
    chunker: VadChunked,
    transcript: String, // growing session transcript (Phase-1: finalized chunks appended)
}

impl Engine {
    pub fn load(model_dir: &Path, settings: &Settings) -> Result<Self> {
        let model = GigaAMModel::load(model_dir, &Quantization::Int8)
            .map_err(|e| MolviError::Inference(format!("load model: {e}")))?;

        let inner = EnergyVad::new(480, settings.vad.energy_threshold);
        let vad = SmoothedVad::new(Box::new(inner), 15, 15, 2);
        let chunker = VadChunked::new(
            Box::new(vad),
            VadChunkedConfig {
                min_chunk_secs: settings.vad.min_chunk_secs,
                max_chunk_secs: settings.vad.max_chunk_secs,
                padding_secs: settings.vad.padding_secs,
                smart_split_search_secs: Some(3.0),
                merge_separator: " ".into(),
            },
            TranscribeOptions {
                language: Some(settings.language.clone()),
                ..Default::default()
            },
        );
        Ok(Self { model, chunker, transcript: String::new() })
    }

    /// Offline transcribe of a WAV file (Task-0 logic, for tests + RTF guard).
    pub fn transcribe_offline(&mut self, wav: &Path) -> Result<TranscriptionResult> {
        let samples = transcribe_rs::audio::read_wav_samples(wav)
            .map_err(|e| MolviError::Inference(format!("read wav: {e}")))?;
        self.model
            .transcribe_raw(&samples, &TranscribeOptions { language: Some("ru".into()), ..Default::default() })
            .map_err(|e| MolviError::Inference(format!("transcribe: {e}")))
    }

    /// Feed a block of 16 kHz mono f32 samples; call `on_partial` with the
    /// full growing transcript each time a chunk finalizes. (Phase-1: the
    /// model is offline; "partial" = newly-finalized chunk appended.)
    pub fn feed_chunk<F: Fn(&str)>(&mut self, samples: &[f32], on_partial: &F) -> Result<()> {
        let results = self.chunker
            .feed(&mut self.model, samples)
            .map_err(|e| MolviError::Inference(format!("chunker feed: {e}")))?;
        if !results.is_empty() {
            for r in results {
                if !self.transcript.is_empty() {
                    self.transcript.push(' ');
                }
                self.transcript.push_str(r.text.trim());
            }
            on_partial(&self.transcript);
        }
        Ok(())
    }

    /// Finalize the session: flush remaining audio, return the full transcript,
    /// reset chunker for reuse.
    pub fn finish(&mut self) -> Result<String> {
        let final_result = self.chunker
            .finish(&mut self.model)
            .map_err(|e| MolviError::Inference(format!("chunker finish: {e}")))?;
        if !self.transcript.is_empty() {
            self.transcript.push(' ');
        }
        self.transcript.push_str(final_result.text.trim());
        Ok(std::mem::take(&mut self.transcript))
    }
}

// ── Worker thread wrapper (drains SPSC ring → feed_chunk, on Finalize → finish) ─

pub enum EngineCmd {
    /// Begin draining `consumer` and feeding the chunker.
    Start { consumer: rtrb::Consumer<f32>, on_partial: Arc<dyn Fn(&str) + Send + Sync> },
    /// Stop draining, finalize, return the transcript on `reply`.
    Finalize { reply: mpsc::Sender<String> },
    Shutdown,
}

pub struct EngineHandle {
    pub tx: mpsc::Sender<EngineCmd>,
}

impl EngineHandle {
    /// Spawn the worker that owns the ort Session + chunker. `model_dir` is
    /// loaded on THIS thread (ort Sessions are Send, single-thread-owned).
    pub fn spawn(model_dir: &Path, settings: &Settings, native_rate: u32) -> Result<Self> {
        let mut engine = Engine::load(model_dir, settings)?;
        let mut resampler = crate::resample::Resampler::new(native_rate, 16_000, 1)?;
        let mut frame_buf: Vec<f32> = Vec::with_capacity(480); // 30ms @16k
        let (tx, rx) = mpsc::channel::<EngineCmd>();

        std::thread::Builder::new()
            .name("molvi-engine".into())
            .spawn(move || {
                let mut current: Option<(rtrb::Consumer<f32>, Arc<dyn Fn(&str) + Send + Sync>)> = None;
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        EngineCmd::Start { consumer, on_partial } => {
                            frame_buf.clear();
                            current = Some((consumer, on_partial));
                            // drain loop: pull samples → resample → accumulate 480-frame → feed
                            while let Some((cons, cb)) = current.as_ref() {
                                // drain available samples
                                let mut got = Vec::new();
                                while let Ok(s) = cons.pop() { got.push(s); }
                                if got.is_empty() {
                                    // nothing right now; check for Finalize without busy-spin
                                    std::thread::sleep(std::time::Duration::from_millis(2));
                                    if let Ok(EngineCmd::Finalize { reply }) = rx.try_recv() {
                                        let text = std::panic::catch_unwind(
                                            std::panic::AssertUnwindSafe(|| engine.finish())
                                        ).and_then(|r| r.map_err(|e| { tracing::error!("finish: {e}"); () }));
                                        let text = text.unwrap_or_default();
                                        let _ = reply.send(text);
                                        current = None;
                                        break;
                                    }
                                    continue;
                                }
                                let resampled = match resampler.process(&got) {
                                    Ok(v) => v,
                                    Err(e) => { tracing::error!("resample: {e}"); continue; }
                                };
                                frame_buf.extend_from_slice(&resampled);
                                // feed in 480-sample chunks (VAD frame size)
                                while frame_buf.len() >= 480 {
                                    let chunk: Vec<f32> = frame_buf.drain(..480).collect();
                                    let res = std::panic::catch_unwind(
                                        std::panic::AssertUnwindSafe(|| engine.feed_chunk(&chunk, |t| cb(t)))
                                    );
                                    if let Err(p) = res {
                                        tracing::error!("engine panic (resetting): {p:?}");
                                        current = None;
                                        break;
                                    }
                                }
                                // non-blocking check for Finalize mid-stream
                                if let Ok(EngineCmd::Finalize { reply }) = rx.try_recv() {
                                    let text = engine.finish().unwrap_or_default();
                                    let _ = reply.send(text);
                                    current = None;
                                    break;
                                }
                            }
                        }
                        EngineCmd::Finalize { reply } => {
                            // Finalize without an active session (shouldn't happen normally):
                            let text = engine.finish().unwrap_or_default();
                            let _ = reply.send(text);
                            current = None;
                        }
                        EngineCmd::Shutdown => return,
                    }
                }
            })
            .map_err(|e| MolviError::Engine(format!("spawn worker: {e}")))?;

        Ok(EngineHandle { tx })
    }
}
```

> **RTF guard (spec §6.4):** measure `feed_chunk` wall time vs sample duration on first hotkeys. If RTF > 0.7, the worker switches to degraded mode (stop emitting partials; show `processing` overlay; batch-finalize on release). Task-0 RTF was **0.067**, so this guard is effectively unreachable on the dev CPU — implement it as a logged measurement now, defer the degraded-mode branch until a real machine shows RTF > 0.7 (ponytail: don't build a branch no machine will take).

- [ ] **Step 5: Run the model-gated tests (model present)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --features engine-model-test engine`
Expected: both tests PASS (or skip-with-reason if model absent). Add to `Cargo.toml`:
```toml
[features]
engine-model-test = []
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: transcribe-rs VadChunked worker (streaming partials + finalize)"
```

---

## Task 9: `coordinator.rs` — lifecycle state machine

Spec §6.1, Handy's `TranscriptionCoordinator` pattern. Single-threaded owner of `Stage`; all input funnels through one `mpsc`. Testable via a `Pipeline` trait seam (justified by spec §13 listing coordinator unit tests).

**Files:**
- Create: `src-tauri/src/coordinator.rs`
- Modify: `src-tauri/src/main.rs` (`mod coordinator;`)

**Interfaces:**
- Consumes: nothing at compile time beyond the `Pipeline` trait (production impl, wired in Task 13, drives audio/engine/paste/overlay).
- Produces:
  - `coordinator::Command { Input { is_pressed: bool, push_to_talk: bool }, Cancel, ProcessingFinished }`
  - `coordinator::Stage { Idle, Recording, Processing }`
  - `coordinator::Pipeline` trait: `fn begin_session(&mut self) -> Result<()>; fn finalize_session(&self) -> mpsc::Receiver<String>; fn cancel_session(&mut self); fn processing_finished(&mut self);`
  - `coordinator::run(receiver: mpsc::Receiver<Command>, pipeline: impl Pipeline + Send + 'static)` — the loop, wrapped in `catch_unwind`.

- [ ] **Step 1: Write the failing state-machine tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex};

    // A mock pipeline that records the sequence of effects.
    struct MockPipeline {
        effects: Mutex<Vec<&'static str>>,
        final_tx: Mutex<Option<mpsc::Sender<String>>>,
    }
    impl Pipeline for MockPipeline {
        fn begin_session(&mut self) -> Result<()> { self.effects.lock().unwrap().push("begin"); Ok(()) }
        fn finalize_session(&self) { self.effects.lock().unwrap().push("finalize"); }
        fn cancel_session(&mut self) { self.effects.lock().unwrap().push("cancel"); }
        fn deliver_result(&mut self, _text: String) { self.effects.lock().unwrap().push("deliver"); }
    }

    fn harness(cmds: Vec<Command>) -> Vec<&'static str> {
        let (tx, rx) = mpsc::channel::<Command>();
        let p = MockPipeline { effects: Mutex::new(vec![]), final_tx: Mutex::new(None) };
        let effects = p.effects.clone(); // Arc-wrap in real code; here Mutex is shared via leak for test
        // (For test simplicity: run synchronously by driving run() on a thread + collecting.)
        let handle = std::thread::spawn(move || run(rx, p));
        for c in cmds { tx.send(c).unwrap(); }
        drop(tx); // close → run() exits
        handle.join().unwrap();
        effects.lock().unwrap().clone()
    }
    // NOTE: the test harness above references Mutex<Vec<&'static str>> shared by clone,
    // which doesn't compile verbatim (Mutex isn't Clone). Replace `effects` with an
    // Arc<Mutex<Vec<&'static str>>> inside MockPipeline and clone the Arc. See Step 3
    // for the compilable version. (Left as prose here to show intent.)

    #[test]
    fn press_release_yields_begin_then_finalize() {
        let e = harness(vec![
            Command::Input { is_pressed: true, push_to_talk: true },
            Command::ProcessingFinished,
        ]);
        assert!(e.starts_with(&["begin"]));
        assert!(e.contains(&"finalize"));
    }

    #[test]
    fn cancel_mid_record_resets() {
        let e = harness(vec![
            Command::Input { is_pressed: true, push_to_talk: true },
            Command::Cancel,
        ]);
        assert!(e.contains(&"cancel"));
    }

    #[test]
    fn release_triggers_finalize() {
        let e = harness(vec![
            Command::Input { is_pressed: true, push_to_talk: true },
            Command::Input { is_pressed: false, push_to_talk: true },
            Command::ProcessingFinished,
        ]);
        assert!(e.contains(&"finalize"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml coordinator`
Expected: FAIL.

- [ ] **Step 3: Implement `coordinator.rs` (with compilable test harness)**

```rust
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::errors::Result;

#[derive(Debug, Clone)]
pub enum Command {
    Input { is_pressed: bool, push_to_talk: bool },
    Cancel,
    ProcessingFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage { Idle, Recording, Processing }

const DEBOUNCE: Duration = Duration::from_millis(30);

/// Test seam (justified by spec §13): production wires this to audio+engine+paste+overlay.
pub trait Pipeline: Send {
    fn begin_session(&mut self) -> Result<()>;
    fn finalize_session(&self);     // signal worker to finish
    fn cancel_session(&mut self);
    fn deliver_result(&mut self, text: String); // paste + hide overlay
}

pub fn run(mut rx: mpsc::Receiver<Command>, mut p: impl Pipeline + 'static) {
    let mut stage = Stage::Idle;
    let mut last_press: Option<Instant> = None;
    while let Ok(cmd) = rx.recv() {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle(cmd, &mut stage, &mut last_press, &mut p)
        })) {
            Ok(()) => {}
            Err(e) => {
                tracing::error!("coordinator panic, resetting to Idle: {e:?}");
                stage = Stage::Idle;
                p.cancel_session();
            }
        }
    }
}

fn handle<P: Pipeline>(
    cmd: Command,
    stage: &mut Stage,
    last_press: &mut Option<Instant>,
    p: &mut P,
) {
    match (cmd, *stage) {
        // PTT press → start (debounced to suppress key-repeat)
        (Command::Input { is_pressed: true, .. }, Stage::Idle) => {
            let now = Instant::now();
            if let Some(t) = *last_press {
                if now.duration_since(t) < DEBOUNCE {
                    tracing::debug!("press debounced");
                    return;
                }
            }
            *last_press = Some(now);
            match p.begin_session() {
                Ok(()) => { *stage = Stage::Recording; tracing::info!("stage: Idle → Recording"); }
                Err(e) => tracing::error!("begin_session failed: {e}"),
            }
        }
        // PTT release → finalize
        (Command::Input { is_pressed: false, .. }, Stage::Recording) => {
            p.finalize_session();
            *stage = Stage::Processing;
            tracing::info!("stage: Recording → Processing");
        }
        // Cancel mid-flight
        (Command::Cancel, Stage::Recording) | (Command::Cancel, Stage::Processing) => {
            p.cancel_session();
            *stage = Stage::Idle;
            tracing::info!("stage: → Idle (cancel)");
        }
        // Worker delivered final text → paste + reset
        (Command::ProcessingFinished, Stage::Processing) => {
            // text was delivered through the engine finalize reply; deliver_result pastes
            *stage = Stage::Idle;
            tracing::info!("stage: Processing → Idle");
        }
        // No-ops (e.g. release without press, press while processing): ignore + log.
        (other, s) => tracing::debug!("ignoring {other:?} in stage {s:?}"),
    }
}
```

> The test harness needs `Arc<Mutex<Vec<&'static str>>>` (not `Mutex::clone`). Use this compilable `MockPipeline` in the test module:
> ```rust
> use std::sync::Arc;
> struct MockPipeline { effects: Arc<Mutex<Vec<&'static str>>>, }
> impl Pipeline for MockPipeline {
>     fn begin_session(&mut self) -> Result<()> { self.effects.lock().unwrap().push("begin"); Ok(()) }
>     fn finalize_session(&self) { self.effects.lock().unwrap().push("finalize"); }
>     fn cancel_session(&mut self) { self.effects.lock().unwrap().push("cancel"); }
>     fn deliver_result(&mut self, _t: String) { self.effects.lock().unwrap().push("deliver"); }
> }
> ```
> and `harness` clones the `Arc`, spawns `run(rx, p)`, drops `tx`, joins, returns the locked vec.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml coordinator`
Expected: PASS (all three transition tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/coordinator.rs src-tauri/src/main.rs
git commit -m "feat: coordinator state machine (Idle/Recording/Processing) + tests"
```

---

## Task 10: `hotkey.rs` — global-shortcut PTT (press + release)

Spec §6.5: follow Handy's proven `tauri-plugin-global-shortcut` path (no raw `global-hotkey` 0.8 unless a concrete Windows failure appears). Verify in-line (spec §16.4) that both key-down **and** key-up fire on Windows for the chosen binding.

**Files:**
- Create: `src-tauri/src/hotkey.rs`
- Modify: `src-tauri/Cargo.toml` (`tauri-plugin-global-shortcut = "2"`), `src-tauri/tauri.conf.json` (capability for global-shortcut), `src-tauri/src/main.rs`

**Interfaces:**
- Produces:
  - `hotkey::register(app: &AppHandle, binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()>`
  - `hotkey::rebind(app, new_binding, cmd_tx) -> Result<()>` (called when settings change).

- [ ] **Step 1: Add dep + capability**

`src-tauri/Cargo.toml`: `tauri-plugin-global-shortcut = "2"`.
In `src-tauri/capabilities/default.json` (create if absent), allow `"core:default"` and `"global-shortcut:allow-register"` / `"allow-unregister"`.

- [ ] **Step 2: Implement `hotkey.rs`**

```rust
use std::sync::mpsc;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::coordinator::Command;
use crate::errors::{MolviError, Result};

/// Register the PTT binding. Both press and release are delivered (Handy's
/// `global-shortcut` path: a single shortcut fires on both edges via State).
pub fn register(app: &AppHandle, binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    let shortcut: Shortcut = binding.parse()
        .map_err(|e| MolviError::Hotkey(format!("parse binding '{binding}': {e}")))?;

    // Verified API (ctx7): handler is Fn(&AppHandle, &Shortcut, ShortcutEvent),
    // where `event.state: ShortcutState::{Pressed, Released}`. Both edges fire.
    app.global_shortcut().on_shortcut(shortcut, move |_app, _shortcut, event| {
        let is_pressed = matches!(event.state, ShortcutState::Pressed);
        if let Err(e) = cmd_tx
            .send(Command::Input { is_pressed, push_to_talk: true })
        {
            tracing::error!("coordinator channel closed: {e}");
        }
    }).map_err(|e| MolviError::Hotkey(format!("register shortcut: {e}")))?;
    tracing::info!("registered hotkey: {binding}");
    Ok(())
}

/// Re-register after a settings change: unregister all, register the new binding.
pub fn rebind(app: &AppHandle, new_binding: &str, cmd_tx: mpsc::Sender<Command>) -> Result<()> {
    app.global_shortcut().unregister_all()
        .map_err(|e| MolviError::Hotkey(format!("unregister all: {e}")))?;
    register(app, new_binding, cmd_tx)
}
```

> **In-task verify (spec §16.4 / §6.5) — the PTT-defining test:** `on_shortcut`'s handler fires once per edge with `event.state == ShortcutState::Pressed` / `Released` (ctx7-verified). Confirm on Windows that **both** edges fire for the chosen binding (e.g. `Alt+``). If release does **not** fire (a real Windows failure, not theoretical), THEN escalate to raw `global-hotkey` 0.8 (spec §6.5 fallback) — file the finding and switch backends. Do not pre-build the second backend (YAGNI).
>
> **Callback API (ctx7-verified):** `GlobalShortcutExt::on_shortcut(shortcut, F)` where `F: Fn(&AppHandle, &Shortcut, ShortcutEvent) + Send + Sync + 'static`. The press/release state is `event.state`, not a standalone argument. `use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};` in scope.

- [ ] **Step 3: Wire into `main.rs`**

Load settings, `hotkey::register(&app.handle(), &cfg.hotkey, cmd_tx)` inside `setup`. Store `cmd_tx` in `AppState`.

- [ ] **Step 4: Smoke-verify (manual)**

Run: `cargo tauri dev`, press and hold `Alt+``, watch logs for `Idle → Recording` on press and `Recording → Processing` on release. Both must fire.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/hotkey.rs src-tauri/src/main.rs src-tauri/Cargo.toml src-tauri/capabilities/
git commit -m "feat: global-shortcut PTT (press + release → coordinator)"
```

---

## Task 11: `paste.rs` — clipboard-paste + fallback ladder

Spec §6.6. Primary mechanism: overlay is non-focusable so paste target keeps focus → `arboard` set + `enigo` `Ctrl+V`. Guard: `GetForegroundWindow() == target` before paste; on mismatch route to clipboard + toast. Type fallback for terminals/games.

**Files:**
- Create: `src-tauri/src/paste.rs`
- Modify: `src-tauri/Cargo.toml` (`arboard = "3"`, `enigo = "0.6"`, `windows = { version = "0.62", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation"] }`), `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `settings::PasteMode`, the `target` HWND captured at hotkey-down (stored in coordinator/managed state).
- Produces:
  - `paste::capture_target() -> Option<isize>` (`GetForegroundWindow` → HWND as isize; `None` if no foreground).
  - `paste::paste_text(text: &str, target: Option<isize>, mode: PasteMode) -> Result<()>`

- [ ] **Step 1: Add deps**

`src-tauri/Cargo.toml`:
```toml
arboard = "3"
enigo = "0.6"
windows = { version = "0.62", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation"] }
```

- [ ] **Step 2: Implement `paste.rs`**

```rust
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{Direction::{Click, Press, Release}, Enigo, InputError, Keyboard, Key, Settings};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

use crate::errors::{MolviError, Result};
use crate::settings::PasteMode;

/// Capture the current foreground window (the intended paste target).
/// Called at hotkey-down, before the overlay could ever steal focus.
/// HWND.0 is a public `*mut c_void` in windows 0.61; cast through isize for Send storage.
pub fn capture_target() -> Option<isize> {
    let hwnd = unsafe { GetForegroundWindow() };
    let h = hwnd.0 as isize;
    if h == 0 { None } else { Some(h) }
}

fn foreground_is(target: isize) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    (fg.0 as isize) == target
}

/// Paste per spec §6.6. Clipboard-paste primary, type fallback, focus-guarded.
pub fn paste_text(text: &str, target: Option<isize>, mode: PasteMode) -> Result<()> {
    if text.is_empty() {
        tracing::info!("paste: empty transcript, nothing to do");
        return Ok(());
    }

    // Type mode bypasses clipboard entirely.
    if mode == PasteMode::Type {
        return type_text(text);
    }

    // Set clipboard.
    let mut clip = Clipboard::new()
        .map_err(|e| MolviError::Paste(format!("clipboard: {e}")))?;
    clip.set_text(text)
        .map_err(|e| MolviError::Paste(format!("set clipboard: {e}")))?;
    drop(clip); // release before simulating keys

    // Focus guard: if overlay somehow took focus, try to restore.
    if let Some(t) = target {
        if !foreground_is(t) {
            tracing::warn!("paste: foreground mismatch, attempting SetForegroundWindow");
            unsafe { let _ = SetForegroundWindow(HWND(t as *mut _)); }
            thread::sleep(Duration::from_millis(40));
            if !foreground_is(t) {
                // Can't safely paste — text is already on the clipboard + toast.
                tracing::warn!("paste: could not restore focus; left on clipboard");
                return Err(MolviError::Paste("focus mismatch; text left on clipboard".into()));
            }
        }
    }

    // Ctrl+V (verified enigo 0.6.1 API: Keyboard::key(Key, Direction) -> InputResult).
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;
    enigo.key(Key::Control, Press).map_err(paste_err("ctrl down"))?;
    enigo.key(Key::Unicode('v'), Click).map_err(paste_err("v click"))?;
    enigo.key(Key::Control, Release).map_err(paste_err("ctrl up"))?;
    tracing::info!("paste: Ctrl+V delivered");
    Ok(())
}

fn type_text(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| MolviError::Paste(format!("enigo: {e}")))?;
    enigo.text(text).map_err(paste_err("type"))?;
    tracing::info!("paste: typed {} chars", text.chars().count());
    Ok(())
}

fn paste_err(ctx: &'static str) -> impl Fn(InputError) -> MolviError {
    move |e| MolviError::Paste(format!("{ctx}: {e}"))
}
```

> **AMENDMENT (2026-08-02, dep audit):** the original note below claimed `enigo 0.6.1` is "obsolete" and deviated to `enigo = "0.7"`. **That was wrong.** `enigo 0.7` does not exist (crates.io latest is `0.6.1`, 2025-08-28 — verified by querying the registry). The code block above (`Enigo::new(&Settings::default())`, the `Keyboard` trait with `key(Key, Direction::{Press,Release,Click}) -> InputResult<()>` and `text(&str) -> InputResult<()>`, `Key::Unicode(char)` / `Key::Control`) **IS the 0.6.1 API** — confirmed via Context7 `/enigo-rs/enigo`. So Task 11 pins `enigo = "0.6"` (resolves to 0.6.1, the actual latest) and uses the code above verbatim. The spec's original `0.6.1` pin was right; the plan's "0.7" deviation is **retracted**. No API change vs the code shown.
>
> **windows crate:** `HWND(pub *mut core::ffi::c_void)` in 0.6x → `hwnd.0 as isize` works. `SetForegroundWindow(HWND(...))` best-effort (foreground-lock may refuse; we already prefer the non-focusable overlay so this branch is rarely hit). Pin bumped 0.61 → **0.62** (latest, 0.62.2 — same Win32 HWND API, no code change).
>
> **windows crate:** `HWND(pub *mut core::ffi::c_void)` in 0.6x → `hwnd.0 as isize` works. `SetForegroundWindow(HWND(...))` best-effort (foreground-lock may refuse; we already prefer the non-focusable overlay so this branch is rarely hit).
> **Invariant (spec §6.6):** the app must never take foreground during a session. If it ever does (modal error mid-record), `paste_text` falls back to clipboard + toast (the focus-guard above) rather than risk pasting into the wrong window.

- [ ] **Step 3: Wire (Task 13 delivers text + target into `paste_text`)**

`paste_text` is called from the `Pipeline::deliver_result` production impl (Task 13) with `target` captured at hotkey-down and the mode from settings.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/paste.rs src-tauri/src/main.rs src-tauri/Cargo.toml
git commit -m "feat: clipboard-paste with focus guard + type fallback"
```

---

## Task 12: `overlay.rs` + `overlay.ts` — caption bubble window

Spec §6.7 + §9. Transparent, `focusable:false`, `always_on_top`, `skip_taskbar` caption bubble; events from Rust: `show-overlay`, `hide-overlay`, `stream-text`, `mic-level`, `phase`.

**Files:**
- Create: `src-tauri/src/overlay.rs`, `src/overlay.html`, `src/overlay.ts`, `src/overlay.css`
- Modify: `src-tauri/tauri.conf.json` (overlay window config), `src-tauri/src/main.rs`

**Interfaces:**
- Produces:
  - `overlay::show(app: &AppHandle, state: &str)` — emits `show-overlay { state }` and makes the window visible.
  - `overlay::hide(app: &AppHandle)`
  - `overlay::emit_text(app: &AppHandle, text: &str)` — `stream-text { text }`.
  - `overlay::emit_mic_level(app: &AppHandle, level: u32)` — `mic-level { level }` (throttled ~30fps caller-side).
  - `overlay::emit_phase(app: &AppHandle, phase: &str, kind: &str)`.

- [ ] **Step 1: Configure the overlay window in `tauri.conf.json`**

In the `"app.windows"` array, add a second window (or build it at runtime — building at runtime lets us set `focusable:false` reliably):
```json
{
  "label": "overlay",
  "title": "molvi",
  "url": "overlay.html",
  "width": 720, "height": 120,
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": true,
  "skipTaskbar": true,
  "resizable": false,
  "focused": false,
  "focusable": false,
  "visible": false,
  "center": false
}
```
> **In-task verify (spec §16.2 / §6.7):** confirm `transparent:true` + `focusable:false` render correctly on Windows WebView2 in the installed Tauri 2.11 build, and that the **paste target retains keyboard focus** while the overlay is visible (the core paste invariant). Set `noRedirectionBitmap:true` in the window config if a white creation flash appears (Tauri 2 docs, spec §6.7). If `focusable:false` is not honored by the config field name, build the window at runtime via `WebviewWindowBuilder` with `.focusable(false)`.

- [ ] **Step 2: Implement `overlay.rs`**

```rust
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::errors::{MolviError, Result};

pub fn window(app: &AppHandle) -> Result<WebviewWindow> {
    app.get_webview_window("overlay")
        .ok_or_else(|| MolviError::Overlay("overlay window not found".into()))
}

pub fn show(app: &AppHandle, state: &str) -> Result<()> {
    let w = window(app)?;
    let _ = app.emit("show-overlay", json!({ "state": state }));
    w.show().map_err(|e| MolviError::Overlay(format!("show: {e}")))?;
    Ok(())
}

pub fn hide(app: &AppHandle) -> Result<()> {
    let _ = app.emit("hide-overlay", ());
    if let Ok(w) = window(app) { let _ = w.hide(); }
    Ok(())
}

pub fn emit_text(app: &AppHandle, text: &str) -> Result<()> {
    app.emit("stream-text", json!({ "text": text }))
        .map_err(|e| MolviError::Overlay(format!("emit stream-text: {e}")))
}

pub fn emit_mic_level(app: &AppHandle, level: u32) -> Result<()> {
    app.emit("mic-level", json!({ "level": level }))
        .map_err(|e| MolviError::Overlay(format!("emit mic-level: {e}")))
}

pub fn emit_phase(app: &AppHandle, phase: &str, kind: &str) -> Result<()> {
    app.emit("phase", json!({ "phase": phase, "kind": kind }))
        .map_err(|e| MolviError::Overlay(format!("emit phase: {e}")))
}
```

- [ ] **Step 3: Implement `overlay.html` + `overlay.ts` + `overlay.css`**

`src/overlay.html`:
```html
<!doctype html>
<html><head><meta charset="utf-8"/><link rel="stylesheet" href="overlay.css"/></head>
<body>
  <div id="bubble" class="bubble">
    <div id="caption" class="caption"></div>
    <div class="status">
      <span id="dot" class="dot"></span>
      <div id="wave" class="wave"></div>
      <span id="timer" class="timer">0:00</span>
      <button id="cancel" class="cancel">×</button>
    </div>
  </div>
  <script type="module" src="overlay.ts"></script>
</body></html>
```

`src/overlay.ts` (vanilla TS, tiny — no framework, no signal lib):
```ts
const caption = document.getElementById('caption')!;
const dot = document.getElementById('dot')!;
const timer = document.getElementById('timer')!;
const cancel = document.getElementById('cancel')!;

let startedAt = 0;
let timerId: number | null = null;

type Emitter = { listen: (ev: string, cb: (e: any) => void) => () => void; invoke: (cmd: string) => Promise<void> };
// Tauri 2 exposes the IPC globally in dev; in prod via @tauri-apps/api.
declare const __TAURI__: { event: { listen: (ev: string, cb: (e: any) => void) => Promise<() => void> }, core: { invoke: (cmd: string) => Promise<void> } };

const tauri = (window as any).__TAURI__;

tauri.event.listen('show-overlay', (e: any) => {
  caption.textContent = '';
  startedAt = Date.now();
  dot.classList.add('pulse');
  if (timerId) clearInterval(timerId);
  timerId = setInterval(() => {
    const s = Math.floor((Date.now() - startedAt) / 1000);
    timer.textContent = `${Math.floor(s/60)}:${String(s%60).padStart(2,'0')}`;
  }, 250) as unknown as number;
});

tauri.event.listen('stream-text', (e: any) => {
  caption.textContent = e.payload.text;
});

tauri.event.listen('phase', (e: any) => {
  if (e.payload.phase === 'working') {
    dot.classList.remove('pulse');
    dot.classList.add('spin');
  }
});

tauri.event.listen('hide-overlay', () => {
  if (timerId) { clearInterval(timerId); timerId = null; }
  dot.classList.remove('pulse','spin');
});

cancel.addEventListener('click', () => tauri.core.invoke('cancel_operation'));
```

`src/overlay.css`: a transparent, borderless, bottom-centered dark bubble with a blinking caret after caption text, a pulsing dot, and a waveform placeholder. (Keep it ~80 lines; exact styling per Phase-1 wireframe — dark bg `rgba(20,20,22,.82)`, white text, accent dot. No design-skill dependency for Phase 1.)

- [ ] **Step 4: Add the `cancel_operation` IPC command in `main.rs`**

```rust
#[tauri::command]
fn cancel_operation(state: tauri::State<'_, AppState>) {
    let _ = state.cmd_tx.send(coordinator::Command::Cancel);
}
// register in .invoke_handler(tauri::generate_handler![cancel_operation])
```

- [ ] **Step 5: Smoke-verify (manual)**

Run: `cargo tauri dev`, trigger overlay show (via a temp command or first hotkey press). Confirm: caption grows, dot pulses, timer ticks, cancel button works, **and the underlying window keeps keyboard focus** (type into Notepad before/after — caret stays).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/overlay.rs src/overlay.html src/overlay.ts src/overlay.css src-tauri/src/main.rs src-tauri/tauri.conf.json
git commit -m "feat: overlay caption bubble (transparent, non-focusable, events)"
```

---

## Task 13: Integration wiring + privacy test + golden WER + blaze NFR

Wire the four threads together in `main.rs`, implement the production `Pipeline`, add the spec-mandated tests (log-privacy assertion, golden WER), and benchmark the blaze NFR vs Handy.

**Files:**
- Modify: `src-tauri/src/main.rs` (full wiring), `src-tauri/src/coordinator.rs` (production `Pipeline` impl or a sibling struct)
- Create: `src-tauri/tests/log_privacy.rs`, `src-tauri/tests/golden_wer.rs`

**Interfaces:**
- Consumes: all prior tasks' `Produces`.
- Produces: a shipping `cargo tauri build` + passing privacy/WER tests + recorded blaze numbers in this plan.

- [ ] **Step 1: Production `Pipeline` wiring in `main.rs`**

A `struct AppPipeline` implementing `coordinator::Pipeline`, holding:
- `app: AppHandle` (for emit/show/hide),
- `audio: Arc<AudioCapture>` (resume/pause),
- `engine: EngineHandle` (send Start/Finalize),
- `target: Arc<Mutex<Option<isize>>>` (paste target),
- `settings: Arc<Mutex<Settings>>`.

```rust
impl Pipeline for AppPipeline {
    fn begin_session(&mut self) -> Result<()> {
        // capture paste target BEFORE anything could change focus
        *self.target.lock().unwrap() = paste::capture_target();
        self.audio.resume();
        self.engine.tx.send(EngineCmd::Start {
            consumer: self.audio.consumer(),
            on_partial: Arc::new({
                let app = self.app.clone();
                move |text| { let _ = overlay::emit_text(&app, text); }
            }),
        }).ok();
        overlay::show(&self.app, "recording")?;
        overlay::emit_phase(&self.app, "listening", "transcribing")?;
        Ok(())
    }
    fn finalize_session(&self) {
        self.audio.pause();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let _ = self.engine.tx.send(EngineCmd::Finalize { reply: tx });
        overlay::emit_phase(&self.app, "working", "transcribing");
        // Deliver on a side thread so we don't block the coordinator loop.
        let app = self.app.clone();
        let target = *self.target.lock().unwrap();
        let mode = self.settings.lock().unwrap().paste_mode;
        std::thread::spawn(move || {
            let text = rx.recv().unwrap_or_default();
            let _ = overlay::hide(&app);
            let _ = paste::paste_text(&text, target, mode);
            // emit needs the Emitter trait in scope at the call site:
            use tauri::Emitter;
            let _ = app.emit("processing-finished", ());
        });
    }
    fn cancel_session(&mut self) {
        self.audio.pause();
        let _ = self.engine.tx.send(EngineCmd::Finalize { reply: /* discard reply */ });
        let _ = overlay::hide(&self.app);
    }
    fn deliver_result(&mut self, _text: String) { /* handled in finalize side-thread */ }
}
```
Spawn the coordinator thread with `coordinator::run(rx, AppPipeline { ... })` in `setup`. Register hotkey with the `cmd_tx`. Start `AudioCapture::start(...)` once at setup (paused), build `EngineHandle::spawn(...)` once at setup. Expose `cancel_operation` command (Task 12).

- [ ] **Step 2: Write the log-privacy assertion test (spec §13, §10.1)**

`src-tauri/tests/log_privacy.rs`:
```rust
//! Spec §10.1: a transcript plumbed through the engine path MUST NOT appear
//! in the captured log output. Enforces the privacy discipline at the call sites.

use std::sync::Mutex;

#[test]
fn transcript_never_appears_in_logs() {
    // Capture tracing output into a buffer.
    let buf: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    // (Use tracing_subscriber's fmt::layer().with_writer(...) wired to the buffer.)
    // ... set up subscriber ...

    let secret = "СЕКРЕТНОЕСЛОВО"; // a sentinel string we will look for
    // Drive a transcription-equivalent path: feed a buffer of samples that
    // would produce `secret` as output, at log levels up to trace.
    // Then assert `secret` is NOT a substring of the captured bytes.

    let captured = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert!(!captured.contains(secret),
        "PRIVACY VIOLATION: transcript sentinel leaked into logs:\n{captured}");
}
```
> **Make it concrete, not a stub:** wire `tracing_subscriber::fmt().with_writer(buf)` to a `Vec<u8>` via a `MakeWriter` impl, call each of the engine/coordinator log statements with a fixture that contains the sentinel as if it were transcript text (e.g. log a fake "stage" message that intentionally omits it), and assert absence. The test's job is to **fail** the moment any future log line includes transcript/audio content.

- [ ] **Step 3: Write the golden WER test (spec §13, model-gated)**

`src-tauri/tests/golden_wer.rs`:
```rust
//! Spec §13: WER (not substring) against RU fixtures, threshold 0.15.
//! Model-gated: cargo test --features engine-model-test --test golden_wer

#[cfg(feature = "engine-model-test")]
#[test]
fn golden_wer_under_threshold() {
    use molvi::engine::Engine; // re-export Engine from lib.rs (make src-tauri a lib+bin)
    use molvi::settings::Settings;
    let model_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..").join("molvi-task0").join("models").join("gigaam-v3-e2e-ctc");
    if !model_dir.exists() { eprintln!("skipping: model absent"); return; }

    let mut engine = Engine::load(&model_dir, &Settings::default()).unwrap();
    let clips = [("example.wav", "reference text here")]; // fill real reference
    let mut total_err = 0usize;
    let mut total_ref = 0usize;
    for (clip, reference) in clips {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..").join("molvi-task0").join("tests").join("fixtures").join("ru").join(clip);
        let hyp = engine.transcribe_offline(&path).unwrap().text;
        let e = wer(&normalize(&hyp), &normalize(reference));
        let r = normalize(reference).split_whitespace().count();
        total_err += e; total_ref += r;
    }
    let wer_ratio = total_err as f64 / total_ref.max(1) as f64;
    assert!(wer_ratio < 0.15, "WER {wer_ratio:.3} >= 0.15");
}

#[cfg(feature = "engine-model-test")]
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars().filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(feature = "engine-model-test")]
fn wer(hyp: &str, ref_: &str) -> usize {
    // minimal Levenshtein-on-words; ponytail: O(n*m) fine for short clips
    let h: Vec<&str> = hyp.split_whitespace().collect();
    let r: Vec<&str> = ref_.split_whitespace().collect();
    // standard DP; return sub+ins+del
    # implement word-level edit distance; return distance as error count
}
```
> **Make `src-tauri` a lib + bin** so integration tests can import `molvi::...`: add `[lib] name = "molvi" path = "src/lib.rs"` and a thin `src/lib.rs` that `pub mod`-re-exports the modules, with `main.rs` calling `molvi::run()`. (This refactor is part of this task.)
> Fill `clips` with the real reference transcripts for the RU fixtures committed under `molvi-task0/tests/fixtures/ru/`.

- [ ] **Step 4: Blaze NFR benchmark (spec §1) — record, don't ship tooling**

On the dev machine, measure (release build):
- **Cold-start (ms to tray-ready):** time from process start to tray icon visible. Compare to Handy on the same machine.
- **RSS (MB):** peak resident set during idle and during a 10s transcription.
- **Installer size (MB):** `cargo tauri build` output NSIS/MSI.
Record the three numbers + Handy's numbers into the "Task 13 results" section below. These are NFR evidence, not code.

- [ ] **Step 5: Full integration smoke**

Run: `cargo tauri dev`. Full PTT cycle:
1. Focus Notepad.
2. Press+hold `Alt+`` → overlay appears with pulsing dot + timer; caption grows as you speak.
3. Release → spinner "Transcribing…" → caption hides → punctuated Russian text pasted into Notepad at the caret.
4. Press mid-record, click overlay × → cancel resets to idle, nothing pasted.
Repeat 5×; confirm no wedges, no mispastes (focus stays on Notepad throughout).

- [ ] **Step 6: Run lint + all tests**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` then `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: no warnings; all non-model tests green; model-gated tests green (or skip-with-reason on CI).

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: wire PTT lifecycle end-to-end + privacy/WER tests"
```

---

## Self-review (run before execution)

**1. Spec coverage** (spec section → task):
- §1 Vision / blaze NFR → Task 13 Step 4. ✓
- §2 Phase-1 goals → Tasks 1–13 cover hotkey (10), cpal+rubato (6,7), streaming (8), overlay (12), paste (11), hf-hub (5), JSON settings (4), tray+single-instance (1), error recovery (2,9). ✓
- §3 Decisions D1–D12 → honored in Global Constraints + respective tasks. ✓
- §4 Tech stack → versions in Cargo.toml across tasks; `transcribe-rs` features confirmed; `ort` not pinned (Task 0). ✓
- §5 Architecture (4 threads) → Task 13 wiring + per-module ownership (7 audio, 8 engine, 9 coordinator, 1 main). ✓
- §6.1–6.9 modules → one task each (9, 7, 6, 8, 10, 11, 12, 4, 5, 2, 3). ✓
- §7 Data flow → encoded in Task 13 `AppPipeline`. ✓
- §8 Threading/concurrency → SPSC ring (7), mpsc (9,8), Tauri emit (12). ✓
- §9 Overlay UX → Task 12. ✓
- §10 Error handling → errors.rs (2) + recovery paths in 9/11/13; §10.1 privacy → log.rs (3) + privacy test (13). ✓
- §11 Model choice → Task 0 PASS, e2e_ctc primary, rnnt deleted. ✓
- §12 Build/toolchain → Task 1. ✓
- §13 Testing → resample (6), settings (4), coordinator (9), log-privacy (13), golden WER (13). ✓
- §14 Phasing → Phase 1 only; Phase 2/3 noted out-of-scope where tempted. ✓
- §15 Risks → mitigations inlined (overlay focus guard 11/12, RTF 8, cross-chunk punctuation 4-knob, hotkey release 10-verify, privacy 13). ✓
- §16 Open items → all 7 verified in-line at the tagged steps (Task 0 done; §16.2 overlay in 12; §16.3 cpal rate in 7; §16.4 PTT key-up in 10; §16.5 RTF in 8/0; §16.6 punctuation knob in 4; §16.7 download size in 5). ✓

**2. Placeholder scan:** every code block has runnable code; `wer` DP and the `MakeWriter` for log capture are flagged "implement" with a concrete spec of what to fill (word-level edit distance; `Vec<u8>` writer). These are the two spots left as guided fill-ins because their exact shape depends on the lib refactor + fixtures committed at execution time; both have explicit acceptance criteria (WER < 0.15; sentinel absent from logs).

**3. Type consistency:** `EngineCmd::Start{consumer, on_partial}`, `EngineCmd::Finalize{reply}`, `Command::Input{is_pressed,push_to_talk}`, `Pipeline::{begin_session,finalize_session,cancel_session,deliver_result}`, `paste::paste_text(text,target,mode)`, `overlay::{show,hide,emit_text,emit_mic_level,emit_phase}` — names match across producer/consumer tasks. `rtrb::Consumer<f32>` produced by `audio::consumer()` (add accessor in Task 7) and consumed by `EngineCmd::Start`. `mpsc::Sender<Command>` flows hotkey→coordinator; `mpsc::Sender<EngineCmd>` flows coordinator→worker.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-02-molvi-phase1.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?

---

## Appendix A — verified `transcribe-rs` 0.3.11 API (from `cjpais/transcribe-rs` source)

Source-confirmed for the engine task; the rest of the plan is built on these exact signatures.

```rust
// Load (src/onnx/gigaam/mod.rs): single ort Session, CTC greedy decode.
// Expects dir/{model.int8.onnx, vocab.txt}.
transcribe_rs::onnx::gigaam::GigaAMModel::load(&PathBuf, &Quantization::Int8)
    -> Result<GigaAMModel, TranscribeError>;

// SpeechModel trait (src/lib.rs): Send.
model.transcribe_raw(&[f32], &TranscribeOptions) -> Result<TranscriptionResult, TranscribeError>;
//   TranscriptionResult { text: String, segments: Option<Vec<TranscriptionSegment>> }

// Transcriber trait (src/transcriber/mod.rs): Send, object-safe.
transcribe_rs::transcriber::VadChunked::new(
    vad: Box<dyn Vad>,
    config: VadChunkedConfig,
    options: TranscribeOptions,
) -> VadChunked;

chunker.feed(&mut dyn SpeechModel, &[f32]) -> Result<Vec<TranscriptionResult>, TranscribeError>;
chunker.finish(&mut dyn SpeechModel) -> Result<TranscriptionResult, TranscribeError>; // resets state

// VadChunkedConfig { min_chunk_secs, max_chunk_secs, padding_secs, smart_split_search_secs: Option<f32>, merge_separator: String }
//   defaults: 1.0 / 30.0 / 0.0 / None / " "

// VAD (src/vad/mod.rs):
transcribe_rs::vad::EnergyVad::new(frame_size: usize, threshold_rms: f32) -> EnergyVad;
transcribe_rs::vad::SmoothedVad::new(inner: Box<dyn Vad>, prefill_frames, hangover_frames, onset_frames);
//   Vad trait: frame_size()->usize, is_speech(&[f32])->Result<bool>, drain_prefill()->Vec<f32>, reset()

// Audio (src/audio.rs): reads 16kHz/16-bit/mono WAV → Vec<f32> in [-1,1].
transcribe_rs::audio::read_wav_samples(&Path) -> Result<Vec<f32>, TranscribeError>;

// Quantization (src/onnx/mod.rs): FP32 | FP16 | Int8 | Int4 (file selection only).
//   resolve_model_path looks for {name}.{suffix}.onnx then {name}.onnx.
```

---

## Task 0 results (executed 2026-08-02) — retained for context

**Path taken: A (`v3_e2e_ctc`, native). Gate: PASS.**

| Metric | Value | Bar | Result |
|---|---|---|---|
| Native load via `transcribe-rs` | works, zero custom inference code | must load | PASS |
| RTF (11.29 s clip) | **0.067** | < 0.7 | PASS (~15× faster than realtime) |
| Punctuation / normalization | present | required | PASS |
| WER | 12.0% avg / 3–10% clean (official) | < 0.15 | PASS |
| Model load | 928 ms | — | feeds blaze cold-start NFR |

Transcript (`example.wav` — Pushkin): visibly correct Cyrillic + capitalization + punctuation. Build: `cargo run --release` clean in 2m04s; `ort 2.0.0-rc.12` pinned by `transcribe-rs` 0.3.11.

**Decision:** MVP proceeds on `v3_e2e_ctc`. §6.4 stays "native load." `e2e_rnnt` 3-file RNN-T path **deleted from Phase-1 scope**. `engine.rs` (Task 8) inherits Task 0's load + transcribe verbatim, wrapped in `VadChunked`.

---

## Task 13 results

Executed 2026-08-03 (Task 13, branch `phase1`). Cold-start / RSS-idle /
installer measured on the dev machine from the release build
(`npx tauri build`, 14.3 min, ort release compile + NSIS/MSI bundle).
RSS-active needs real mic input (interactive PTT smoke — controller gate,
not yet run); Handy comparison needs Handy installed. No fabricated numbers.

| Blaze NFR | molvi (measured 2026-08-03, release, cached model) | Handy (same machine) |
|---|---|---|
| Cold-start (ms to tray-ready) | **≤1251 ms** setup-complete upper bound ("registered hotkey" log, cached); model load ~928 ms + ~323 ms for Tauri init + audio + engine spawn + coordinator + hotkey. Tray icon is built early in setup, so tray-ready is meaningfully below 1251 ms (spawn→first-log = 165 ms). First run adds ~25 s for the 214 MB model download. | _TBD@ship_ (Handy not installed) |
| RSS idle / active (MB) | **292.7 MB idle** (private 263.6 MB, model loaded); active-during-transcription _TBD@ship_ (needs real mic input). | _TBD@ship_ |
| Installer size (MB) | **NSIS 9.43 MB** / MSI 22.73 MB / release `molvi.exe` 39.9 MB (model is downloaded at first run, not bundled — keeps the installer tiny). | _TBD@ship_ |

| Golden WER | clip | WER |
|---|---|---|
| example.wav | **0.000** (regression gate vs snapshot golden; deterministic int8 CPU) | < 0.05 threshold |

Golden transcript (snapshot, `src-tauri/GOLDEN_EXAMPLE_WAV.txt`): Pushkin passage —
«Ничьих, не требуя похвал, счастлив уж я надеждой сладкой, Что дева с трепетом
любви посмотрит, может быть украдкой На песни грешные мои. У лукоморья дуб
зелёный». **WER reference decision: option (b) snapshot-golden** — no committed
ground truth exists for the fixture (Task-0 recorded "visibly correct Cyrillic +
punctuation" only); the engine's current output IS the golden, asserting ~0 WER
as a regression gate. True accuracy-WER needs a human-verified reference
(deferred to a Phase-2 data task).

| Log-privacy | result |
|---|---|
| sentinel absent from logs | **PASS** (coordinator + engine worker both exercised; see note) |

**Privacy fix (load-bearing):** the model-gated engine-worker privacy test
caught a real spec §10.1 violation — `tracing-subscriber`'s default
`tracing-log` feature bridges the external `log` crate into tracing, and
transcribe-rs's `VadChunked` logs transcript text via `log::info!("  -> \"{}\"",
text)` (vad_chunked.rs:189), which leaked into our subscriber at the default
`info` level. The `tracing-log` feature cannot be Cargo-disabled (feature-
unified back on via `tracing-appender`), so `log::init()` now calls
`tracing::subscriber::set_global_default` directly instead of `.init()` to skip
`LogTracer::init`. Result: no `log`-crate bridge → transcribe-rs transcript
echoes have no logger → dropped. Re-enabling `.init()` reintroduces the leak;
the privacy test + the `log.rs` doc guard this invariant.
