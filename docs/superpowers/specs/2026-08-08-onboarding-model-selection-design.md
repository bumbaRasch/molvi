# Onboarding model selection — design

**Date:** 2026-08-08
**Status:** Approved (brainstormed), pending spec review
**Backward compatibility:** Not required. Free to change cold-start logic, onboarding flow, settings write paths.

## Problem

Onboarding step 1 silently auto-downloads the default model (Nemotron, ~2.4 GB) at cold
start (`lib.rs` bg thread → `model_store::ensure_model(&settings.model)`), with no user
choice. A Russian-only user wastes ~2.2 GB downloading Nemotron when GigaAM (~214 MB,
Russian, punctuated, faster) would serve them better. The infrastructure to choose a
model already exists — but only in **Settings → Recognition**, shown *after* the 2.4 GB
already downloaded.

## Goal

Replace the silent auto-download with an explicit, informed model choice in onboarding
step 1. Only the chosen model downloads. A smart default (driven by the UI language the
user just picked) pre-selects the better option to minimize friction while preserving a
one-tap override.

## The two engines (data)

| Model | id | Languages | Size | Punctuation | Speed |
|---|---|---|---|---|---|
| GigaAM-v3 | `gigaam-v3-e2e-ctc` | Russian only | ~214 MB | periods + commas | fastest |
| Nemotron 3.5 | `nemotron-3.5-asr-streaming-0.6b` | 40 locales (multilingual) | ~2.4 GB | commas only | fast (streaming) |

Constraint: the engine loads **at startup only** (no hot-reload). Switching models later =
download + restart (existing Settings flow, unchanged).

## Smart default

Driven by the UI language the user selected in onboarding's language picker:

- `ui_lang == "ru"` → pre-select **GigaAM** (smaller, RU-optimized, punctuated, fastest —
  objectively better for Russian dictation).
- `ui_lang != "ru"` → pre-select **Nemotron** (multilingual — a non-RU user needs it).

