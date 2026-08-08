# molvi — Phase-3 Design Spec

- **Date:** 2026-08-05
- **Status:** Draft, pending user review
- **Extends:** [`2026-08-03-molvi-phase2-design.md`](./2026-08-03-molvi-phase2-design.md) (Phase-2 + i18n + ponytail-audit sweep + recognition/credits fixes shipped on `main` @ `d2dad9b`)
- **Target platform:** Windows 11 x64 (unchanged)
- **Surface:** Tauri 2 desktop application (unchanged)
- **Posture:** 100% local / offline / private. The only network activity remains (a) the optional signed auto-updater check and (b) the optional user-supplied Polished endpoint. **No new network calls in Phase-3.**

---

## 1. Vision (Phase-3)

Phase-2 + i18n shipped a fast, privacy-first, 36-language dictation app with a
mature core. Phase-3 closes the three "feels broken" gaps every user notices
against cloud dictation (Win+H, Wispr Flow), then ships the post-processing +
UX layer that wins the local-dictation lane outright. The moat stays the moat:
**100% local, RTF 0.029, no telemetry, no account**.

Three "must-have to be best" closures:

1. **Live overlay caption via Nemotron streaming** — parakeet-rs 0.3.7 already
   ships cache-aware streaming (`transcribe_chunk` + `NemotronEncoderCache`),
   unused by molvi. Switching the Nemotron path from whole-buffer finalize to
   per-560ms-chunk partials gives a live caption while speaking — every cloud
   competitor's headline feature, achieved **locally**. GigaAM CTC has no
   streaming API → RU-only users keep the current chunk-boundary partial
   behavior (still better than nothing; `VadChunked` already finalizes on
   pauses).
2. **Endpoint detection / auto-stop on silence** — toggle mode today requires
   a second tap to finalize. Win+H, Dragon, Talon, Wispr all auto-stop on
   trailing silence. molvi reuses its existing `SmoothedVad` (450 ms hangover)
   + a new ~1200 ms trailing-silence timer in the coordinator. PTT manual
   release stays the default.
3. **Replace-selected-text mode** — paste over the current selection instead
   of always appending. Dragon/Talon/Wispr replace; molvi appends. A
   `PasteMode::Replace` variant + Win32 selection-replace.

Five local-differentiators no competitor ships:

4. **Lightweight command-mode grammar** — deterministic RU+EN phrase → enigo
   key-chord mapping ("new line", "undo", "select all", "delete last word").
   Talon has the deep version; *no* local PTT app has even a toy version.
   Reuses enigo + post-proc hooks.
5. **Backtrack/correction parsing** — Wispr's most-raved feature, in pure
   post-proc: `"meet at 2… actually 3"` → `"3"`. Fits molvi's deterministic
   pipeline for free.
6. **Per-app post-proc profiles** — route by foreground window (metadata,
   privacy-safe) to a post-proc preset (formal in Word, casual in Discord,
   raw in terminal). Wispr Styles, local.
7. **Snippet/expansion engine** — voice cue → paste a stored formatted block.
   Natural extension of molvi's existing dictionary.
8. **Multi-model picker + downloader** — Handy's strength; molvi already has
   `model_store.rs` + two engines. Expose the picker + drop-in GGUF/ONNX
   discovery + download progress.

UX layer (parallel to the above; visible delight):

9. **Overlay redesign** — one teal accent through three phases (breathing →
   ring-shimmer → check), inline edit-before-paste, paste-failed recovery.
10. **3-step skippable onboarding** — model download + hotkey capture + first
    word. Leads with privacy.
11. **Federated settings search + inline `?` help** — one box filters 9
    sections + history + dictionary; per-group inline help kills buried docs.
12. **History + Dictionary as first-class** — full-text row expansion, lang/date
    filters, keyboard nav, bulk actions, undo-delete toast, live filter,
    import preview. Fixes a real i18n bug en route (`history.ts` hardcoded
    `ru-RU` locale).
