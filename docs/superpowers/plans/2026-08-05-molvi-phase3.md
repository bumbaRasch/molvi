# molvi — Phase-3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the three "feels broken" gaps every user notices vs cloud dictation (live caption, auto-stop, replace-selection), then ship the post-processing + UX layer that wins the local-dictation lane — staying 100% local, blaze, no telemetry.

**Architecture:** The Phase-1/2 4-thread design (Tauri main / coordinator / cpal / inference worker) is unchanged. Phase-3 widens the inference worker (Nemotron cache-aware streaming — `parakeet-rs` ships it unused), adds a coordinator trailing-silence timer, and widens the finalize side-thread (per-app profile resolve, command-mode parse, snippet expand). New Rust modules (`commands.rs`, `snippets.rs`, `profiles.rs`) mirror existing molvi patterns; new webview `onboarding` mirrors existing `settings`/`overlay`. No new crates for the must-haves.

**Tech Stack:** Rust (unchanged: tauri 2.11.5, transcribe-rs 0.3.11, parakeet-rs 0.3.7, ort 2.0.0-rc.13, rusqlite 0.40.1, enigo 0.6.1, regex, cpal 0.18.1, windows 0.62.2). Frontend = Vite 8 + TypeScript 7, vanilla TS, no framework. Optional stretch crate: `transcribe-rs` `whisper-cpp` feature behind a non-default cargo feature.

**Spec:** [`docs/superpowers/specs/2026-08-05-molvi-phase3-design.md`](../specs/2026-08-05-molvi-phase3-design.md)

---

## Global Constraints

Copied verbatim from the spec — every task inherits these:

- **App identity:** `molvi`, identifier `com.molvi.app`. App-data dir: `%APPDATA%\com.molvi.app\`.
- **Target platform:** Windows 11 x64, WebView2. MSVC build tools.
- **Toolchain:** rustc **1.97.1**, edition **2024** (`rust-toolchain.toml`).
- **Privacy (HARD RULE, spec §10.1/§8):** NEVER log transcript text, partials, post-processed text, dictionary entries, history rows, snippet cues/expansions, command transcripts, or audio — at any level. Detected lang + foreground exe ARE metadata and may be logged. Enforced by the widened `tests/log_privacy.rs`.
- **No backward compatibility:** clean breaks only. `settings.json` regenerates from `#[serde(default)]` on structural change; `snippets.db`/`profiles.db` may be dropped/recreated.
- **Blaze (one-way ratchet):** no regression for the default RU/PTT/Smart user (RTF 0.029, cold-start 1251 ms, RSS 292 MB, NSIS ≤ ~11 MB). Nemotron streaming RTF ≤ 0.09 steady-state. Per-chunk KV-cache = constant cost (NOT quadratic).
- **Minimal deps, all latest & docs-verified:** zero new crates for the must-haves; optional `transcribe-rs/whisper-cpp` feature only on the stretch task. ctx7/docs.rs mandatory before coding any external API.
- **Clean code (ponytail):** smallest working diff, stdlib/native first, no unrequested abstraction, `// ponytail:` comments for deliberate shortcuts. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo fmt --manifest-path src-tauri/Cargo.toml` clean at every commit. Comments explain WHY, never WHAT.
- **i18n:** ~30 new keys added to `en` (canonical); all 35 sibling locales get the same keys before merge (set-equality invariant from AGENTS.md holds).

---

## Pre-flight: ctx7-verified APIs (do NOT re-derive)

| Concern | Library | What to verify in Task 1 |
|---|---|---|
| Nemotron streaming | `/altunenes/parakeet-rs` | `Nemotron::transcribe_chunk(&[f32]) -> Result<String>`, `Nemotron::from_shared(handle, cache: NemotronEncoderCache)`, `NemotronEncoderCache::with_dims(num_layers, left_context=56, hidden_dim, conv_context)`, `Nemotron::get_transcript()`, `Nemotron::reset()`, chunk size 8960 samples / 560 ms @ 16 kHz |
| Foreground exe | `windows 0.62.2` | `Win32::UI::WindowsAndMessaging::GetForegroundWindow`, `Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW}`, PROCESS_QUERY_LIMITED_INFORMATION |
| enigo key chord | `enigo 0.6.1` | `Key::Other(u32)` for VK codes (matches existing `paste.rs` VK_V pattern), `Key::Control`, `Direction::Press`/`Release` |

Run `npx ctx7@latest docs /altunenes/parakeet-rs "Nemotron transcribe_chunk from_shared NemotronEncoderCache with_dims reset get_transcript"` at Task 1 step 1 — confirm the API shape before writing any code.

---

## File Structure (target — Phase-3 deltas)

```
src-tauri/src/
  main.rs              — unchanged
  lib.rs               — widen: onboarding window, model picker wiring
  paths.rs             — + snippets_db_path(), profiles_db_path()
  errors.rs            — + Snippet, Profile, Command variants
  settings.rs          — WIDEN (spec §6.1): onboarded, endpoint_detection,
                         PasteMode::Replace, recognition_mode + "command",
                         command_mode, backtrack_parsing, profiles,
                         snippets_enabled, stream_partials
  engine_adapter.rs    — ★ NemotronEngine switches to transcribe_chunk path
  engine.rs            — GigaAM unchanged
  coordinator.rs       — ★ trailing-silence timer; ★ command-mode hotkey branch
  pipeline.rs          — ★ profile-resolve at begin_session; ★ command dispatch
                         in finalize side-thread
  postproc.rs          — ★ backtrack Smart step; ★ snippet-expand Smart step
  paste.rs             — + PasteMode::Replace branch
  overlay.rs           — + partial caption event; + edit/paste-failed events
  hotkey.rs            — + command-mode hotkey
  ipc.rs               — + snippet/profile/model-listing/onboarding commands
  commands.rs          — NEW: deterministic RU+EN grammar → KeyChord
  snippets.rs          — NEW: snippets.db CRUD + whole-text apply transform
  profiles.rs          — NEW: profiles.db CRUD + foreground-exe resolver
  model_store.rs       — + discover_models() for the picker
