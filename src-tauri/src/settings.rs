use serde::{Deserialize, Serialize};

use crate::errors::{MolviError, Result};
use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PasteMode {
    #[default]
    Clipboard,
    Type,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionMode {
    #[default]
    PushToTalk,
    Toggle,
    // ponytail: variant alive only via serialization until Task 5 wires the
    // dedicated command-mode hotkey + grammar parse. The tray PTT↔Toggle
    // cycle (tray.rs) deliberately never selects it.
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PostMode {
    Raw,
    #[default]
    Smart,
    Polished,
}

/// One per-app post-processing profile (spec §1.6/§6.1). `exe` is matched
/// case-insensitively against the foreground window's process basename in
/// Task 8. Empty vec by default; `Default` so a partial profile JSON still
/// parses (#[serde(default)] on every config struct — settings.rs:108-109).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileEntry {
    pub exe: String,
    pub post_mode: PostMode,
    pub prompt: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartToggles {
    pub apply_dictionary: bool,
    pub fix_case: bool,
    pub normalize_whitespace: bool,
    pub cleanup_repeated_marks: bool,
    pub merge_chunks: bool,
    pub remove_duplicate_words: bool,
    pub remove_fillers: bool,
    pub inter_chunk_punctuation: bool,
}
impl Default for SmartToggles {
    fn default() -> Self {
        Self {
            apply_dictionary: true,
            fix_case: true,
            normalize_whitespace: true,
            cleanup_repeated_marks: true,
            merge_chunks: true,
            remove_duplicate_words: true,
            remove_fillers: false,
            inter_chunk_punctuation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PostProcessing {
    pub mode: PostMode,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub smart: SmartToggles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistorySettings {
    pub enabled: bool,
    pub max_entries: u32,
    pub max_age_days: u32,
}
impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            max_entries: 100,
            max_age_days: 7,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundsSettings {
    pub enabled: bool,
    pub start: String,
    pub stop: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdaterSettings {
    pub check_on_startup: bool,
}
impl Default for UpdaterSettings {
    fn default() -> Self {
        Self {
            check_on_startup: true,
        }
    }
}

// `#[serde(default)]` on every struct -> any missing key at any depth picks up
// the field's Default. No version field, no migration: backward compat is not a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlaySettings {
    pub enabled: bool,
    pub show_waveform: bool,
    pub show_timer: bool,
    pub sounds: SoundsSettings,
}
impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_waveform: true,
            show_timer: true,
            sounds: SoundsSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub input_device: Option<String>,
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
        Self {
            min_chunk_secs: 1.0,
            max_chunk_secs: 20.0,
            padding_secs: 0.1,
            energy_threshold: 0.01,
        }
    }
}

/// Trailing-silence auto-stop (toggle mode only). After the user stops
/// speaking, auto-finalize once `trailing_silence_ms` of quiet audio has
/// elapsed. Off by default; PTT ignores it entirely (pipeline builds
/// `auto_stop = None` for PushToTalk).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct EndpointSettings {
    pub enabled: bool,
    pub trailing_silence_ms: u32,
}
impl Default for EndpointSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            trailing_silence_ms: 1200,
        }
    }
}

/// Command-mode config (spec §1.4/§6.1). Dedicated hotkey + deterministic
/// grammar parsed in Task 5. Unread until Task 5 wires `parse()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandModeSettings {
    pub enabled: bool,
    pub hotkey: Option<String>,
    pub grammar: String,
}
impl Default for CommandModeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            hotkey: None,
            grammar: "default".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    // Right-Alt is AltGr (synthesized Ctrl+Alt) and fails MOD_ALT-only
    // RegisterHotKey; this mirrors the hotkey as Ctrl+Alt+` when true.
    pub hotkey_altgr_mirror: bool,
    pub recognition_mode: RecognitionMode,
    pub model: String,
    pub language: String,
    pub ui_lang: String,
    pub paste_mode: PasteMode,
    pub overlay: OverlaySettings,
    pub audio: AudioSettings,
    pub vad: VadSettings,
    pub endpoint: EndpointSettings,
    pub post_processing: PostProcessing,
    pub history: HistorySettings,
    pub autostart: bool,
    pub updater: UpdaterSettings,
    // ponytail: phase3 foundation fields. command_mode is read by Task 5's
    // grammar parse; profiles by Task 8's per-app matcher. Alive via these
    // fields (no orphan structs); no consumer in Task 4 itself.
    pub command_mode: CommandModeSettings,
    pub profiles: Vec<ProfileEntry>,
    // ponytail: phase3 Smart-step gate. Read by postproc::smart_pipeline (Task
    // 8b); off by default so the snippets store is fetched but never consulted
    // until the user opts in. Dead-config discipline: field lands alive, the
    // consuming wiring ships with it.
    pub snippets_enabled: bool,
    // ponytail: phase3 Smart-step gate. Read by postproc::smart_pipeline (Task
    // 8c); off by default so backtrack never fires until the user opts in.
    // Dead-config discipline: field lands alive, the consuming wiring ships
    // with it.
    pub backtrack_parsing: bool,
    // ponytail: phase3 onboarding gate (Task 10). Off by default so first
    // launch shows the onboarding window; flipped to true by `complete_onboarding`
    // IPC (Done/Skip). `#[serde(default)]` regenerates it on existing installs.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Alt+`".into(),
            hotkey_altgr_mirror: false,
            recognition_mode: RecognitionMode::PushToTalk,
            model: "gigaam-v3-e2e-ctc".into(),
            language: "auto".into(),
            ui_lang: "en".into(),
            paste_mode: PasteMode::Clipboard,
            overlay: OverlaySettings::default(),
            audio: AudioSettings::default(),
            vad: VadSettings::default(),
            endpoint: EndpointSettings::default(),
            post_processing: PostProcessing::default(),
            history: HistorySettings::default(),
            autostart: false,
            updater: UpdaterSettings::default(),
            command_mode: CommandModeSettings::default(),
            profiles: Vec::new(),
            snippets_enabled: false,
            backtrack_parsing: false,
            onboarded: false,
        }
    }
}