13. **Brand mark** — waveform-`m` monogram, single SVG, tray/installer/favicon.

Explicitly **deferred to Phase-4 / moonshot**:

- Talon-lite cursor/editing control ("select last 3 words", structural editing)
- Wake-word always-listening (local KWS — convenience vs privacy/UX tension)
- Hot-word biasing at model level (blocked on transcribe-rs/parakeet-rs
  exposing a biasing hook)
- True GigaAM streaming (no CTC streaming API; would need an upstream
  transcribe-rs contribution or a new RU streaming model like sherpa-onnx
  Zipformer)
- File / meeting transcription mode (MacWhisper's lane — dilutes PTT focus)
- Multi-device sync (cloud — violates the local hard rule)

---

## 2. Goals & Non-Goals

### Phase-3 goals

1. **Live caption while speaking** — Nemotron streaming path, ≤600 ms
   time-to-first-partial, partial interval 560 ms, RTF stays ≤ 0.09 in
   streaming mode (target hardware i5-12450H, all-cores when Nemotron).
2. **Auto-stop on silence** — configurable 800–2000 ms trailing-silence
   timeout (default 1200 ms), opt-in for toggle mode; PTT unchanged.
3. **Replace-selected-text** — `PasteMode::Replace` + Win32 selection-replace
   via enigo `Shift+Left`-select-clear or `WM_PASTE` routing.
4. **Command-mode** — RU+EN grammar, ≥15 starter commands, separate hotkey or
   PTT-modifier; deterministic, no LLM.
5. **Backtrack parsing** — Smart post-proc step (off by default; aggressive).
6. **Per-app profiles** — settings array of `{exe, mode, prompt}`; coordinator
   resolves foreground-exe → profile at begin_session (metadata-only log).
7. **Snippets** — voice cue → stored block, CRUD UI mirroring dictionary.
8. **Multi-model picker** — drop-in ONNX/GGUF discovery under
   `%APPDATA%\com.molvi.app\models\*`, settings UI list, active-model switch
   with restart notice (existing pattern).
9. **Overlay redesign + onboarding + federated search + history/dictionary
    upgrades + brand mark** — see §7.

### Non-Goals (Phase-3)

- **Streaming for GigaAM** — CTC has no encoder-cache reuse; each partial
  re-encodes from t=0 (O(n²)). GigaAM keeps the chunk-boundary finalize.
- **Wake-word / always-listening** — opt-in convenience, but always-on mic
  is a blaze + UX + privacy tension even local; defer.
- **Hot-word biasing / PhraseSets** — needs a parakeet-rs/transcribe-rs API
  surface that doesn't exist today.
- **Cloud anything new** — the local hard rule is the moat. The only network
  is the existing optional updater + optional user-supplied Polished endpoint.
- **macOS / Linux ports** — Phase-3 stays Windows 11 x64.
- **Talon-lite cursor control** — hard, needs per-app selection math; Phase-4.
- **File / meeting transcription** — different product; dilutes PTT focus.

### Cross-cutting Quality Bar

Unchanged from Phase-2 (carries verbatim):

- **Blaze / one-way ratchet.** RTF 0.029 / cold-start 1251 ms / RSS 292 MB /
  NSIS ≤ ~11 MB for the default RU/PTT/Smart user. Nemotron streaming RTF
  ≤ 0.09 for the streaming user. Streaming cost is **per-chunk-constant**
  (KV-cache), so steady-state scales linearly with utterance length, NOT
  quadratically. Profile-verified.
- **No backward compatibility.** Pre-1.0; clean breaks only. `settings.json`
  regenerates from `#[serde(default)]` on structural change.
- **Minimal dependencies, all latest & docs-verified.** Every new crate /
  feature flag targets latest stable as of 2026-08, pinned in the plan with
  exact versions. ctx7 mandatory before coding.
- **Clean code (ponytail).** Smallest diff, stdlib/native first, no unrequested
  abstraction. `cargo clippy -- -D warnings` + `cargo fmt` clean at every
  commit. Comments explain WHY, never WHAT.
- **Privacy HARD RULE (§14).** Never log transcript / partials / audio. The
  Phase-3 widen widens `tests/log_privacy.rs` to cover streaming partials,
  command-mode grammar, snippet expansion, per-app routing. Detected lang +
  foreground exe ARE metadata; transcript text is NOT.

---

## 3. Key Decisions Log

Carries D1–D21 from Phase-1/2. Phase-3 load-bearing decisions:

| #  | Decision | Chosen | Rejected alternative | Rationale |
|----|----------|--------|----------------------|-----------|
| D22 | Streaming path | **parakeet-rs `transcribe_chunk` + `NemotronEncoderCache`** for Nemotron only; GigaAM unchanged | Two-model fast-partial+verify; re-encode growing buffer; ship a streaming RU model (sherpa-onnx Zipformer) | parakeet-rs already ships the API unused; KV-cache = constant per-chunk cost ≈ 35 ms / 560 ms chunk; zero new deps. GigaAM CTC streaming is research-grade. |
| D23 | Endpointing | **Reuse SmoothedVad (450 ms hangover) + new ~1200 ms trailing-silence timer in coordinator** | Silero VAD (new ONNX dep); parakeet-rs `ParakeetEOU` (English-only, second model); WebRTC VAD | SmoothedVad already runs; hangover + silence-timeout is the dictation-industry standard. Silero/ParakeetEOU add deps and don't help RU/multilingual. |
| D24 | Replace-selected-text | **`PasteMode::Replace` + enigo select-all-clear-then-type OR `WM_PASTE` after selection-replace** | Always-append (current); clipboard-monitor auto-replace | Win32 selection-replace is a leaf operation; adding a PasteMode variant reuses the existing mode enum + the existing focus-guard path. |
| D25 | Command-mode | **Deterministic RU+EN grammar → enigo key-chord; separate hotkey or PTT-modifier (NOT continuous parsing)** | NLU library; LLM-based intent detection; continuous always-on command parsing | Talon's `.talon`-grammar approach (string/list-match → action). Zero AI. Phase-4 may add a fuzzy matcher; Phase-3 ships the deterministic starter set. |
| D26 | Backtrack parsing | **Regex-based Smart post-proc step (off by default)** | LLM-based disfluency removal; prosody-based detection | `"X… actually Y"` / `"X, no wait, Y"` patterns are deterministic in text; Smart pipeline already exists; off-by-default = no surprise. |
| D27 | Per-app profiles | **Foreground-exe → profile lookup at begin_session (metadata-only log)** | Window-title-based (privacy-leaky); always-on global prompt | Exe-name is metadata (privacy-safe to log); user-configured table of `{exe, post_mode, prompt}`. |
| D28 | Snippets | **Reuse dictionary.rs shape with a separate `snippets.db`** | Same table as dictionary; in-memory only | Mirrors Phase-2 D16 (separate dictionary.db); distinct lifecycle from dictionary (snippets are voice-shortcut, dictionary is correction). |
| D29 | Multi-model picker | **Discover ONNX/GGUF under `%APPDATA%\com.molvi.app\models\*`, whitelist known engine shapes** | Hardcoded list; arbitrary .onnx via a generic loader | Whitelist = safety + sensible errors; matches Handy's drop-in pattern. |
| D30 | Overlay redesign | **One teal accent through three phases (breathe → ring → check); inline edit before paste; paste-failed recovery** | Red recording / blue spinning (current — reads as error→loading); modal edit window | Single-accent motion matches privacy-calm ethos; contenteditable keeps it native; recovery fixes a dead-end. |
| D31 | Onboarding | **3-step skippable (model + hotkey + first word); never forced** | Forced multi-step wizard; settings-only | Skippable = power-user-friendly; model auto-advance if present. |
| D32 | Settings IA | **Federated search box + inline `?` per group (Phase-3); command palette (Ctrl+K) deferred to Phase-3.1** | Full settings reskin; separate docs site | Search + inline help solves 90% of "where is X"; palette is a stretch. |

---

## 4. Technology Stack (Phase-3 additions)

All versions latest stable as of 2026-08; **every new API/feature-flag is
ctx7/docs.rs-verified at plan-writing time**.

### Rust — features / no new crates for the must-haves

| Layer | Crate | Notes |
|-------|-------|-------|
| Streaming decoder | `parakeet-rs` 0.3.7 (already a dep) | Use `Nemotron::transcribe_chunk` + `NemotronEncoderCache` (560 ms / 8960-sample chunks, ~35 ms/chunk). Verify exact `from_shared` cache-construction signature in-task. |
| Endpointing | existing `transcribe-rs::vad::SmoothedVad` + new coordinator timer | No new VAD dep. |
| Replace selection | existing `windows` (Win32 `keybd_event` for `Shift+Ins`/`Ctrl+V` after selection-replace) + existing `enigo` | No new dep. |
| Command-mode grammar | stdlib `regex` (already a dep) | Phrase alternation + capture; map to `enigo` key-chords. |
| Per-app routing | existing `windows` (`GetForegroundWindow` + `QueryFullProcessImageNameW`) | Already used by `paste::capture_target`; widen to return exe path. |
| Snippets store | `rusqlite` 0.40.1 (already a dep) | New `snippets.db`, same shape as `dictionary.db`. |
| Multi-model discovery | stdlib `std::fs::read_dir` + existing `model_store.rs` | No new dep. |

### Rust — optional (stretch task: user-loadable Whisper)

| Layer | Crate | Notes |
|-------|-------|-------|
| Whisper engine | `transcribe-rs` with `whisper-cpp` feature | Behind a non-default cargo feature `engine-whisper`. Adds `whisper.cpp` build dep when enabled. **Stretch — defer if Phase-3 scope creeps.** |

### Frontend — no new deps

- Vanilla TS (unchanged). Vite 8 + TypeScript 7 (unchanged).
- Overlay redesign: pure CSS keyframe swap + one new `contenteditable`
  affordance.
- Onboarding: new webview `"onboarding"` (mirrors existing `"settings"` +
  `"overlay"` pattern), shown once on first launch (`settings.onboarded`
  bool, `#[serde(default)]`).
- Federated settings search: one `<input>` atop the sidebar, vanilla DOM
  filter; no search library.

### No new i18n keys beyond the obvious

Phase-3 adds ~30 new manifest keys (onboarding.*, command.*, snippet.*,
profile.*, search.*). `en` stays canonical; the 35 sibling locales get the
same keys (machine-translated then human-polished; the audit-clean set-
equality invariant holds).

---

## 5. Architecture (Phase-3 deltas)

The Phase-1/2 4-thread design is **unchanged**. Phase-3 widens the inference
worker path (Nemotron streaming), adds a coordinator timer (EOU), and widens
the finalize side-thread (command-mode parse, snippet expand, per-app profile
resolve).

```
                  ┌─────────────────────────────────────────────────────────────┐
                  │                        TAURI MAIN THREAD                     │
                  │   webview · tray · IPC · events · UPDATER · ONBOARDING       │
                  └──────▲───────────────────────────────▲─────────────▲────────┘
                         │ partial text (NEW)             │ final text  │ DB / post-proc
                         │ stream-text / mic-level / phase│             │ (called from the
                  ┌──────┴─────────────────────────┐    ┌──────────┴───┐    ┌────┴──────────────┐
                  │  INFERENCE WORKER               │    │  FINALIZE     │    │  STORE             │
                  │  (ort Session + model)          │    │  SIDE-THREAD  │    │  rusqlite:          │
                  │  + P-CORE AFFINITY              │    │  profile →    │    │  molvi.db (history)│
                  │  ★ Nemotron: transcribe_chunk   │    │  command? →   │    │  dictionary.db     │
                  │    + NemotronEncoderCache       │    │   key-chord   │    │  snippets.db (NEW)  │
                  │    (★ = Phase-3 change)         │    │  snippet? →   │    │  profiles.db (NEW)  │
                  │  GigaAM: VadChunked (unchanged) │    │   expand      │    └──────────────────────┘
                  └──────▲─────────────────────────┘    │  else: post-  │
                         │ 16 kHz mono f32 (SPSC)         │   proc → paste│
                  ┌──────┴─────────────────────────┐    │   → history   │
                  │  cpal AUDIO (in + out)         │     └────────────────┘
                  │  in: capture → ring            │
                  │  out: start/stop tones         │
                  └────────────────────────────────┘

  Coordinator (thread 2) widens with:
   ★ trailing-silence timer (1200 ms) → auto-Finalize command
   ★ per-app profile resolver at begin_session (QueryFullProcessImageNameW)
```

**Deltas from Phase-2:**

1. **Nemotron streaming** — `NemotronEngine::feed_chunk` accumulates samples
   to the 8960-sample (560 ms) chunk boundary, calls
   `model.transcribe_chunk(&chunk)`, fires `on_partial(model.get_transcript())`.
   `finish` flushes 3 zero-padded chunks + `model.reset()` (preserves
   `target_lang`). GigaAM `feed_chunk` unchanged.
2. **Trailing-silence timer** — coordinator tracks time-since-last-speech
   (driven by the existing VAD state piped through `Command::MicLevel` or a
   new `Command::Silence { ms }`); auto-sends `Finalize` after the
   configurable timeout. PTT mode ignores it.
3. **Per-app profile resolver** — `begin_session` calls a new
   `crate::profile::resolve(&app) -> Option<ProfileId>` that queries
   foreground exe (metadata-only log) and looks up the user's
   `profiles.db`. Profile fields override the session's `PostMode` + prompt.
4. **Command-mode parse** — finalize side-thread calls
   `crate::commands::parse(&final_text) -> Option<KeyChord>` before post-proc;
   if it matches, the chord runs via enigo and paste is skipped. Off by
   default; a `recognition_mode = "command"` setting (or a PTT-modifier)
   enables it.
5. **Snippet expand** — Smart post-proc step (between dictionary apply and
   case fix): look up the whole-text in `snippets.db`, replace with the
   stored block if matched.

---

## 6. Components (Rust modules — Phase-3 deltas)

Phase-2 layout preserved; new files are additive.

```
src-tauri/src/
  main.rs            — unchanged (1-line shim)
  lib.rs             — widen: onboarding window, federated-search wiring
  settings.rs        — WIDEN: streaming-EOU config, PasteMode::Replace,
                       recognition_mode + "command", profiles/snippets arrays,
                       onboarding bool
  paths.rs           — + snippets_db_path(), profiles_db_path()
  errors.rs          — + Snippet, Profile, Command variants
  engine_adapter.rs  — ★ NemotronEngine switches to streaming path
  engine.rs          — GigaAM unchanged
  coordinator.rs     — ★ trailing-silence timer; ★ per-app profile resolve hook
  pipeline.rs        — ★ profile-resolve at begin_session; ★ command-mode dispatch
  postproc.rs        — ★ backtrack step; ★ snippet-expand step
  paste.rs           — + PasteMode::Replace branch (Win32 selection-replace)
  overlay.rs         — + partial caption event (was already wired); + edit affordance
  hotkey.rs          — + command-mode hotkey (separate binding)
  ipc.rs             — + snippet/profile/onboarding commands; federated-search helpers
  commands.rs        — NEW: deterministic RU+EN grammar → enigo KeyChord
  snippets.rs        — NEW: snippets.db CRUD + apply-transform
  profiles.rs        — NEW: profiles.db CRUD + foreground-exe resolver
  model_store.rs     — + discover_models() for the picker

src/                  — frontend Phase-3 deltas
  onboarding.html / onboarding.ts / onboarding.css — NEW webview (3-step)
  settings/sections/onboarding.ts (re-entry from settings)
  settings/sections/snippets.ts   — NEW
  settings/sections/profiles.ts   — NEW
  settings/sections/models.ts     — NEW (model picker)
  settings/federated-search.ts    — NEW (sidebar search box)
  settings/ui.ts                  — + BreathingDot, RingShimmer, Check components
  overlay.ts / overlay.css        — ★ redesign (breathe/ring/check, inline edit,
                                    paste-failed recovery)
  i18n/locales/*.ts               — + ~30 new keys × 36 files
```

### 6.1 `settings.rs` — widened schema (still `#[serde(default)]`, no version)

Additive to Phase-2:

```jsonc
{
  // ... Phase-2 fields (post current audit) ...
  "onboarded": false,                  // first-launch gate
  "endpoint_detection": {              // §1.2 auto-stop
    "enabled": false,                  // opt-in
    "trailing_silence_ms": 1200
  },
  "paste_mode": "clipboard",           // now also: "replace"
  "recognition_mode": "push_to_talk",  // now also: "command"
  "command_mode": {                    // §1.4
    "enabled": false,
    "hotkey": null,                    // separate binding (e.g. "Ctrl+Alt+`")
    "grammar": "default"               // future: user-loaded grammar
  },
  "backtrack_parsing": false,          // §1.5 Smart step, off by default
  "profiles": [                        // §1.6 per-app
    // { "exe": "WINWORD.EXE", "post_mode": "polished", "prompt": null }, ...
  ],
  "snippets_enabled": true,            // §1.7 — db lazily opened
  "stream_partials": true              // §1.1 — Nemotron streaming, on by default
}
```

### 6.2 `commands.rs` — deterministic grammar → enigo KeyChord

```rust
pub struct KeyChord { pub keys: Vec<enigo::Key>, pub hold_ctrl: bool, /* … */ }

