// Settings type tree — mirrors src-tauri/src/settings.rs EXACTLY (R4).
// Enum string-literal unions match the serde forms documented in the brief:
// paste_mode lowercase; recognition_mode / post_processing.mode snake_case.
// Option<String> -> string | null.

import type { Store } from "./store";

export type PasteMode = "clipboard" | "type" | "replace";
export type RecognitionMode = "push_to_talk" | "toggle" | "command";
export type PostMode = "raw" | "smart" | "polished";

// ponytail: phase3 mirror of settings.rs CommandModeSettings + ProfileEntry.
// Consumed by commands.rs / profiles.rs (Tasks 5/7/8).
export interface CommandModeSettings {
  enabled: boolean;
  hotkey: string | null;
  grammar: string;
}

export interface ProfileEntry {
  exe: string;
  post_mode: PostMode;
  prompt: string | null;
  enabled: boolean;
}

export interface SmartSettings {
  apply_dictionary: boolean;
  fix_case: boolean;
  normalize_whitespace: boolean;
  cleanup_repeated_marks: boolean;
  merge_chunks: boolean;
  remove_duplicate_words: boolean;
  remove_fillers: boolean;
  inter_chunk_punctuation: boolean;
}

export interface PostProcessingSettings {
  mode: PostMode;
  endpoint: string | null;
  api_key: string | null;
  model: string | null;
  prompt: string | null;
  smart: SmartSettings;
}

export interface HistorySettings {
  enabled: boolean;
  max_entries: number;
  max_age_days: number;
}

export interface SoundsSettings {
  enabled: boolean;
  start: string;
  stop: string;
}

export interface UpdaterSettings {
  check_on_startup: boolean;
}

export interface OverlaySettings {
  enabled: boolean;
  show_waveform: boolean;
  show_timer: boolean;
  sounds: SoundsSettings;
}

export interface AudioSettings {
  input_device: string | null;
}

export interface VadSettings {
  min_chunk_secs: number;
  max_chunk_secs: number;
  padding_secs: number;
  energy_threshold: number;
}

export interface EndpointSettings {
  enabled: boolean;
  trailing_silence_ms: number;
}

export interface Settings {
  hotkey: string;
  hotkey_altgr_mirror: boolean;
  recognition_mode: RecognitionMode;
  model: string;
  language: string;
  ui_lang: string;
  paste_mode: PasteMode;
  overlay: OverlaySettings;
  audio: AudioSettings;
  vad: VadSettings;
  endpoint: EndpointSettings;
  post_processing: PostProcessingSettings;
  history: HistorySettings;
  autostart: boolean;
  updater: UpdaterSettings;
  command_mode: CommandModeSettings;
  profiles: ProfileEntry[];
  snippets_enabled: boolean;
  backtrack_parsing: boolean;
  onboarded: boolean;
}

// Dictionary IPC row (dictionary_list payload).
export interface DictEntry {
  entry: string;
  replacement: string;
  created_at: number;
}

// Snippets IPC row (snippet_list payload) — mirrors src-tauri/src/snippets.rs.
// cue = the spoken trigger word; expansion = the stored block pasted on a
// whole-text match. (No created_at — the snippets table doesn't track it.)
export interface SnippetEntry {
  cue: string;
  expansion: string;
}

// Dictionary import-preview IPC row (dictionary_import_preview payload).
export interface ImportPreview {
  path: string;
  total: number;
  new: number;
  conflicts: number;
}

// History IPC row (history_query payload) — mirrors src-tauri/src/history.rs.
export interface HistoryRow {
  id: number;
  created_at: number; // unix ms
  text: string;
  lang: string | null;
  engine: string | null;
  post_mode: string | null;
}

// Model picker IPC row (model_status payload) — mirrors src-tauri/src/model_store.rs.
// snake_case to match the wire format (like DictEntry/HistoryRow); NOT a Settings
// field, so the R4 invariant (TS mirrors settings.rs) is unaffected.
export interface ModelStatus {
  model_id: string;
  cached: boolean;
  size_bytes: number;
}

// Update-check IPC row (check_update payload) — mirrors src-tauri/src/updater.rs
// `CheckResult`. snake_case wire format; NOT a Settings field (R4 unaffected).
export interface CheckResult {
  up_to_date: boolean;
  version: string | null;
  current_version: string;
}

export type State = { settings: Settings | null };
export type SettingsStore = Store<State>;

export interface Section {
  el: HTMLElement;
  cleanup?: () => void;
}
export type SectionBuilder = (store: SettingsStore) => Section;