src/                    — frontend deltas
  onboarding.html      — NEW (3-step first-run)
  onboarding.ts        — NEW
  onboarding.css       — NEW
  settings/sections/{snippets,profiles,models}.ts — NEW
  settings/federated-search.ts — NEW (sidebar search)
  settings/ui.ts       — + BreathingDot/RingShimmer/Check components
  overlay.{ts,css}     — ★ redesign (breathe/ring/check + inline edit +
                          paste-failed recovery)
  i18n/locales/*.ts    — + ~30 keys × 36 files
```

Dependency order: **1 (streaming) → 2 (EOU) → 3 (replace) → 4 (foundation: settings/paths/errors) → 5,6,7 (commands/snippets/profiles) → 8 (pipeline wire) → 9 (overlay redesign) → 10 (onboarding) → 11 (federated search) → 12 (history/dict upgrades) → 13 (brand mark) → 14 (model picker) → 15 (gate).**

---

## Task 1: Nemotron cache-aware streaming

**Role:** Switch Nemotron from non-streaming whole-buffer finalize to per-560ms-chunk partials via `parakeet-rs`'s `transcribe_chunk` + `NemotronEncoderCache`. The single highest-ROI Phase-3 change.

**Files:**
- Modify: `src-tauri/src/engine_adapter.rs`, `src-tauri/tests/log_privacy.rs` (widen streaming-substrate assertion)
- Test: inline `#[cfg(test)]` in `engine_adapter.rs`; in-task verify the streaming shape compiles before claiming done.

**Interfaces:**
- Consumes: `parakeet_rs::{Nemotron, NemotronEncoderCache, NemotronHandle}` (verify exact import paths in Step 1).
- Produces: `NemotronEngine` with `feed_chunk` that calls `transcribe_chunk` + `on_partial(get_transcript())`; `finish` flushes 3 zero-padded chunks + `reset()`. Same `SpeechEngine` trait shape — no caller change.

- [ ] **Step 1: ctx7 verify — confirm the streaming API shape**

```bash
npx ctx7@latest docs /altunenes/parakeet-rs "Nemotron transcribe_chunk from_shared NemotronEncoderCache with_dims reset get_transcript streaming example"
```
Confirm: chunk size 8960 samples / 560 ms @ 16k; `NemotronEncoderCache::with_dims(num_layers, left_context=56, hidden_dim, conv_context)` (multilingual 3.5 left_context=56); `transcribe_chunk(&[f32]) -> Result<String>`; `get_transcript() -> String`; `reset()` clears state, preserves `target_lang`. If any signature differs, adapt the code below — that's the AGENTS.md rule, not a placeholder.

- [ ] **Step 2: Failing test (streaming emits growing partials)**

Append to `engine_adapter.rs`'s test module (gated on `--features engine-model-test`, model present):
```rust
#[cfg(feature = "engine-model-test")]
#[test]
fn nemotron_streaming_emits_growing_partials() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..").join("molvi-task0").join("models")
        .join("nemotron-3.5-asr-streaming-0.6b");
    if !dir.exists() { eprintln!("skipping: Nemotron model absent"); return; }
    let mut engine = NemotronEngine::load(&dir, &Settings::default()).unwrap();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![String::new()]));
    let cap = captured.clone();
    let cb: &(dyn Fn(&str) + Send + Sync) = &move |t: &str| {
        cap.lock().unwrap().push(t.to_string());
    };
    // Feed 3 chunks of 8960 samples (silence + a real clip's first 2 chunks).
    let samples: Vec<f32> = vec![0.0; 8960 * 3];
    engine.feed_chunk(&samples, cb).unwrap();
    let (final_text, _lang) = engine.finish().unwrap();
    let parts = captured.lock().unwrap();
    // Streaming must have emitted at least 2 partials (chunks 1 and 2).
    assert!(parts.len() >= 2, "streaming should emit ≥2 partials, got {}", parts.len());
    // The last partial equals (or is a prefix of) the finalize result.
    let last_partial = parts.last().unwrap();
    assert!(!last_partial.is_empty() || !final_text.is_empty(),
            "either the last partial or the final text must be non-empty");
}
```
Run: `cargo test --manifest-path src-tauri/Cargo.toml --features engine-model-test engine_adapter::tests::nemotron_streaming -- --nocapture`
Expected: FAIL or skip (model absent) until Step 3 lands.

- [ ] **Step 3: Rewrite `NemotronEngine::feed_chunk` to streaming**

In `engine_adapter.rs`, replace the current buffer-accumulate implementation with a chunked streamer:
```rust
const NEMOTRON_CHUNK: usize = 8960; // 560 ms @ 16 kHz (parakeet-rs hardcoded)

impl NemotronEngine {
    /// Feed a block of 16 kHz mono f32 samples. Accumulates to the 8960-sample
    /// chunk boundary, calls `model.transcribe_chunk(&chunk)`, and fires
    /// `on_partial(model.get_transcript())` on each boundary cross. Trailing
    /// samples (< one chunk) stay in `frame_buf` until finish() flushes them.
    /// Privacy §10.1: partials flow only to the on_partial callback (Tauri
    /// event); no tracing:: interpolation.
    fn feed_chunk_streaming(
        &mut self,
        samples: &[f32],
        on_partial: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<()> {
        self.frame_buf.extend_from_slice(samples);
        while self.frame_buf.len() >= NEMOTRON_CHUNK {
            let chunk: Vec<f32> = self.frame_buf.drain(..NEMOTRON_CHUNK).collect();
            self.model
                .transcribe_chunk(&chunk)
                .map_err(|e| MolviError::Inference(format!("nemotron chunk: {e}")))?;
            on_partial(self.model.get_transcript().as_str());
        }
        Ok(())
    }
}
```
Adjust the `SpeechEngine` impl: `feed_chunk` calls `feed_chunk_streaming`. `finish` flushes any `< NEMOTRON_CHUNK` remainder by zero-padding to one full chunk, calls `transcribe_chunk`, then `reset()`, and returns `(model.get_transcript_with_tokens()?, detected_lang)` (mirror the existing lang-tag path via `split_lang_tag`).

Add `frame_buf: Vec<f32>` field to `NemotronEngine`. Construct the engine with a real `NemotronEncoderCache` (left_context=56 for multilingual 3.5) via `Nemotron::from_shared(handle, cache)`.

- [ ] **Step 4: Widen `tests/log_privacy.rs` for streaming partials**

Add a `nemotron_streaming_substrates_log_no_transcript` test (model-gated): feed 3 chunks of sentinel-bearing audio through the streaming path, capture logs, assert no sentinel substring leaks. Pattern mirrors the existing `finalize_substrates_log_no_transcript`.

- [ ] **Step 5: Manual smoke + golden WER re-baseline**

Run `cargo tauri dev`; switch to Nemotron engine; speak a sentence. Expected: live caption updates every ~560 ms while speaking, finalizes with the trailing words. Re-run `golden_wer` against the RU fixture via Nemotron to confirm WER didn't regress (arXiv says <0.21% abs degradation).

- [ ] **Step 6: fmt + clippy + commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
git add src-tauri/src/engine_adapter.rs src-tauri/tests/log_privacy.rs
git commit -m "feat(engine): Nemotron cache-aware streaming (live partials via transcribe_chunk)"
```

---

## Task 2: Endpoint detection / auto-stop on trailing silence

**Role:** Toggle mode auto-finalizes after N ms (default 1200) of post-speech silence. PTT mode ignores it.

**Files:**
- Modify: `src-tauri/src/coordinator.rs`, `src-tauri/src/pipeline.rs`, `src-tauri/src/settings.rs`

**Interfaces:**
- Consumes: existing `SmoothedVad` state (already piped through the worker → emit a `Command::Silence { ms: u32 }` when state transitions speech→silence, or have the coordinator track wall-clock since last speech).
- Produces: a new `EndpointSettings { enabled: bool, trailing_silence_ms: u32 }` field on Settings; coordinator auto-sends `Command::Finalize` when the silence timer elapses in toggle mode.

- [ ] **Step 1: Failing test (toggle auto-stops after trailing silence)**
- [ ] **Step 2: Add `EndpointSettings` to settings.rs** — `enabled: false`, `trailing_silence_ms: 1200`. Widen the `Default for Settings`.
- [ ] **Step 3: Implement the silence tracker in the coordinator** — `Stage::Recording` tracks `last_speech_at: Option<Instant>` updated by a periodic `Command::Tick` (the existing 2ms `recv_timeout` cadence doubles as the tick). On tick, if `last_speech_at.is_some_and(|t| t.elapsed() >= trailing_silence_ms)` and mode is Toggle and endpoint enabled, call `p.finalize_session()` and advance to `Processing`.
- [ ] **Step 4: Pipe the speech/silence edge from the worker** — the engine worker already receives speech/silence state via VAD inside `feed_chunk`; emit a `Command::Silence { active: bool }` on edge transitions. The coordinator updates `last_speech_at`.
- [ ] **Step 5: Test passes + fmt + clippy + commit** — `feat(coordinator): trailing-silence auto-stop (toggle mode, opt-in)`

---

## Task 3: Replace-selected-text paste mode

**Role:** `PasteMode::Replace` clears the target's current selection, then pastes.

**Files:**
- Modify: `src-tauri/src/settings.rs`, `src-tauri/src/paste.rs`, `src/settings/sections/text.ts`, `src/i18n/locales/*.ts`

**Interfaces:**
- Consumes: existing `paste::paste_text`, `enigo` for selection-clear.
- Produces: `PasteMode::Replace`; `paste::paste_text` branches on it.

- [ ] **Step 1: Failing test (Replace mode clears selection first)** — mock test on the key-chord sequence.
- [ ] **Step 2: Widen `PasteMode` enum** — add `Replace` variant (`#[serde(rename_all = "lowercase")]` → `"replace"`).
- [ ] **Step 3: Implement `paste_text` Replace branch** — before the clipboard+Ctrl+V path, if mode is Replace: send `Ctrl+A` (select-all) only if no current selection detected via `GetForegroundWindow` + `QM_GETSEL`-style heuristic; for general windows, just `Delete` after `Ctrl+V` would lose content — safer: send `Ctrl+V` after the user has already selected (the existing focus-guard holds; if no selection, Replace falls back to append + a toast). The honest behavior: Replace = "if there's a selection, replace it; else append".
- [ ] **Step 4: UI + i18n** — add a 3rd paste-mode option in `text.ts`; add `text.use_replace` key to `en` + 35 siblings.
- [ ] **Step 5: fmt + clippy + tsc + commit** — `feat(paste): Replace mode — paste over current selection instead of appending`

---

## Task 4: Foundation — settings widen + paths + errors

**Role:** Add the Cargo scaffolding for the new modules. Every later task consumes this.

**Files:**
- Modify: `src-tauri/src/settings.rs`, `src-tauri/src/paths.rs`, `src-tauri/src/errors.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `EndpointSettings`, `CommandModeSettings`, `ProfileEntry`, new `PasteMode::Replace` (already in Task 3), `RecognitionMode::Command`, `paths::snippets_db_path()`, `paths::profiles_db_path()`, `errors::MolviError::{Snippet, Profile, Command}`.

- [ ] **Step 1: Failing test (phase3 fields default)** — append to `settings::tests`.
- [ ] **Step 2: Widen `settings.rs`** — add `EndpointSettings { enabled, trailing_silence_ms }`, `CommandModeSettings { enabled, hotkey: Option<String>, grammar: String }`, `ProfileEntry { exe, post_mode, prompt, enabled }`, `onboarded: bool`, `backtrack_parsing: bool`, `snippets_enabled: bool`, `stream_partials: bool`. All `#[serde(default)]`, no version. Add `RecognitionMode::Command`.
- [ ] **Step 3: `paths.rs`** — `snippets_db_path()` → `app_data_dir()?.join("snippets.db")`; `profiles_db_path()` → `profiles.db`. Inline tests asserting both end with the right filename.
- [ ] **Step 4: `errors.rs`** — `Snippet(String)`, `Profile(String)`, `Command(String)` variants.
- [ ] **Step 5: `lib.rs`** — `mod commands; mod snippets; mod profiles;` (each empty with a doc-comment `// ponytail: filled in Task N` + `#[allow(dead_code)]` until its task).
- [ ] **Step 6: fmt + clippy + commit** — `feat(phase3): foundation — settings widen, snippets/profiles paths, new error variants`

---

## Task 5: `commands.rs` — deterministic RU+EN grammar → enigo KeyChord

**Role:** Parse a finalized transcript against a fixed phrase table; on match, emit an enigo key-chord and skip paste.

**Files:**
- Fill: `src-tauri/src/commands.rs`

**Interfaces:**
- Produces: `commands::parse(text: &str) -> Option<CommandAction>`. `CommandAction` is an enum: `KeyChord(Vec<enigo::Key>, Vec<enigo::Key>)` (press, then release), `PasteSnippet(String)` (delegated to snippets.rs), or `ChangeMode(ModeChange)` (defer).

- [ ] **Step 1: Failing tests (one per command family)** — `parse("new line")` → `Enter`; `parse("новая строка")` → `Enter`; `parse("undo")` → `Ctrl+Z`; `parse("отмена")` → `Ctrl+Z`; `parse("select all")` → `Ctrl+A`; `parse("выделить всё")` → `Ctrl+A`; `parse("delete last word")` → `Ctrl+Backspace`; `parse("удалить последнее слово")` → `Ctrl+Backspace`; `parse("hello world")` → `None` (no match).
- [ ] **Step 2: Implement** — a single `Regex::new(r"(?i)^(new line|newline|новая строка|undo|отмена|redo|повтор|select all|выделить всё|delete last word|удалить последнее слово|copy|копировать|paste|вставить|cut|вырезать|tab|таб|enter|ввод)$")` alternation; on match, return the KeyChord from a static lookup table. The `^...$` anchors force whole-text match (snippets handle cue-inside-sentence). Privacy: text in-memory only.
- [ ] **Step 3: Verify enigo key-chord builder** — `npx ctx7@latest docs /enigo-rs/enigo "Key Control Direction Press Release keyboard sequence"`; confirm `Key::Other(VK)` + `Direction::Press`/`Release` is current. Matches `paste.rs`'s VK_V pattern.
- [ ] **Step 4: Tests pass + fmt + clippy + commit** — `feat(commands): deterministic RU+EN command-mode grammar`

---

## Task 6: `snippets.rs` — snippets.db CRUD + whole-text apply

**Role:** Voice-cue → stored-block expansion. Mirrors `dictionary.rs` shape; the apply transform is whole-text equality (not token substitution).

**Files:**
- Fill: `src-tauri/src/snippets.rs`

**Interfaces:**
- Produces: `Snippets { conn: Connection }`, `Snippets::open() -> Result<Self>`, `add(cue, expansion)`, `remove(cue)`, `list() -> Vec<SnippetEntry>`, `import_csv`/`export_csv`/`import_json`/`export_json` (mirror dictionary.rs), and `expand(&self, text: &str) -> Option<String>` (returns the expansion if text whole-matches a cue, case-insensitive; None otherwise).

- [ ] **Step 1: Failing tests** — `expand` matches whole-text cue case-insensitive; `expand` returns None for non-cue text; CRUD roundtrip; CSV/JSON roundtrip.
- [ ] **Step 2: Implement** — schema from spec §6.3. `expand`: load cues into a `HashMap<String, String>` (lowercased cue → expansion), look up `text.to_lowercase()`. Privacy: in-memory only.
- [ ] **Step 3: Tests pass + fmt + clippy + commit** — `feat(snippets): voice-cue expansion store (whole-text match)`

---

## Task 7: `profiles.rs` — profiles.db CRUD + foreground-exe resolver

**Role:** Per-app post-proc profile routing.

**Files:**
- Fill: `src-tauri/src/profiles.rs`

**Interfaces:**
- Produces: `Profile { exe: String, post_mode: PostMode, prompt: Option<String>, enabled: bool }`, `Profiles { conn: Connection }` with `open()`, `upsert`, `remove`, `list`, and `resolve(foreground_exe: &str) -> Result<Option<Profile>>` (case-insensitive UPPERCASED basename match; fail-open).

- [ ] **Step 1: ctx7 verify** — `windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow` + `Win32::System::Threading::{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION), QueryFullProcessImageNameW}`. Confirm signatures vs `windows 0.62.2`.
- [ ] **Step 2: Failing tests** — `resolve("WINWORD.EXE")` finds a stored profile; `resolve("UNKNOWN.EXE")` returns None; case-insensitive ("winword.exe" matches "WINWORD.EXE"); CRUD roundtrip.
- [ ] **Step 3: Implement** — schema from spec §6.4. `resolve`: SELECT by UPPER(exe) = UPPER(?) ORDER BY enabled DESC LIMIT 1.
- [ ] **Step 4: Add `pub fn foreground_exe() -> Result<String>`** — `unsafe { GetForegroundWindow() }` → `OpenProcess` → `QueryFullProcessImageNameW` → UPPERCASE the basename. Reuses the paste.rs `capture_target` HWND logic but widens the return.
- [ ] **Step 5: Tests pass + fmt + clippy + commit** — `feat(profiles): per-app post-proc profile store + foreground resolver`

---

## Task 8: `pipeline.rs` — wire profile + command + snippet into finalize side-thread

**Role:** The integration task. `begin_session` resolves the active profile; the finalize side-thread dispatches command-mode, then snippet expand, then post-proc with profile override, then paste.

**Files:**
- Modify: `src-tauri/src/pipeline.rs`, `src-tauri/src/coordinator.rs` (command-mode hotkey)

**Interfaces:**
- Consumes: `commands::parse`, `snippets::Snippets`, `profiles::Profiles`.
- Produces: the widened finalize flow.

- [ ] **Step 1: Resolve profile at begin_session** — clone the foreground exe + matching profile into the side-thread closure; if a profile is active, its `post_mode` and `prompt` override the session's `PostProcessing`. Log metadata: `profile resolved: WINWORD.EXE → polished` (or `no profile`).
- [ ] **Step 2: Command dispatch in finalize** — after `rx.recv()`, if `settings.recognition_mode == Command` (or the command-mode hotkey was the trigger), call `commands::parse(&text)`. If `Some(action)`, run the enigo chord and return (no paste, no post-proc, no history). If None, fall through to normal post-proc + paste.
- [ ] **Step 3: Snippet expand in Smart pipeline** — add a new Smart step between `apply_dictionary` and `fix_case`: if `settings.snippets_enabled` and `dict.is_some()`, check `snippets.expand(&text)`. If Some, replace text and skip the rest of Smart (the expansion is final).
- [ ] **Step 4: Backtrack Smart step** — add `smart_step_backtrack(text: &str) -> String` (off by default; gated on `settings.backtrack_parsing`). Regex: `r"(?is)^(.*?)\s*(?:\.\.\.|…|,?\s*no wait,?)\s+(.*)$"` → captures group 2 (the correction). Multi-pass for "X… actually Y… actually Z" → Z.
- [ ] **Step 5: Command-mode hotkey branch in coordinator** — if `Command::Input` arrives on the command-mode hotkey (separate binding), set a session flag `command_mode = true`; finalize dispatches via `commands::parse` instead of paste.
- [ ] **Step 6: Widen `tests/log_privacy.rs`** — cover command parse (no match), snippet expand, profile resolve in one new test.
- [ ] **Step 7: fmt + clippy + commit** — `feat(pipeline): per-app profile + command-mode dispatch + snippet expand + backtrack`

---

## Task 9: Overlay redesign — breathe/ring/check + inline edit + paste-failed recovery

**Role:** Make the overlay world-class (UX research §3).

**Files:**
- Modify: `src/overlay.{ts,css}`, `src-tauri/src/overlay.rs`, `src/i18n/locales/*.ts`

**Interfaces:**
- Consumes: existing `stream-text` / `phase` / `paste-failed` events (Task 1 added streaming to `stream-text`).
- Produces: redesigned overlay with 3-phase motion + edit affordance + recovery buttons.

- [ ] **Step 1: CSS redesign** — drop red/blue color swap. Add `.breathing-dot` (scale 1→1.15 sine.inOut 1.6s infinite), `.ring-shimmer` (rotating conic-gradient sweep), `.check` (SVG checkmark 400ms fade). Honor `prefers-reduced-motion`.
- [ ] **Step 2: Overlay TS state machine** — bind existing `phase` events to the new CSS classes; bind `stream-text` to grow the caption + show the caret.
- [ ] **Step 3: Inline edit affordance** — POLISHING state shows an "edit" button; click → caption becomes `contenteditable=true`; Enter triggers a custom `edit-confirmed` event with the new text; Esc cancels (no paste). Clear caption text from the DOM on hide (§10.1).
- [ ] **Step 4: Paste-failed recovery** — replace the static caption with two buttons: "Paste anyway" (re-attempt paste) + "Open history" (emit `navigate-history`). Both are localized; no transcript text in the labels.
- [ ] **Step 5: i18n** — add `ovl.edit`, `ovl.paste_anyway`, `ovl.open_history` keys to `en` + 35 siblings.
- [ ] **Step 6: fmt (tsc + build) + manual smoke + commit** — `feat(overlay): one-accent three-phase motion + inline edit + paste-failed recovery`

---

## Task 10: Onboarding — 3-step skippable first-run

**Role:** Make first-launch feel premium (UX research §4).

**Files:**
- Create: `src/onboarding.html`, `src/onboarding.ts`, `src/onboarding.css`
- Modify: `src-tauri/tauri.conf.json` (new `"onboarding"` window label), `src-tauri/src/lib.rs` (show onboarding on first launch when `!settings.onboarded`), `src-tauri/src/ipc.rs` (`complete_onboarding` command), `src/i18n/locales/*.ts` (~15 new keys)

**Interfaces:**
- Produces: a self-contained webview shown once; sets `settings.onboarded = true` on completion or skip.

- [ ] **Step 1: New webview config** — `tauri.conf.json` `"windows"` adds `"onboarding"` (label, 720x480, decorations, center, not in tray).
- [ ] **Step 2: lib.rs first-launch gate** — in `setup`, after settings load, if `!settings.onboarded` show the onboarding window; else show the tray as today.
- [ ] **Step 3: Onboarding HTML** — 3 panels (welcome+model, hotkey+mic, first-word) with a `[Skip]` button always visible.
- [ ] **Step 4: Onboarding TS** — step 1 subscribes to model-download progress (existing `model_store` callback); step 2 captures a hotkey (reuse `hotkey.rs` capture logic via a new IPC command `capture_next_hotkey`); step 3 fires a one-shot PTT + shows the result in the edit field from Task 9.
- [ ] **Step 5: IPC `complete_onboarding` command** — sets `settings.onboarded = true`, saves, hides the onboarding window.
- [ ] **Step 6: i18n** — `onboarding.welcome`, `onboarding.privacy_lead`, `onboarding.model_explainer`, `onboarding.download_progress`, `onboarding.hotkey_step`, `onboarding.mic_step`, `onboarding.first_word`, `onboarding.all_set`, `onboarding.skip`, `onboarding.continue`, `onboarding.open_settings`.
- [ ] **Step 7: fmt + clippy + tsc + manual smoke (first launch) + commit** — `feat(onboarding): 3-step skippable first-run with privacy lead`

---

## Task 11: Federated settings search + inline `?` help

**Role:** Make settings navigable (UX research §5).

**Files:**
- Create: `src/settings/federated-search.ts`
- Modify: `src/settings/main.ts`, `src/settings/ui.ts`, `src/settings.css`

**Interfaces:**
- Consumes: existing `history_query`, `dictionary_list`, `snippets_list`, `profiles_list` IPC.
- Produces: a sidebar-top search box filtering sections + surfacing history/dict/snippet matches inline.

- [ ] **Step 1: Search input + section title index** — build an in-memory index of `{sectionId, title, keywords}` at startup. Filter sections live on input.
- [ ] **Step 2: Federated inline results** — on focus + non-empty query, fire debounced (150ms) `history_query(q, 5, 0)` + `dictionary_list` (filter client-side). Render matches below the search box; click navigates.
- [ ] **Step 3: Inline `?` help** — every `SettingsGroup` gets a `?` button toggling a one-line `Alert` hint sourced from a new `help.{sectionId}` i18n key.
- [ ] **Step 4: i18n** — add `search.placeholder`, `search.no_results`, `search.history_matches`, `search.dict_matches`, `help.*` keys (one per section).
- [ ] **Step 5: tsc + build + commit** — `feat(settings): federated search box + inline ? help`

---

## Task 12: History + Dictionary upgrades (and fix `ru-RU` locale bug)

**Role:** Make both first-class (UX research §6).

**Files:**
- Modify: `src/settings/sections/history.ts`, `src/settings/sections/dictionary.ts`, `src/settings/ui.ts` (toaster action button), `src/i18n/locales/*.ts`

- [ ] **Step 1: Fix `history.ts` locale bug** — replace the hardcoded `toLocaleString("ru-RU")` with `getCurrentLang()` mapped to a BCP-47 tag.
- [ ] **Step 2: History full-text row expansion + filter chips** — lazy per-row expand; lang + date-range chips reuse existing `history_query` params.
- [ ] **Step 3: History keyboard nav** — j/k or arrows; Enter repaste; Del delete (with confirmation toast).
- [ ] **Step 4: History bulk select + bulk delete** — checkbox per row, shift-range select, two-step confirm dialog (existing).
- [ ] **Step 5: Toaster action button** — widen `toast()` to accept an optional `action: { label, onClick }`. Used by history + dictionary undo-delete (5s window).
- [ ] **Step 6: Dictionary live filter + undo-delete + import preview** — entry/replacement OR filter; undo-delete toast on remove; import preview shows "N new, M conflicts" before applying.
- [ ] **Step 7: i18n** — add `history.filter_lang`, `history.filter_date`, `history.bulk_delete`, `dict.undo_delete`, `dict.import_preview` keys.
- [ ] **Step 8: tsc + build + commit** — `feat(history,dict): row expand + filters + keyboard nav + bulk + undo-delete + import preview`

---

## Task 13: Brand mark — waveform-`m` monogram

**Role:** One SVG, all surfaces (UX research §7).

**Files:**
- Create: `src/icons/molvi-mark.svg`
- Replace: `src-tauri/icons/icon.ico` + Tauri's icon set (regenerate via `cargo tauri icon`), `src/settings/icons.ts` (logo), `src/favicon.ico`

- [ ] **Step 1: Author the SVG** — 4 rectangles (tile + 3 unequal-height bars forming lowercase "m"); teal `#0E7C86` tile radius 12; white bars. Test at 16px (tray) + 1024px (installer).
- [ ] **Step 2: Regenerate icon set** — `cargo tauri icon path/to/molvi-mark.png` (need a 1024x1024 PNG render first; render via any rasterizer).
- [ ] **Step 3: Wire into settings/About + favicon** — `src/settings/icons.ts` exports `LOGO`; About section renders it; `index.html` `<link rel="icon">`.
- [ ] **Step 4: Manual smoke + commit** — `chore(brand): waveform-m monogram across tray/installer/favicon/about`

---

## Task 14: Multi-model picker + downloader

**Role:** Expose model discovery + download progress in the UI.

**Files:**
- Modify: `src-tauri/src/model_store.rs`, `src-tauri/src/ipc.rs`, `src/settings/sections/recognition.ts` (or new `models.ts`), `src/i18n/locales/*.ts`

- [ ] **Step 1: `model_store::discover_models()`** — read `%APPDATA%\com.molvi.app\models\*` directories; classify each by the known engine shapes (GigaAM = `model.int8.onnx` + `vocab.txt`; Nemotron = the parakeet-rs layout; user-dropped Whisper = `ggml-*.bin`). Return `Vec<ModelManifest>`.
- [ ] **Step 2: IPC `list_models` / `download_model` / `delete_model`** — list returns the manifest; download starts an hf-hub fetch + emits progress events; delete removes the dir (with a confirm).
- [ ] **Step 3: Settings UI** — new "Models" subsection in Recognition (or its own section): list with active-model indicator, download button + progress bar per model, delete with confirm.
- [ ] **Step 4: i18n** — `models.title`, `models.active`, `models.download`, `models.delete`, `models.downloading`, `models.delete_confirm`, `models.unknown_shape`.
- [ ] **Step 5: fmt + clippy + tsc + commit** — `feat(models): discover + download + delete UI (model picker)`

---

## Task 15: Integration — privacy widen + perf remeasure + gate

**STATUS: COMPLETE (2026-08-06, commit `574ac06`).** Concretized by the dedicated spec `docs/superpowers/specs/2026-08-06-phase3-task15-privacy-perf-gate-design.md` (`38a6897`) + plan `docs/superpowers/plans/2026-08-06-phase3-task15-privacy-perf-gate.md` (`beae54f`). Privacy: profile-prompt substrate widened (`POLISHED_PROMPT_SENTINEL`) + `%APPDATA%` redacted from all 10 path-bearing log/error sites (stdlib `paths::redact_appdata`, no `dirs` dep). Gate: 186 lib + 6 log_privacy + clippy + tsc + build all green. Perf: code-level proof (default path byte-untouched) + Session-8 smoke (no regression). See the "Task 15 EXECUTION" subsection in `.superpowers/sdd/2026-08-05-molvi-phase3/progress.md`.

**Role:** Wire end-to-end, prove privacy + blaze, green the CI gate.

**Files:**
- Modify: `src-tauri/tests/log_privacy.rs` (final widen), `AGENTS.md` (Phase-3 NFR table), `docs/superpowers/plans/2026-08-05-molvi-phase3.md` (mark complete)

- [ ] **Step 1: Privacy widen** — run a transcript through streaming + command parse (no match) + snippet expand + per-app profile + history insert; capture the log buffer; assert no transcript/command/snippet/profile-prompt substring appears.
- [ ] **Step 2: Perf remeasure** — RTF (Nemotron streaming, 60s utterance), cold-start to tray (with onboarding shown once), peak RSS, NSIS size. Fill the Phase-3 NFR row in `AGENTS.md`. Assert no regression vs Phase-2 baselines for the default RU/PTT/Smart path.
- [ ] **Step 3: Full gate** — `cargo fmt`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml --lib`, `npx tsc --noEmit`, `npm run build`, `cargo tauri build` (produces signed NSIS).
- [ ] **Step 4: Manual GUI smoke** — first-launch onboarding, Nemotron live caption, toggle-mode auto-stop, replace-selected-text in Notepad, command-mode hotkey ("select all" in Word), snippet cue, per-app profile (Word→Polished), federated settings search, history keyboard nav, undo-delete toast, brand mark in tray + installer.
- [ ] **Step 5: Commit + update AGENTS.md + write session-4 handoff** — `test(phase3): widen privacy + remeasure NFRs + green gate`

---

## Stretch tasks (defer if scope creeps)

- **Task S1: User-loadable Whisper via `transcribe-rs/whisper-cpp` feature** — behind a non-default cargo feature `engine-whisper`. Adds `WhisperEngine` to the trait + picker. ~1 day.
- **Task S2: Command palette (`Ctrl+K`)** — Phase-3.1; deferred per UX research §5.
- **Task S3: Confidence-based caption tint** — blocked on parakeet-rs exposing per-token confidence; defer.

---

## Self-Review

**Spec coverage** — every Phase-3 goal (spec §1, §2) maps to a task: live caption (1), auto-stop (2), replace (3), command-mode (5,8), backtrack (8), per-app (7,8), snippets (6,8), model picker (14), overlay redesign (9), onboarding (10), federated search (11), history/dict upgrades (12), brand (13). Privacy §8 → Task 1 Step 4 + Task 8 Step 6 + Task 15 Step 1. Performance §9 → Task 15 Step 2. UX §7 → Tasks 9-13. ✓

**Placeholder scan** — every step has real code or a concrete action. Task 1 Step 3's code is the verified parakeet-rs streaming shape; if ctx7 in Step 1 returns a different signature, the implementer adapts (AGENTS.md rule, not a placeholder). ✓

**Type consistency** — `EndpointSettings`, `CommandModeSettings`, `ProfileEntry`, `PasteMode::Replace`, `RecognitionMode::Command` defined in Task 4 (and Task 3 for PasteMode), consumed identically in Tasks 5/6/7/8. `commands::parse → Option<CommandAction>`, `snippets::expand → Option<String>`, `profiles::resolve → Result<Option<Profile>>` — names match across producers/consumers. ✓

**Scope** — single cohesive Phase-3 release; one plan is appropriate. Streaming + EOU + replace are tightly coupled (the live-caption UX is incomplete without auto-stop + replace). Command-mode + snippets + per-app are independent string-layer work that ship together with the finalize side-thread integration. UX layer parallelizes. ✓

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-05-molvi-phase3.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks (spec/quality + ctx7 spot-checks at each gate), fast iteration. Required sub-skill: `superpowers:subagent-driven-development`.

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints for review.

Which approach?