/// Match a finalized transcript against the RU+EN grammar. Returns the first
/// match (longest-first alternation, regex-escaped). None → no command.
/// Privacy: text is in-memory only; never logged (§10.1).
pub fn parse(text: &str) -> Option<KeyChord> { /* … */ }
```

Starter grammar (~15 commands): `new line`, `newline`, `н новая строка`;
`undo`, `отмена`; `redo`, `повтор`; `select all`, `выделить всё`; `delete last
word`, `удалить последнее слово`; `copy`, `копировать`; `paste`, `вставить`;
`cut`, `вырезать`; `capital`, `капитал`; `lowercase`, `строчные`; `tab`,
`таб`; `enter`, `ввод`.

### 6.3 `snippets.rs` — `snippets.db` (mirrors dictionary.rs shape)

```sql
CREATE TABLE IF NOT EXISTS snippets (
  cue        TEXT PRIMARY KEY,   -- the phrase to match (case-insensitive)
  expansion  TEXT NOT NULL,      -- the block to paste
  created_at INTEGER NOT NULL
);
```

CRUD + import/export (CSV/JSON) identical to `dictionary.rs`. The apply
transform is whole-text equality (not token-substitution) — the *entire*
finalized transcript matches the cue, not a word within it. This distinguishes
snippets from dictionary entries.

### 6.4 `profiles.rs` — `profiles.db` + foreground resolver

```sql
CREATE TABLE IF NOT EXISTS profiles (
  exe        TEXT PRIMARY KEY,    -- e.g. "WINWORD.EXE" (case-insensitive match)
  post_mode  TEXT NOT NULL,       -- "raw" | "smart" | "polished"
  prompt     TEXT,                -- optional Polished override
  enabled    INTEGER NOT NULL     -- 0/1
);
```

```rust
/// Resolve the foreground window's exe (UPPERCASED basename, metadata-only)
/// against profiles.db. None → no profile (use global settings).
/// Privacy §10.1: exe basename is metadata; never logs window title or text.
pub fn resolve(app: &AppHandle) -> Result<Option<Profile>> { /* … */ }
```

---

## 7. UI / UX deltas

See [`docs/phase-3-ux-research.md`](../../phase-3-ux-research.md) for the full
UX research report. Summary of locked decisions:

### 7.1 Overlay redesign

- **One teal accent through three phases.** RECORDING = breathing dot (scale
  1→1.15 sine.inOut 1.6s) + waveform; POLISHING = ring shimmer sweep; SUCCESS
  = teal check 400ms → hide.
- **Inline edit-before-paste.** POLISHING shows an "edit" affordance; click
  pauses paste, caption becomes `contenteditable`, Enter pastes, Esc cancels.
  DOM is cleared on hide (§10.1 safe).
- **Paste-failed recovery.** Replace dead-end "text is in clipboard" with
  `"Text saved — Paste anyway | Open history"`.
- **Streaming-ready.** Partial event grows the caption; the existing
  `.caption::after` caret already exists.

### 7.2 Onboarding (3 steps, skippable)

1. **Welcome + model fetch.** Privacy-promise lead. Real byte+ETA bar.
   `[Continue] [Skip — set up later]`. Auto-advance if model present.
2. **Hotkey + mic test.** Capture + 2s meter where the breathing dot reacts.
3. **First word.** Hold hotkey, speak; result lands in the §7.1 edit field.
   Soft teal check (no confetti — calm ethos).

### 7.3 Federated settings search

One `<input>` atop the sidebar. Filters the 9 sections by title/keywords AND
surfaces matching history + dictionary pairs inline. Also the keyboard
skip-link. Command palette (`Ctrl+K`) deferred to Phase-3.1.

### 7.4 History + Dictionary upgrades

- History: full-text row expansion, lang/date filter chips, keyboard nav
  (j/k/Enter/Del), bulk select + bulk delete, undo-delete toast. **Also fix
  the hardcoded `toLocaleString("ru-RU")` → use `ui_lang`.**
- Dictionary: live filter, undo-delete toast, import preview
  (`"N new, M conflicts"`).

### 7.5 Brand mark

Waveform-`m` monogram: lowercase "m" whose three vertical strokes are
unequal-height equalizer bars, in a 12px-radius teal tile, white bars. One
SVG, four rectangles, no font dependency. Tray / installer / favicon / about
section.

---

## 8. Privacy (HARD RULE — strengthened for Phase-3)

Carries Phase-1/2 §10.1 verbatim. Phase-3 widens the test:

- **Streaming partials** are transcript-equivalent — NEVER logged at any level.
  The `stream-text` Tauri event carries them to the overlay webview only; no
  `tracing::` call site interpolates them.
- **Command-mode grammar parsing** happens in-memory; the unmatched transcript
  on a no-match is NOT logged.
- **Snippet expansion** — the cue + expansion are user-authored (no concern),
  but the matched transcript text IS privacy-sensitive; not logged.
- **Per-app profile resolution** — the foreground **exe basename** IS metadata
  and may be logged (`profile resolved: WINWORD.EXE → polished`); the window
  **title** is NOT (may contain document names with PII) — never logged.
- **Onboarding** — the third-step "first word" passes through the same
  finalize path; nothing additional is logged.

`tests/log_privacy.rs` widens to cover each new path.

---

## 9. Performance Budget

Phase-2 baselines (must not regress for default RU/PTT/Smart user):

| Metric | Phase-2 baseline | Phase-3 budget |
|---|---|---|
| RTF (GigaAM e2e_ctc) | 0.029 | ≤ 0.029 (GigaAM path unchanged) |
| RTF (Nemotron streaming) | n/a (non-streaming finalize) | ≤ 0.09 steady-state (per-chunk KV-cache) |
| Cold-start to tray-ready | 1251 ms | ≤ ~1350 ms (profiles/snippets lazy; onboarding window shown only when `!onboarded`) |
| RSS | 292 MB | ≤ ~330 MB (NemotronEncoderCache ~7.5 MB/stream; snippets.db/profiles.db connections are tiny) |
| NSIS installer | 9.43 MB | ≤ ~12 MB (new SVG mark; no new bundled assets) |

Streaming is **per-chunk-constant** — a 60 s utterance costs 60 s × RTF 0.09 ≈
5.4 s of CPU spread across the utterance. No quadratic blowup.

---

## 10. Risk Register (Phase-3)

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| parakeet-rs `transcribe_chunk` shape differs from plan (cache-construction args) | Med | Med | ctx7/docs.rs verify at Task 1; the spike binary already proves the API exists. |
| Nemotron streaming WER degrades meaningfully vs non-streaming | Low | Med | arXiv benchmark says −0.21% abs (essentially free); Task 1 golden WER re-baseline catches any regression. |
| Trailing-silence timer cuts off deliberate pauses (think-time) | Med | Med | Default 1200 ms is the dictation industry norm; user-configurable 800–2000 ms; off-by-default. |
| Replace-selected-text misfires (wrong target window) | Med | High | Reuse the existing focus-guard (`paste::capture_target`); on mismatch fall back to clipboard-only with a paste-failed toast (never mis-paste). |
| Command-mode grammar mis-fires inside normal dictation | Med | Med | Off-by-default + separate hotkey/PTT-modifier (D25); deterministic grammar (no false positives in normal speech — the phrases are unambiguous). |
| Onboarding adds friction for power users | Low | Low | Skippable from step 1; auto-advance when model present; never re-shown once `onboarded=true`. |
| Per-app profile exe matching is brittle (UPPERCASE, symlinks) | Low | Low | UPPERCASE basename match (Windows case-insensitive FS); fail-open = no profile. |
| New settings fields bloat the manifest | Low | Low | ~7 new top-level fields; `#[serde(default)]` covers; no migration. |
| Phase-3 scope creeps past ~6 weeks | Med | Med | Stretch goals (Whisper engine, command palette) explicitly deferred; tasks ordered by ROI; cut at Task N if running long. |