The recommendation is a default, not a mandate: both cards are always visible; switching
is one tap. The card descriptions make the tradeoff explicit ("Russian only" vs "40
languages") so a bilingual RU user can pick Nemotron deliberately.

## Flow (step 1 redesign)

Current step 1: welcome + indeterminate "Downloading the speech model…" → auto-advance on
`engine-ready`.

New step 1, three phases:

```
Welcome (privacy anchor)
   │
   ▼
[Choice] two cards, pre-selected by ui_lang; user taps a card (can switch) → [Download {size}]
   │  on confirm: invoke onboarding_select_model(model_id, lang)  →  bg thread proceeds
   ▼
[Download] real progress bar (bytes / total / pct) + [Cancel]
   │  engine-ready  →  auto-advance
   ▼
Step 2 (hotkey + mic test) → Step 3 (first word, gates on engine-ready — as today)
```

Download blocks step 1 (chosen approach A): the wait is "earned" — the user chose this
model and saw its size upfront. RU default (GigaAM 214 MB) downloads fast; only multilingual
users who knowingly chose Nemotron wait for 2.4 GB. Progress lives in one focused place, no
interleaving with later steps.

## Architecture — deferred cold-start

**Current** (`lib.rs` bg thread `molvi-setup-bg`, ~L562–658): `block_on(ensure_model(settings.model))`
→ engine spawn → coordinator spawn → hotkey register → `tray::set_status` + `emit("engine-ready")`.

**New:** the bg thread **waits for a selection signal** before downloading; nothing downloads
silently at cold start.

- `AppState` gains `model_selection_tx: Mutex<Option<std::sync::mpsc::Sender<(String, String)>>>`
  (mirrors the existing `cmd_tx: Mutex<Option<Sender<Command>>>` — same proven pattern).
- In `run()`, create `(tx, rx) = mpsc::channel()`; `tx` → `AppState`; `rx` moved into the
  bg-thread spawn in `setup`.
- Bg thread: `let (model_id, _lang) = rx.recv()?;` then `block_on(ensure_model(&model_id))`
  → rest of the chain (engine → coordinator → hotkey → tray ready → engine-ready) unchanged.
- **Returning-user auto-proceed (critical):** in `setup`, after building `AppState` and
  reading `settings`, if `settings.onboarded` → immediately `tx.send((settings.model, settings.language))`
  so the bg thread proceeds without waiting for onboarding. First-run (`!onboarded`) does
  NOT send — onboarding sends after the user picks. This unifies the path: the bg thread
  always waits on the channel; returning users auto-feed it, first-run users feed it via
  the new IPC.

**Verified patterns (ctx7, v2.tauri.app):** `#[tauri::command]` + `tauri::State<T>` +
`Mutex<T>` managed state (`develop/state-management`, `develop/calling-rust`); `app.emit`
(`use tauri::Emitter`); frontend `listen`/`invoke` (`@tauri-apps/api/event`). All already
in use across molvi's 37 commands.

## New IPC

`onboarding_select_model(model_id: String, language: String) -> Result<()>` (in `ipc.rs`,
alongside the other onboarding commands):

1. Validate `model_id` ∈ {`gigaam-v3-e2e-ctc`, `nemotron-3.5-asr-streaming-0.6b`}.
2. `settings.model = model_id`; `settings.language = language`; persist (`settings::save`).
3. Signal the bg thread: take `model_selection_tx` from `AppState`, `send((model_id, language))`.
4. The bg thread receives → `ensure_model` → engine spawn → `emit("engine-ready")`.

Frontend resolves `language` from `ui_lang` via the existing `NEMOTRON_LANGS` map in
`recognition.ts` (best match, fallback `"auto"`). For GigaAM, language is inert (RU
hardcoded in `engine.rs`) — pass `"auto"`. This keeps the `NEMOTRON_LANGS` data in one
place (frontend), no Rust duplication.

Download progress reuses the existing events `model-download-progress` /
`model-download-complete` / `model-download-failed`, and cancel reuses `cancel_model_download`.

**Where the download lives (unified path, verified against source):** the bg thread
remains the single owner of download + engine spawn for BOTH first-run and returning-run.
Verified mechanism:
- `ensure_model(id, make_progress)` (model_store.rs:288) takes a `Fn(u64) -> Option<Progress>`;
  the cold-start currently passes `|_| None`. Passing a real closure that builds a
  `ModelProgressEmitter` (model_store.rs:115) wires `model-download-progress` — exactly as
  `ipc::download_model` (ipc.rs:591) already does.
- The bg thread runs the download as a `tauri::async_runtime::spawn` task (not `block_on`),
  stores its `JoinHandle` in the **existing** `AppState.model_download: Mutex<Option<JoinHandle>>`
  (lib.rs:175), and waits on a channel for the result. This makes the **existing**
  `cancel_model_download` (ipc.rs:613, `take().abort()`) work for the bg-thread download too —
  no new cancel plumbing.
- The bg thread **loops** on the choice channel: `rx.recv()` → download → on success proceed
  to engine spawn; on cancel (task aborted → result-channel `recv()` Err) or error
  (`model-download-failed`) → loop back to `rx.recv()` (wait for the next choice = retry or a
  different model). mpsc buffers, so no lost-send race between cancel and the next pick.

Returning-user auto-proceed: `setup` sends `(settings.model, settings.language)` once on the
choice channel when `onboarded`; the bg thread's first `recv()` gets it (mpsc buffers, so
order-independent). For a cached returning-user model, `ensure_model`'s byte-exact fast path
(model_store.rs:301) is a no-op → engine spawns immediately, as today.

## Step-1 UI

**Choice phase:**
```
Choose your speech model
You can change this later in Settings.

┌────────────────────────────────────┐
│ ✓ Recommended                       │   ← pre-selected (smart default)
│   GigaAM                            │
│   Russian — fast, with punctuation. │
│   214 MB                            │   ← from model_store::ModelStatus.size_bytes (fmtBytes)
└────────────────────────────────────┘
┌────────────────────────────────────┐
│   Nemotron                          │
│   40 languages (multilingual).      │
│   2.4 GB · commas, no periods       │
└────────────────────────────────────┘

              [ Download 214 MB ]         ← label shows selected card's size
```
Tapping a card switches selection + updates the button label. The "Recommended" badge
sits on whichever card the smart default pre-selected.