impl Settings {
    /// Load from the canonical path. Missing -> default; corrupt -> default (logged).
    pub fn load() -> Result<Self> {
        let path = paths::settings_path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json_str(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(MolviError::Settings(format!(
                "read {}: {e}",
                crate::paths::redact_appdata(path)
            ))),
        }
    }

    /// Parse. Missing keys default via `#[serde(default)]`. Invalid JSON -> default.
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
        std::fs::write(path, text).map_err(|e| {
            MolviError::Settings(format!("write {}: {e}", crate::paths::redact_appdata(path)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_have_expected_values() {
        let s = Settings::default();
        assert_eq!(s.hotkey, "Alt+`");
        assert_eq!(s.recognition_mode, RecognitionMode::PushToTalk);
        assert_eq!(s.model, "gigaam-v3-e2e-ctc");
        assert_eq!(s.language, "auto");
        assert_eq!(s.paste_mode, PasteMode::Clipboard);
        assert!((s.vad.min_chunk_secs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn phase2_fields_default() {
        let s = Settings::default();
        assert_eq!(s.recognition_mode, RecognitionMode::PushToTalk);
        assert_eq!(s.post_processing.mode, PostMode::Smart);
        assert!(!s.history.enabled);
        assert_eq!(s.history.max_entries, 100);
        assert_eq!(s.history.max_age_days, 7);
        assert!(!s.autostart);
        assert!(!s.overlay.sounds.enabled);
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
        let s = Settings {
            hotkey: "Ctrl+Space".into(),
            ..Settings::default()
        };
        s.save_to(&dir.join("settings.json")).unwrap();
        let loaded = Settings::load_from(&dir.join("settings.json")).unwrap();
        assert_eq!(loaded.hotkey, "Ctrl+Space");
    }

    #[test]
    fn ui_lang_defaults_to_en() {
        assert_eq!(Settings::default().ui_lang, "en");
    }

    #[test]
    fn ui_lang_missing_from_json_defaults_to_en() {
        let json = r#"{"model":"gigaam-v3-e2e-ctc"}"#;
        let s: Settings = serde_json::from_str(json).expect("parse");
        assert_eq!(s.ui_lang, "en");
    }

    #[test]
    fn endpoint_defaults() {
        let s = Settings::default();
        assert!(!s.endpoint.enabled, "endpoint off by default");
        assert_eq!(s.endpoint.trailing_silence_ms, 1200);
        // `#[serde(default)]` on Settings -> a JSON missing `endpoint` picks up
        // the field's Default (off, 1200ms) rather than failing to parse.
        let s2: Settings = serde_json::from_str(r#"{"hotkey":"Alt+`"}"#).expect("parse");
        assert!(!s2.endpoint.enabled);
        assert_eq!(s2.endpoint.trailing_silence_ms, 1200);
    }

    #[test]
    fn paste_mode_replace_roundtrips() {
        let m = PasteMode::Replace;
        assert_eq!(serde_json::to_string(&m).unwrap(), "\"replace\"");
        assert_eq!(
            serde_json::from_str::<PasteMode>("\"replace\"").unwrap(),
            PasteMode::Replace
        );
        // default unchanged
        assert_eq!(PasteMode::default(), PasteMode::Clipboard);
    }

    #[test]
    fn phase3_fields_default() {
        let s = Settings::default();
        // command_mode
        assert!(!s.command_mode.enabled);
        assert!(s.command_mode.hotkey.is_none());
        assert_eq!(s.command_mode.grammar, "default");
        // profiles
        assert!(s.profiles.is_empty());
        // RecognitionMode::Command exists, serializes as "command", but is NOT default
        assert_eq!(s.recognition_mode, RecognitionMode::PushToTalk);
        assert_eq!(
            serde_json::to_string(&RecognitionMode::Command).unwrap(),
            "\"command\""
        );
        // a settings JSON missing command_mode/profiles still defaults (#[serde(default)])
        let s2: Settings = serde_json::from_str(r#"{"hotkey":"Alt+`"}"#).unwrap();
        assert!(!s2.command_mode.enabled);
        assert!(s2.profiles.is_empty());
        // snippets_enabled (Task 8b): default off, missing-key defaulted.
        assert!(!s.snippets_enabled);
        assert!(!s2.snippets_enabled);
        // backtrack_parsing (Task 8c): default off, missing-key defaulted.
        assert!(!s.backtrack_parsing);
        assert!(!s2.backtrack_parsing);
        // onboarded (Task 10): default off, missing-key defaulted.
        assert!(!s.onboarded);
        assert!(!s2.onboarded);
    }

    fn tempdir_for_test() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("molvi-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}