---

## 11. Open Items for Phase-3 Kickoff

**Resolved 2026-08-05** by 3 parallel research subagents (competitor audit +
streaming/EOU + UX):

- ✅ **parakeet-rs 0.3.7 streaming API** — `Nemotron::transcribe_chunk` +
  `NemotronEncoderCache` exist and are unused; 560 ms / 8960-sample chunks;
  ~35 ms/chunk on i5-12450H (RTF ~0.06); ~7.5 MB/stream cache.
- ✅ **transcribe-rs 0.3.11 has NO GigaAM streaming** — CTC head, no encoder
  cache; GigaAM keeps chunk-boundary finalize (no regression).
- ✅ **SmoothedVad already in molvi** — 450 ms hangover is a competent
  trail-off detector; Silero/ParakeetEOU add deps without RU/multilingual
  benefit. No new VAD dep.
- ✅ **Talon's grammar approach is deterministic** — `.talon` files are
  string/list-match → action, NOT NLU/LLM. Confirms D25's regex/trie shape.
- ✅ **Win+H / Dragon / Talon / Wispr all auto-stop on silence** — the
  1200 ms default is the dictation industry norm.
- ✅ **Federated search reuses existing IPC** — `history_query` +
  `dictionary_list` already ship; no new commands needed for the search box.

**In-task verify (per AGENTS.md rule):**

- Task 1: ctx7/docs.rs the exact `Nemotron::from_shared` cache-construction
  signature + the `transcribe_chunk` `&[f32]` size contract.
- Task 5: ctx7/enigo docs for `Key::Other(VK)` chord building (matches the
  paste.rs VK_V pattern).
- Task 8: ctx7 `windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow`
  + `Win32::System::Threading::QueryFullProcessImageNameW` for the profile
  resolver (already used by `paste::capture_target`; widen to return exe).