**Download phase:**
```
Downloading GigaAM…
██████████░░░░░░  58%  · 124 / 214 MB
                [ Cancel ]
```
Real progress (≤4 Hz via `ModelProgressEmitter`). Cancel → `cancel_model_download`.

**Error phase:**
```
⚠ Download failed. Check your connection.
        [ Retry ]  [ Choose another ]
```
Retry → re-invoke `onboarding_select_model`. "Choose another" → back to the choice cards.
A partial/cancelled file is rejected by `model_store`'s byte-exact cache check on retry
(no HTTP Range resume — known limitation, restarts that file).

## Skip

Skip = proceed with the **pre-selected (recommended) model**: the frontend invokes
`onboarding_select_model` with the smart-default model before calling `complete_onboarding`.
Semantically, skipping = "accept the recommendation." The recommended model downloads in
the background; the app works. No dead engine-less state. (The pre-selected card *is* the
skip behavior — explicit and consistent.)

## Recognition language

- **Nemotron:** `settings.language` defaults from `ui_lang` via `NEMOTRON_LANGS` (frontend
  resolves, fallback `"auto"`). Explicit locale selection remains in Settings → Recognition.
  Rationale: Nemotron streaming auto-detection is unreliable (lang-tag tokens are zero,
  detection falls back to `settings.language`), so a concrete locale from the UI language
  is more reliable than blind `"auto"`.
- **GigaAM:** language inert (`engine.rs` hardcodes `TranscribeOptions { language: Some("ru") }`).

## i18n

Reuse existing keys: `models.gigaam_desc`, `models.download`, `models.downloading`,
`models.cancel`, `models.download_failed` (already translated ×36).

New keys (~6) added to all 36 locales (`src/i18n/locales/*.ts`), `en` canonical:

| Key | en value |
|---|---|
| `onboarding.model_choose_title` | `Choose your speech model` |
| `onboarding.model_choose_subtitle` | `You can change this later in Settings.` |
| `onboarding.model_recommended` | `Recommended` |
| `onboarding.model_nemotron_desc` | `40 languages (multilingual). Commas, no periods.` |
| `onboarding.model_retry` | `Retry` |
| `onboarding.model_choose_another` | `Choose another model` |

(GigaAM description reuses `models.gigaam_desc`. Model **names** "GigaAM" / "Nemotron" are
proper nouns, untranslated. **Sizes** are computed via `fmtBytes`, not localized strings.)

## Files affected

- `src-tauri/src/lib.rs` — bg-thread deferral (remove cold-start `ensure_model`, add
  channel wait); returning-user auto-send in `setup`; register the new command; add
  `model_selection_tx` to `AppState`.
- `src-tauri/src/ipc.rs` — new `onboarding_select_model` command.
- `src/onboarding.ts` — step-1 redesign (choice cards + download/error phases; resolve
  `ui_lang` → Nemotron locale; invoke the new command; listen to download events).
- `src/onboarding.css` — card / selected / badge / progress styles (logical CSS properties
  for RTL, per project convention).
- `src/i18n/locales/*.ts` ×36 — new keys (en canonical, set-equal across locales).
- No schema change (`Settings.model` / `Settings.language` already exist).
- No change to `src/settings/types.ts` (ModelStatus already mirrors Rust).

## Out of scope

- Changing the engine-no-hot-reload constraint (engine + language apply at startup only).
- Nemotron punctuation (settled — streaming-only; do not re-litigate per AGENTS.md).
- HTTP Range resume for the 2.4 GB download (known `model_store` limitation).
- The Settings → Recognition model picker (unchanged; still handles post-onboarding switches
  with download + restart).
