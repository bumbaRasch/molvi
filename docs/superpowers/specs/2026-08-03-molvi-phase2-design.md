# molvi — Phase-2 Design Spec

- **Date:** 2026-08-03
- **Status:** Draft, pending user review
- **Extends:** [`2026-08-02-molvi-push-to-talk-design.md`](./2026-08-02-molvi-push-to-talk-design.md) (Phase-1, shipped @ `967a80f`)
- **Target platform:** Windows 11 x64 (unchanged)
- **Surface:** Tauri 2 desktop application (web UI, native Rust core) — unchanged
- **Posture:** Still 100 % local / offline / private. **No cloud providers.** The only new
  network activity is (a) the optional signed auto-updater check and (b) the optional
  user-configured AI-post-processing endpoint (which may itself be a local server).

---

## 1. Vision (Phase-2)

Phase-1 shipped a fast, privacy-first **Russian** push-to-talk MVP: GigaAM-v3 `e2e_ctc`
on-device (RTF 0.029 ≈ 34× realtime), 4-thread arch, non-focusable overlay, clipboard-paste
with focus guard. Phase-2 turns molvi into the **best local dictation app** without
surrendering the moat:

- **Daily-driver ergonomics** — a real settings UI, toggle mode, autostart, signed
  auto-updater, audio feedback, a proper tray menu.
- **Memory** — transcript history (opt-in, local, erasable) + a custom dictionary for
  recurring names/terms/EN-in-RU. The highest daily-value gap vs Handy.
- **Better text** — deterministic *Smart* post-processing + optional *Polished* mode via a
  user-supplied OpenAI-compatible endpoint (cloud or a local Ollama/llama.cpp server).
- **Multilingual (gated)** — a measurement-driven spike decides whether NVIDIA Nemotron
  3.5 ASR Streaming runs real-time on the target CPU. If it does, multilingual recognition
  is wired (non-streaming, manual engine picker). If it does not, multilingual defers to
  Phase-3 and molvi stays RU-only.
- **Blaze preserved** — no regression to RTF 0.029 / cold-start 1251 ms / RSS 292 MB /
  NSIS 9.43 MB. The ort thread-count + P-core-affinity levers are applied for free (no
  fork); sequential-mode + spin only if measurement justifies a tiny upstream PR.

Everything stays on-device. The privacy promise (§14) is strengthened, not weakened: the
default stores **nothing** after paste; history is opt-in and erasable; no transcript text
is ever logged.

---

## 2. Goals & Non-Goals

### Phase-2 goals (scope cut **B**)

1. **Transcript history** — opt-in SQLite, plaintext, local, tight default retention
   (100 entries / 7 days), per-entry delete, Clear All, Disable & Erase.
2. **Custom dictionary** — separate `dictionary.db`, CRUD + Import/Export (CSV/JSON),
   applied as a deterministic *Smart* transform.
3. **Settings UI** — full sidebar app: Recognition → Microphone → Text → Dictionary →
   History → Hotkey → Overlay → Updates → About (pipeline first, system second).
4. **Toggle mode** — explicit PTT | Toggle setting (PTT default).
5. **Autostart** — `tauri-plugin-autostart`.
6. **Signed auto-updater** — `tauri-plugin-updater` + GitHub Releases + signing keypair.
7. **Audio feedback** — opt-in start/stop tones (bundled wavs), grouped with Overlay.
8. **Tray menu** — Status / Toggle / Settings / History / Quit.
9. **AI post-processing** — Raw / Smart (deterministic) / Polished (OpenAI-compatible
   endpoint). **No bundled LLM.**
10. **ort threading win** — thread-count + P-core affinity (no fork); measure-first on the
    sequential/spin lever.
11. **Nemotron viability spike** — narrow measurement task; conditional multilingual wiring.
12. **Deferred Phase-1 polish** — async model download + progress, overlay bottom-center
    positioning, the cancel-paste generation-guard edge case.

### Non-Goals (Phase-2)

- **Streaming / EOU / true sub-chunk partials** (Phase-1 item 4) — deferred to Phase-3.
  Requires a cache-aware streaming engine; only worth it once Nemotron streaming is proven.
- **Full Nemotron cache-aware streaming** — even on a GO spike, Phase-2 wires **non-streaming**
  only (parakeet-rs offline `transcribe_audio` over the whole captured buffer).
- **Cloud transcription providers** — none. molvi stays on-device. The post-processing
  endpoint is the user's choice (cloud *or* local server); transcription itself never leaves
  the machine.
- **A bundled local LLM** — kills blaze (GBs + seconds of CPU latency). "Local" polish is
  satisfied by pointing molvi at the user's own on-device OpenAI-compatible server.
- **SQLCipher / encryption-at-rest** — deferred (Phase-4/Enterprise if ever). The privacy
  guarantee is *nothing stored by default*, not encrypted storage.
- **Automatic language routing** — even at Nemotron GO, the engine is a manual user pick.
  Auto-routing (prompt_index=101) is deferred until its real-world quality is understood.
- **i18n / RTL UI, macOS / Linux ports, DirectML EP, Silero VAD, CLI flags** — Phase-3+.

### Cross-cutting Quality Bar

These govern **every** Phase-2 decision and task; they override convenience.

**Blaze / maximum performance.** molvi's differentiator is speed. The Phase-1 NFRs
(RTF 0.029, cold-start 1251 ms, RSS 292 MB, NSIS ≤ ~11 MB) are a **one-way ratchet** — no
regression for the default RU/PTT/Smart user. Choices optimize footprint + latency: the
inference path (audio → worker → emit) stays hot-path-clean (no allocations/locks); post-
processing, history I/O, updater checks, and tone playback run off the hot path (finalize
side-thread / async / lazy). Wins are **measurement-driven** — benchmark before claiming
(the ort §11 sweep, the Smart-pipeline µs budget). Prefer stdlib, native platform features,
and already-installed deps over new code or new crates.

**No backward compatibility.** Pre-1.0; clean breaks only. No migration paths, no shims, no
deprecation. `settings.json` regenerates from `#[serde(default)]` on any structural change;
storage schemas (`history`, `dictionary`) may be dropped/recreated — no on-disk data
guarantees until 1.0.

**Minimal dependencies — only what earns its place.** YAGNI at the dep level: each new crate
must do work the stdlib / an existing dep cannot, and must be the lean option in its class.
Concretely: `rusqlite` `bundled` (one static SQLite, no system dep), `ureq` over `reqwest`
(sync + tiny, runs on the existing finalize side-thread), `cpal` reused for output tones (no
second audio dep), inline SVGs + a ~40-line signal store (no UI framework, no icon font). No
duplicate-functionality crates; no "in case we need it" deps.

**All latest, docs-verified.** Every crate/npm package targets the **latest stable as of
2026-08**, pinned in the plan with exact versions. API signatures are **never trusted to
memory** — verified via ctx7 (`npx ctx7@latest library` / `docs`) + docs.rs + crates.io at
plan-writing time (AGENTS.md rule). The current verified set lives in §4; the per-dep
verify-gates in §18.

**Clean code (ponytail).** Smallest working diff wins. No speculative abstractions (no trait
with one impl, no factory for one product, no config for a value that never changes).
Calibration knobs that survive real-hardware variance stay (resampling, VAD thresholds,
chunk sizes, retention); everything else is a constant. Deliberate shortcuts carry a
`ponytail:` comment naming the ceiling and upgrade path. Every non-trivial module leaves one
runnable self-check. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
-- -D warnings` and `cargo fmt` are clean at every commit. Comments explain *why*, never
*what*.

**Privacy is non-negotiable (§14 / Phase-1 §10.1).** Blaze/lean never overrides privacy:
never simplify away input validation at trust boundaries, error handling that prevents data
loss, or the logging-discipline guard.

---

## 3. Key Decisions Log

Carries forward D1–D12 from Phase-1. Phase-2 load-bearing decisions:

| # | Decision | Chosen | Rejected alternative | Rationale |
|---|---|---|---|---|
| D13 | Phase-2 scope | **Cut B** — items 1–3 + polish + ort win (5) + Nemotron spike; defer streaming (4) + full streaming-Nemotron to Phase-3 | A (app-layer only); C (all 6 items) | Captures the cheap blaze win (item 5 needs ~no fork), resolves the Nemotron GO/NO-GO with data not guesses, ships the high-value app-layer, defers the genuinely expensive streaming/EOU which only pays off once a streaming engine is proven. |
| D14 | AI post-proc | **Raw / Smart (deterministic) / Polished (OpenAI-compat endpoint)**; no bundled LLM | Bundle a local LLM; endpoint-only | Smart is ms-fast, fully predictable, zero-config (the deterministic list in §8). Polished reuses one HTTP path for cloud *or* local Ollama/llama.cpp — "local" without bundle bloat. A bundled LLM contradicts blaze (GBs + latency). |
| D15 | History privacy posture | **Opt-in, plaintext SQLite, 100/7d default, per-entry delete + Clear All + Disable & Erase** | SQLCipher encryption; in-memory only | §10.1 is a HARD RULE: the guarantee is *nothing stored without consent*, not encrypted storage. Tight default retention + panic button beat "encrypted but hoarded". Honest copy beats security-theater copy. |
| D16 | Dictionary storage | **Separate `dictionary.db`, independent lifecycle** | Same DB/table as history | "Clear all history" must never touch the user's dictionary. Conceptually distinct (user-authored vs captured speech). |
| D17 | Nemotron integration | **Measurement spike first (parakeet-rs loader); GO RTF<0.5 / Conditional 0.5–1.0 (off+warning) / NO-GO ≥1.0; at GO wire non-streaming + manual picker; no auto-routing; no streaming in Phase-2** | Build Nemotron in transcribe-rs now; auto-route RU→GigaAM | Spike answers only "can it run on target HW?". parakeet-rs already matches Nemotron's format (verified docs.rs: supports multilingual + prompt_index + cache streaming + auto-detect) — least own code, no upstream PR before viability proven. transcribe-rs migration only if (GO pass ∧ permanent ∧ Phase-3 streaming work). |
| D18 | ort threading | **Thread-count via existing `create_session_with_threads` + P-core affinity via Win32 `SetThreadAffinityMask`; no fork. Sequential+spin via tiny upstream PR ONLY if measured gain** | Fork transcribe-rs upfront | `src/onnx/session.rs::build_session` already wires `with_intra_threads`/`with_parallel_execution`/`with_intra_op_spinning`; `create_session_with_threads` is public. Affinity is a property of *molvi's* worker thread, not the library. Current RTF 0.029 is already excellent — measure before believing the 0.029→0.022 claim. |
| D19 | Toggle mode | **Explicit setting PTT (default) \| Toggle; coordinator branches on `settings.push_to_talk`** | Hybrid tap/hold on one binding | Cleanest mental model, no timing discriminator latency, no edge cases. The field already exists in `Settings`. |
| D20 | Updater | **`tauri-plugin-updater` + GitHub Releases hosting + Tauri signing keypair (pubkey in `tauri.conf.json`, privkey in CI secrets)** | Self-hosted manifest; defer | Standard Tauri path; free hosting; offline-capable (check is the only network call, optional). |
| D21 | Visual system | **Fluent-native discipline + Swiss-Modernist restraint; system-ui; teal `#0E7C86` accent (CTAs/selection/focus only); semantic colors status-only; Windows lighter-on-hover + stroke-focus; 8px grid; vanilla TS + ~10 tiny components + inline SVG icons** | A brand web-font; a CSS framework; Preact | Native Win11 feel + zero font download (blaze). Restrained accent + semantic discipline = calm + accessible (WCAG AAA target). vanilla TS suffices (Handy uses React, but molvi's UI is plain forms — Preact only if a genuinely hard reactive problem appears; none visible). |

---

## 4. Technology Stack (Phase-2 additions)

All versions latest stable as of 2026-08; **every new API/dep is ctx7/docs.rs-verified at
plan-writing time** (AGENTS.md rule: never trust memory). Pin exact versions in the plan.

### Rust (crates.io) — new

| Layer | Crate | Version (verified 2026-08-03 via subagent — ctx7/docs.rs/crates.io) | Notes |
|---|---|---|---|
| History + dictionary | `rusqlite` | **0.40.1** | `features = ["bundled"]` — compiles the SQLite amalgamation via `cc` on MSVC (no system SQLite/DLL). `Connection` is `Send`+`!Sync` → share via `Arc<Mutex<Connection>>`. No MSRV issue on 1.97.1. Two DB files (`molvi.db`, `dictionary.db`). |
| Autostart | `tauri-plugin-autostart` | **2.5.1** | `init(MacosLauncher::LaunchAgent, Some(["--autostarted"]))`; runtime toggle via `ManagerExt::autolaunch()` → `enable/disable/is_enabled` (trait is `autolaunch`, **not** `autostart`). Windows: writes `HKCU\…\Run`. |
| Signed updater | `tauri-plugin-updater` | **2.10.1** | `cargo tauri signer generate`; pubkey **inline** at `plugins.updater.pubkey` (NOT a path); build envs `TAURI_SIGNING_PRIVATE_KEY`(+`_PASSWORD`); `bundle.createUpdaterArtifacts: true`; `latest.json` per-platform `{version,notes,pub_date,platforms.<target>.{signature,url}}`; Rust `UpdaterExt::updater()?.check().await?` → `Option<Update>` → `download_and_install().await` + `app.restart()`. NSIS confirmed. |
| Relaunch after update | `tauri-plugin-process` | latest 2.x | `app.restart()` lives here (not core tauri). |
| File dialogs (dict import/export) | `tauri-plugin-dialog` | **2.7.2** | `DialogExt::dialog().file() → FileDialogBuilder`; `.blocking_pick_file()` / `.blocking_save_file()` (**off main thread**); built on `rfd`. |
| Post-processing HTTP | `ureq` | **3.3.0** | Chosen over `reqwest` 0.13.4: pure-Rust, **no tokio**, ~25 deps vs ~60, `#![forbid(unsafe)]`. `features = ["json"]` (rustls default, gzip default). One `Agent` built via `config_builder().timeout_global(Some(...))`; distinct `Error::StatusCode`/`Timeout`/`ConnectionFailed`. Runs on the existing finalize side-thread. |
| Nemotron spike (experimental) | `parakeet-rs` | **0.3.7** | **Spike binary only — NOT a `molvi` dep until GO.** `Nemotron::from_pretrained(dir, None)`; multilingual auto-detected from the encoder's `prompt_index` input; **`set_target_lang("ru-RU"|"en-US"|…|"auto")`** (`"auto"` = prompt_index 101); **offline `transcribe_audio(&[f32]) -> String`** for Phase-2 (streaming `transcribe_chunk` is Phase-3). ⚠ pins `ort 2.0.0-rc.13` directly (transcribe-rs pulls rc.12) — fine standalone; flag if ever unified. |
| P-core process affinity | `windows` (existing) | **0.62.2** (already latest — no bump) | **Add features `Win32_System_Threading` + `Win32_System_SystemInformation`**: `SetProcessAffinityMask(GetCurrentProcess(), mask)` + `GetLogicalProcessorInformationEx` (enumerate P-cores via `EfficiencyClass == 0`). Mechanism = **process** affinity (worker-thread affinity doesn't reach ort's own intra-op pool). Verify exact feature flags at the task. |
| Audio feedback | `cpal` (already a dep) | 0.18.1 | cpal output stream for start/stop tones (bundled wavs). No new audio dep. |

### Existing Rust deps — widened in place

- `tauri` 2.11.5 → add **menu** (`tauri::menu::{Menu, MenuItem, CheckMenuItem, PredefinedMenuItem, on_menu_event}`) for the tray menu + settings window menu.
- `transcribe-rs` 0.3.11 — unchanged for Phase-2 base (GigaAM). At Nemotron GO, `parakeet-rs` is used alongside it behind a local adapter (§10), **not** a transcribe-rs change.
- `settings.rs` — widened with new fields (§6.1).

### Frontend (npm) — unchanged stack, new code

- Vite 8 + TypeScript 7, **vanilla TS, no framework**. A tiny signal-store (~40 lines) +
  ~10 small DOM components (§9.3). No Preact unless a future reactive problem demands it.
- Icons: ~14 inline Lucide-style SVGs (no CDN — CSP `default-src 'self'`, no emoji).
- One optional web-font (Atkinson Hyperlegible woff2, ~30 KB) for the overlay caption **only
  if** contrast-over-backgrounds testing shows the system font insufficient.

---

## 5. Architecture (Phase-2 deltas)

The Phase-1 4-thread architecture (Tauri main / coordinator / cpal audio / inference worker)
is **unchanged** — it proved out. Phase-2 adds narrow, non-hot-path concerns:

```
                 ┌─────────────────────────────────────────────────────────────┐
                 │                        TAURI MAIN THREAD                     │
                 │   webview · tray+menu · IPC commands · events · UPDATER      │
                 └──────▲───────────────────────────────▲─────────────▲────────┘
                        │ stream-text / mic-level / phase │ final text  │ DB / post-proc
                        │                                 │             │ (called from the
                 ┌──────┴─────────────────┐    ┌──────────┴───┐    ┌────┴──────────────┐
                 │  INFERENCE WORKER       │    │  FINALIZE     │    │  STORE             │
                 │  (ort Session + model)  │    │  SIDE-THREAD  │    │  rusqlite:          │
                 │  + P-CORE AFFINITY      │    │  post-proc →  │    │  molvi.db (history)│
                 │  (SetThreadAffinityMask)│    │  paste →      │    │  dictionary.db     │
                 └──────▲─────────────────┘    │  history insert│    └────────────────────┘
                        │ 16 kHz mono f32 (SPSC) │ (if enabled)   │
                 ┌──────┴─────────────────┐    └────────────────┘
                 │  cpal AUDIO (in + out)  │
                 │  in: capture → ring     │
                 │  out: start/stop tones  │
                 └────────────────────────┘
```

**Deltas from Phase-1:**

1. **P-core affinity** applied at worker spawn (`SetThreadAffinityMask`) — no behavioral
   change, only steady-state RTF.
2. **Finalize side-thread** (already exists for paste) now also runs post-processing
   (Smart sync / Polished HTTP) and, if history is enabled, inserts + prunes.
3. **Store** — two `rusqlite::Connection`s, opened lazily and held in Tauri managed state:
   - `dictionary.db` — touched only by the IPC/settings thread (CRUD from the Dictionary UI);
     no cross-thread access → bare `Connection`, no `Mutex`.
   - `molvi.db` (history) — **written** by the finalize side-thread **and read** by the IPC
     thread (History viewer). `rusqlite::Connection` is `Send` but not `Sync`, and SQLite
     serializes writers anyway, so history lives behind one `Arc<Mutex<Connection>>` shared
     by both threads. Contention is negligible (a finalize write is rare and brief; viewer
     reads are on-demand). `history` is opened only when the user opts in.
4. **Tray + menu** — a real `Menu` replaces the click-only handler.
5. **Updater** — plugin check on startup (gated by a setting) + manual "Check now".
6. **cpal output** — a short-lived output stream plays a tone wav on session start/stop.

---

## 6. Components (Rust modules — Phase-2)

Phase-1 layout is preserved; new files are additive.

```
src-tauri/src/
  main.rs            — unchanged (1-line shim)
  lib.rs             — widen: menu, updater plugin, new IPC commands, tone output hook
  paths.rs           — add history_db_path(), dictionary_db_path()
  errors.rs          — widen MolviError (Db, Dictionary, PostProc, Updater, Spike)
  log.rs             — unchanged (privacy §10.1 unchanged)
  settings.rs        — WIDEN (§6.1): new fields, still #[serde(default)], no migration
  audio.rs           — add output stream for start/stop tones (folded here, no tones.rs)
  engine.rs          — GigaAM unchanged; thread-count + affinity knobs at spawn
  ort_opts.rs        — NEW (small): thread-count + affinity helpers (§11)
  coordinator.rs     — branch on settings.recognition_mode for Toggle vs PTT
  pipeline.rs        — widen finalize side-thread: post-proc → paste → history insert
  paste.rs           — unchanged (+ generation-guard edge fix from deferred polish)
  overlay.rs         — widen: bottom-center positioning, tone trigger hooks
  hotkey.rs          — widen: AltGr Ctrl+Alt mirror registration (config checkbox)
  history.rs         — NEW: molvi.db schema, insert, prune, query, clear, disable&erase
  dictionary.rs      — NEW: dictionary.db schema, CRUD, import/export, apply-as-transform
  postproc.rs        — NEW: Smart (deterministic pipeline) + Polished (HTTP endpoint)
  tray.rs            — NEW: real Menu (extracted from lib.rs setup)
  updater.rs         — NEW: wraps tauri-plugin-updater (check + apply + error surfacing)
  db_migrate.rs      — NOT created (no backward compat; #[serde(default)] handles settings)

molvi-nemotron-spike/ — NEW reference binary (mirrors molvi-task0); produced by the spike task
```

### 6.1 `settings.rs` — widened schema (no version field, no migration)

Phase-1's `#[serde(default)]` posture is preserved: any missing key at any depth defaults,
**no schema version, no migration path** (backward compat is explicitly not a goal — a clean
break is fine; settings.json simply regenerates with defaults on structural change).

New fields (additive to Phase-1):

```jsonc
{
  // ... Phase-1 fields unchanged ...
  // ... Phase-1 fields unchanged (incl. "model"); at Nemotron GO, "model" also accepts
  // "nemotron-multilingual-..." (manual engine pick via the Recognition dropdown) ...
  "recognition_mode": "push_to_talk",  // "push_to_talk" | "toggle"  (clean-break rename of Phase-1 push_to_talk: bool)
  "post_processing": {
    "mode": "smart",                   // "raw" | "smart" | "polished"
    "endpoint": null,                  // OpenAI-compat base URL (polished)
    "api_key": null,                   // (polished)
    "model": null,                     // e.g. "gpt-4o-mini" or local model id (polished)
    "prompt": null,                    // optional custom system prompt (polished)
    "smart": {                         // Smart-mode sub-toggles (all on by default)
      "apply_dictionary": true,
      "fix_case": true,
      "normalize_whitespace": true,
      "cleanup_repeated_marks": true,
      "merge_chunks": true,
      "remove_duplicate_words": true,
      "normalize_numbers_dates": true,
      "remove_fillers": false,         // off by default (aggressive)
      "inter_chunk_punctuation": true
    }
  },
  "history": {
    "enabled": false,                  // §10.1: OFF by default
    "max_entries": 100,
    "max_age_days": 7
  },
  "overlay": {
    // Phase-1 fields +
    "position": "bottom_center",       // "bottom_center" (was "bottom"; Phase-1 polish)
    "sounds": {                        // moved here from a conceptual "audio" group
      "enabled": false,                // opt-in (discretion)
      "start": "start.wav",            // bundled
      "stop": "stop.wav"
    }
  },
  "hotkey": {
    // existing "hotkey" string +
    "altgr_mirror": false              // register Ctrl+Alt+` mirror for RU/EU layouts
  },
  "autostart": false,
  "updater": { "check_on_startup": true, "channel": "stable" }
}
```

The Phase-1 `push_to_talk: bool` is retained as `recognition_mode` (an enum string) for a
clean break; the coordinator reads whichever field is present.

### 6.2 `history.rs` + `dictionary.rs` — data model

**`molvi.db` — `history` table (created lazily, only when the user enables history):**

```sql
CREATE TABLE IF NOT EXISTS history (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at  INTEGER NOT NULL,           -- unix ms
  text        TEXT    NOT NULL,           -- the final (post-processed, pasted) transcript
  lang        TEXT,                       -- engine language at capture time
  engine      TEXT,                       -- "gigaam" | "nemotron"
  post_mode   TEXT                        -- "raw" | "smart" | "polished"
);
CREATE INDEX idx_history_created ON history(created_at DESC);
```

- **Insert** happens on the finalize side-thread, **after** a successful paste, only when
  `history.enabled` is true. Inserting before paste would retain text the user never received.
- **Prune** runs after each insert: delete rows beyond `max_entries` (by id) and older than
  `max_age_days`.
- **Query** (for the UI viewer): paginated `SELECT ... ORDER BY created_at DESC`, with a
  case-insensitive `LIKE` search term. Only `id`, `created_at`, `text` cross IPC to the UI.
- **Delete one:** `DELETE WHERE id = ?`.
- **Clear All:** `DELETE FROM history` (leaves the table; dictionary untouched).
- **Disable & Erase:** `DROP TABLE history` + set `history.enabled=false` + optionally delete
  `molvi.db`. One action.

**`dictionary.db` — `dictionary` table (created on first access):**

```sql
CREATE TABLE IF NOT EXISTS dictionary (
  entry       TEXT PRIMARY KEY,          -- the token/phrase to match (case-insensitive)
  replacement TEXT NOT NULL,             -- the text to substitute
  created_at  INTEGER NOT NULL
);
```

- CRUD via IPC from the Dictionary UI.
- **Import/Export:** CSV (`entry,replacement`) and JSON (`[{entry,replacement}, ...]`) via
  file dialogs (`tauri-plugin-dialog`, already available in the Tauri 2 ecosystem — verify
  feature at plan time).
- **Apply-as-transform:** used by `postproc::smart` (§8) — whole-token, case-insensitive
  match, supports multi-word phrase entries, replacement preserves surrounding spacing/
  punctuation. **No fuzzy/phonetic match** (deterministic only — the user's "fully
  predictable" requirement).

---

## 7. Settings UI — Information Architecture + Visual System

### 7.1 Information Architecture (locked)

Sidebar (left rail ~200px, label + inline-SVG glyph) + scrollable content pane (max ~640px):

**Pipeline group — "how molvi turns voice into text":**

1. **Recognition** — Engine (`GigaAM (Russian)` / `Nemotron Multilingual` — the latter only if
   GO; Conditional-GO shows the slower-warning badge), Language, collapsed **Advanced**
   (VAD: min/max chunk, padding, energy threshold).
2. **Microphone** — Input device dropdown + **Refresh devices** button (USB hot-plug) + a
   live level meter (the existing `mic_level` atomic, read on a timer while this pane is open).
3. **Text** — Paste mode (clipboard / type), Post-processing radio (**Raw / Smart / AI
   Rewrite**). **AI Rewrite** conditionally reveals Endpoint / API key / Model / Prompt.
   **Smart** conditionally reveals the sub-toggles (§6.1). Trailing-space append (carry-over).
4. **Dictionary** — CRUD list (entry → replacement), add/edit/delete, **Import / Export**
   (CSV/JSON).
5. **History** — **consent-first layout**: the opt-in checkbox + privacy promise copy at the
   top, *then* retention (entries / days), *then* search + list + per-entry re-paste/delete,
   *then* **Clear all** + **Disable & erase** at the bottom.

**System group — "how molvi works as an app":**

6. **Hotkey** — binding capture, mode (PTT / Toggle), **AltGr Ctrl+Alt mirror** checkbox.
7. **Overlay** — enabled, position (bottom-center), waveform, timer, **Sounds** (audio
   feedback toggle + start/stop sound choice — grouped here because all four are
   feedback/UX indicators, matching Handy's `AudioFeedback`+`SoundPicker` grouping).
8. **Updates** — current version, Check now, channel.
9. **About** — version, licenses, links, and the **Privacy Promise** as its own item (not
   buried in license text).

### 7.2 Visual design system (locked)

Grounded in **Fluent 2** (native Win11 color/interaction discipline) + **Swiss Modernism**
(grid/restraint/single-accent/WCAG-AAA from ui-ux-pro-max's style DB) + **Handy** (proven
same-stack peer: sidebar + grouped settings + `ui/` kit + post-proc API shape) + molvi's
calm/privacy ethos. (ui-ux-pro-max's `--design-system` misrouted to a newsletter/landing
pattern — discarded; its targeted typography/color/a11y queries were used.)

**Color tokens (CSS custom properties on `:root`):**

```css
:root {
  /* Neutrals (Fluent: lighter = primary surface) */
  --bg: #FFFFFF;          /* cards, primary surfaces */
  --canvas: #F4F5F7;      /* app background (recedes) */
  --border: #E5E7EB;
  --text: #1A1A1A;
  --muted: #6B7280;

  /* Brand accent — CTAs, selected state, focus ring ONLY (Fluent: never large surfaces) */
  --accent: #0E7C86;      /* calm teal — distinctive, matches serene/privacy ethos */
  --accent-hover: #0a6670;
  --on-accent: #FFFFFF;

  /* Semantic — STATUS ONLY, never decoration (Fluent rule) */
  --recording: #DC2626;   /* live indicator */
  --success: #16A34A;
  --warning: #D97706;
  --destructive: #DC2626; /* Clear all, Disable & erase */

  /* Radius + motion */
  --radius-control: 6px;
  --radius-card: 8px;
  --ease: cubic-bezier(0.2, 0, 0, 1);
}
```

- **Interaction (Windows rule):** hover = *lighter* surface; selected = brand fill;
  **focus = 2px brand stroke on the container**, never just a color change.
- **Typography:** `font-family: system-ui, "Segoe UI", "Segoe UI Variable", sans-serif;`
  everywhere in app chrome (zero font download = blaze; native Win11; Cyrillic-complete).
  Base 16px, line-height 1.5. The **overlay caption** uses system-ui + a scrim/text-shadow
  for contrast over arbitrary backgrounds; Atkinson Hyperlegible woff2 is bundled **only if**
  contrast testing over real backgrounds shows gaps.
- **Grid:** 8px base unit (Swiss). Content max-width ~640px for readability. Sidebar ~200px.
- **Motion:** 150–300ms, state-only (sidebar selection, panel fade, recording pulse, toast,
  progress bar). `prefers-reduced-motion` honored. No decorative animation.
- **Accessibility:** `<label for>` on every input (no placeholder-only), helper text, 8px+
  gaps between targets, text contrast ≥ 4.5:1, focus rings visible, every mutating action
  (dict add/edit/delete, save, clear, disable&erase, check-updates) shows explicit
  success/error feedback — no silent clicks.

### 7.3 Component kit (vanilla TS, no framework)

A small set of DOM helpers, each a thin function returning an element + a setter, driven by
one tiny signal-store (~40 lines: `subscribe`/`get`/`set`). Mirrors Handy's `ui/` kit but lean:

`Toggle`, `Select`, `TextInput`, `SecretInput` (for API key — `type=password`),
`Slider`, `SettingsGroup` (card wrapper), `SettingRow` (label + control + help text),
`Button`, `Alert`, `ProgressBar`, `Dialog`, plus `Sidebar` nav and a `Tooltip`.

**No Preact.** Preact appears only when a genuinely complex reactive problem surfaces that
this store can't solve elegantly — none is visible in Phase-2 (it's forms + a searchable list).

---

## 8. Post-processing Pipeline (`postproc.rs`)

Runs on the **finalize side-thread** (Phase-1 already has it), between `transcribe.finish()`
and `paste`. The chosen `post_processing.mode` selects the path.

### 8.1 Raw
No transform. The model output (GigaAM already emits punctuation + caps) is pasted as-is.
Fastest, most predictable, zero surprise — the opt-out for users who want exact model output.

### 8.2 Smart — deterministic pipeline (global default)

The user's exact, closed list — **deterministic transforms only, ms-fast, fully predictable.
No rephrasing, no style, no "make it nicer" — that is the LLM's job (Polished).**

Pipeline order (each step is a pure fn `&str -> String`; order matters, tuned, each unit-tested):

1. **merge_chunks** — join the session's finalized chunks with correct spacing (Phase-1 already
   space-joins; Smart also cleans the inter-chunk boundary).
2. **inter_chunk_punctuation** — heuristic: insert sentence-final punctuation at true pauses
   (long VAD gaps) between chunks; strip intra-chunk over-punctuation from short chunks
   (the GigaAM cross-chunk risk, §11 of Phase-1 spec, mitigated here).
3. **cleanup_repeated_marks** — collapse `...`/`??`/`!!`/`---` and stray repeated punctuation.
4. **remove_duplicate_words** — drop accidental immediate double words (`the the`).
5. **apply_dictionary** — whole-token, case-insensitive match; multi-word phrase entries;
   replacement preserves surrounding spacing/punctuation. Reads `dictionary.db` (cached for
   the session). Deterministic, no fuzzy match.
6. **fix_case** — sentence-start capitalization; fix ALL-CAPS bursts; preserve proper nouns
   already capitalized.
7. **remove_fillers** *(opt, off by default)* — strip configured filler words
   (`ээ`, `мм`, `ну`, `типа`, …; user-extensible list). Aggressive, hence opt-in.
8. **normalize_numbers_dates** — digit/date normalization *as far as possible without guessing*
   (e.g. "двадцать третье мая" → "23 мая"; never invents ambiguous forms).
9. **normalize_whitespace** — collapse runs, trim, single space after punctuation.

Each step is independently toggleable (§6.1 `post_processing.smart.*`). The pipeline is
pure-function and unit-tested per step (§15).

### 8.3 Polished — OpenAI-compatible endpoint

Sends the transcript to a user-configured OpenAI-compatible `/chat/completions` endpoint.
**One HTTP path serves both cases:**

- **Cloud** — the user enters their provider URL + key + model (OpenAI, Groq, OpenRouter, …).
  Explicitly the user's choice; transcription never leaves the machine, only the final text
  does, and only in this mode.
- **Local** — the user points molvi at their own on-device server (Ollama `/v1/chat/completions`,
  llama.cpp server, LM Studio). **Fully local/offline/private** when so configured.

Request shape (OpenAI Chat Completions-compatible):

```jsonc
POST {endpoint}/chat/completions
{ "model": "<model>", "messages": [
    { "role": "system", "content": "<prompt or molvi default>" },
    { "role": "user", "content": "<transcript>" }
  ], "temperature": 0 }
```

- Default system prompt fixes punctuation/case/grammar while preserving meaning, language,
  and the user's dictionary terms — never rephrases style. User can override `prompt`.
- HTTP client: **`ureq`** (sync, lean) on the finalize side-thread, with a timeout (e.g. 15s)
  and clear error surfacing (endpoint unreachable / 4xx / 5xx / parse). On failure, the UI
  shows the error and the **Raw** transcript is pasted + kept (never lost).
- **Privacy note (§14):** when Polished points at a cloud URL, transcript text leaves the
  machine. The UI states this explicitly at the endpoint field. The local-server path keeps
  it on-device. molvi itself ships no cloud default and performs no telemetry.

### 8.4 Paste-after-post-proc ordering

Smart is µs–ms (no perceptible delay). Polished is network-bound (cloud: hundreds of ms;
local server: more). The finalize side-thread blocks on it before paste — that is inherent
and the user opted in. The overlay shows a `polishing` phase (the existing `emit_phase`
widens to `kind: "polishing"`). There is no "paste raw then replace" — ponytail: one paste,
predictable.

---

## 9. Toggle Mode (`coordinator.rs`)

Phase-1 is PTT-only. Phase-2 branches on `settings.recognition_mode`:

- **PTT (default):** press-and-hold. Idle --press--> Recording --release--> Processing -->
  Idle (unchanged).
- **Toggle:** tap once --press--> Recording; tap again --press--> Processing --> Idle.
  The same `Command::Input { is_pressed: true }` is interpreted per mode. In Toggle, the
  release event is ignored (no-op). Debounce (30ms) still applies to suppress key-repeat.

The `Command::Input` already carries `push_to_talk: bool`; the handler uses
`settings.recognition_mode` (the clean-break rename of `push_to_talk`). Cancel (× button,
tray, second hotkey if mapped) aborts either mode. The overlay + tray Status reflect the mode.

---

## 10. Nemotron Viability Spike (item 6)

A **narrow, measurement-only** task whose sole question is *"can Nemotron 3.5 ASR Streaming
run real-time on the target CPU (i5-12450H)?"* — **not** "write Nemotron support."

### 10.1 Loader
**`parakeet-rs`** 0.3.7 (experimental dependency of the spike binary, NOT of `molvi`).
Verified 2026-08-03 (docs.rs/crates.io/ctx7 `/altunenes/parakeet-rs`): supports Nemotron
multilingual via `Nemotron::from_pretrained(dir, None)`; multilingual is **auto-detected**
from the encoder's `prompt_index` input; language is set with
`set_target_lang("ru-RU"|"en-US"|…|"auto")` where `"auto"` = prompt_index 101; an **offline
`transcribe_audio(&[f32]) -> String`** path exists (whole-buffer, for Phase-2) alongside the
cache-aware streaming path (Phase-3). transcribe-rs is **not touched** — migrating Nemotron
into transcribe-rs is justified only when **all three** hold: (a) the spike passes GO,
(b) Nemotron becomes a permanent molvi engine, (c) Phase-3 real streaming work begins. Until
then parakeet-rs is a temporary specialized backend. ⚠ parakeet-rs 0.3.7 pins
`ort 2.0.0-rc.13`; transcribe-rs 0.3.11 pulls `ort 2.0.0-rc.12` — harmless in the standalone
spike; flag if the two crates ever share a tree (Phase-3).

### 10.2 Deliverable
A `molvi-nemotron-spike/` reference binary (mirrors `molvi-task0/`) that:

- Loads a Nemotron ONNX export (candidate: `pantinor/nemotron-3.5-asr-streaming-0.6b-onnx`
  for parakeet-rs layout, or `onnx-community/...-int4` for size) via parakeet-rs.
- Runs a small multilingual clip set (EN/ES/DE/FR + a RU control) with known references.
- **Auto-generates a report** (markdown + JSON) with: cold-load ms, warm-load ms, per-
  utterance RTF, peak RSS, CPU utilization %, WER sanity per language, model id, language,
  clip length. Every subsequent decision reads from this report.
- **Still to verify at spike start:** the Nemotron model's license (HF pages say **NVIDIA
  Open Model License**, **not** the OpenMDW-1.1 the original brief asserted — confirm before
  integration) and the int4-vs-fp32 ONNX export choice (`pantinor/...-onnx` parakeet-rs layout
  vs `onnx-community/...-int4` for size). The parakeet-rs Nemotron API itself is verified.

### 10.3 GO / NO-GO gate (3 levels, stricter than Phase-1's 0.7)

| Verdict | RTF | Consequence |
|---|---|---|
| **GO** | `< 0.5` | Wire basic **non-streaming** multilingual in Phase-2 (parakeet-rs `transcribe_audio` whole-buffer, mirroring the GigaAM finalize path, behind a thin local adapter trait). |
| **Conditional GO** | `0.5 ≤ RTF < 1.0` | Wire it but **off by default**; the engine picker shows a warning: *"Multilingual mode is slower than Russian recognition."* |
| **NO-GO** | `≥ 1.0` | Do not integrate. Multilingual + streaming reconsidered in Phase-3 (other models / quantizations / EPs). |

### 10.4 What "basic multilingual" means at GO
- A **manual engine picker** in Recognition: `GigaAM (Russian)` / `Nemotron Multilingual`.
  The user chooses. **No automatic RU→GigaAM routing** (too early — language detection,
  mixed speech, debuggability questions). Auto-routing via `prompt_index=101` is deferred
  until its real-world quality is understood.
- parakeet-rs sits behind a thin local adapter trait (the seam Phase-1 §6.4 predicted) so
  the pipeline speaks one shape regardless of engine. Introduced **only at GO** (ponytail:
  no speculative trait while only GigaAM exists).

### 10.5 Explicitly NOT in Phase-2 (no "while we're at it")
Cache-aware streaming loop · streaming decoder · partial hypotheses · endpoint detection ·
speculative decoding · unified upstream `SpeechModel` · transcribe-rs migration. These are
Phase-3 (item 4 + full Nemotron streaming) — and only if the spike passed.

---

## 11. ort Threading Plan (item 5)

Goal: shave steady-state RTF without a fork unless measurement justifies it.

| Lever | Mechanism | Fork? |
|---|---|---|
| **P-core process affinity** | molvi enumerates P-cores via `GetLogicalProcessorInformationEx` (`ProcessorRelationship` with `EfficiencyClass == 0`; P-cores) and restricts the **whole process** to them via `SetProcessAffinityMask(GetCurrentProcess(), mask)`. ort spawns its own intra-op pool, so affinity on the worker thread alone does **not** reach it — **process affinity is the robust lever**. Side effect: ort detects the reduced CPU set and sizes its pool to the P-core count automatically. `windows` features: `Win32_System_Threading` + `Win32_System_SystemInformation` (verify exact features at the task). | **No** (molvi-local) |
| **Intra-op thread count** | After process affinity, ort auto-sizes its pool to the P-core count — no explicit knob needed. (`transcribe-rs::onnx::session::create_session_with_threads` exists but is NOT reachable through `GigaAMModel::load`; reachable only via a small upstream PR — unnecessary once process affinity is in place.) | **No** |
| **Sequential execution (`parallel_execution=false`)** | transcribe-rs's `build_session` is private; `create_session` hardcodes `parallel=true`. Exposing a full-options builder is a **tiny upstream PR** (`create_session_with_options(SessionOpts)`). | **Tiny PR — only if measured gain** |
| **Spin backoff (`with_intra_op_spinning(false)`)** | Same — currently only exposed for the XNNPACK path. | **Tiny PR — only if measured gain** |

**Measure-first posture:** current RTF is **0.029** (already ~34× realtime). The
0.029→0.022 gain from sequential+spin is **speculative**. Ship thread-count + P-core affinity
first (free wins), add a benchmark task that sweeps the four levers on the GigaAM CTC path,
and pursue the upstream PR **only if** the sweep shows a real gain. Ponytail: do not fork
before measuring.

---

## 12. Auto-Updater

- **Plugin:** `tauri-plugin-updater` 2.10.1 + `tauri-plugin-process` (for `app.restart()`).
  Check on startup (gated by `updater.check_on_startup`, default true) + manual **Check now**
  in Updates.
- **Signing (verified):** `cargo tauri signer generate -w ~/.tauri/molvi.key` produces a
  minisign keypair; the **public key is pasted inline** at `tauri.conf.json`
  `plugins.updater.pubkey` (NOT a file path); the **private key + password** are CI secrets
  (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). `bundle.
  createUpdaterArtifacts: true` emits the signed NSIS + `.sig`. The updater verifies the
  minisign signature before applying.
- **Hosting:** GitHub Releases. The Tauri-generated `latest.json` manifest
  (`{version,notes,pub_date,platforms."windows-x86_64".{signature,url}}`) is attached to the
  release; the artifact is the NSIS installer. `plugins.updater.endpoints` points at the
  `releases/latest/download/latest.json` URL. Free, fits the repo.
- **Apply flow:** `UpdaterExt::updater()?.check().await?` → `Option<Update>` →
  `download_and_install(on_chunk, on_download_finish).await?` → `app.restart()`
  (`download_and_install` does **not** auto-relaunch — `tauri-plugin-process` does).
- **Channel:** `stable` (Phase-2). No beta channel yet.
- **Offline:** if `check_on_startup` is off (or the check fails silently), molvi is fully
  offline. The updater is the **only** network activity besides the optional post-proc
  endpoint. No telemetry, no analytics.

---

## 13. Data Flow — PTT/Toggle lifecycle with post-proc + history (delta)

Phase-1 steps 1–6 unchanged (hotkey → coordinator → cpal → worker → finalize). Step 7
(paste) widens:

7. **Finalize side-thread (widened):**
   1. Receive final transcript from worker (Phase-1).
   2. **Post-process** per `post_processing.mode`: Raw (skip) / Smart (deterministic pipeline,
      §8.2) / Polished (HTTP endpoint, §8.3 — emit `phase { working, kind: "polishing" }`).
      On Polished failure: surface error, fall back to Raw, never lose the transcript.
   3. **Paste** (Phase-1: clipboard + Ctrl+V + focus guard + type fallback).
   4. **History insert** — *only if* `history.enabled` AND paste succeeded: insert the final
      (post-processed) text + metadata; prune to `max_entries` / `max_age_days`.
   5. Signal `ProcessingFinished` → coordinator Idle.

Privacy invariant (§14) holds: history insert is post-paste and opt-in; post-proc cloud is
explicit user config; nothing logged.

---

## 14. Privacy (HARD RULE, §10.1 — strengthened)

Carries Phase-1 §10.1 verbatim and adds the Phase-2 posture:

- **Never log** transcript text, partials, post-processed text, dictionary entries, history
  rows, or audio samples — at any level, not even `trace`. Logs carry metadata only. The
  Phase-1 log-privacy assertion test is **widened** to cover the new paths (post-proc, db).
- **History is opt-in and off by default.** Promise copy (UI + About):

  > *By default molvi stores no transcripts after they are pasted. History is optional,
  > local-only, and can be erased at any time.*

- **History** is plaintext local SQLite; retention caps (100 / 7d default); per-entry delete,
  Clear All, and Disable & Erase. The guarantee is *nothing stored without consent*, not
  encrypted storage.
- **Dictionary** is user-authored (no privacy concern); stored separately so Clear-history
  never touches it.
- **Post-processing:** when Polished points at a cloud URL, the final transcript text leaves
  the machine — the endpoint field says so explicitly. The local-server path (Ollama/
  llama.cpp) stays on-device. molvi ships **no cloud default** and performs **no telemetry**.
  The only other network call is the optional updater check.

---

## 15. Testing Strategy (Phase-2)

Ponytail-aligned: the smallest check that fails if the logic breaks. No framework sprawl.

- **`postproc.rs`** — a unit test **per Smart step** (merge, punct, repeated-marks,
  duplicate-words, dictionary-apply with phrase entries, case, fillers, numbers/dates,
  whitespace) + a full-pipeline test asserting determinism (same input ⇒ same output) and
  idempotence on re-application. RU + EN fixtures.
- **`dictionary.rs`** — CRUD round-trip, Import/Export (CSV + JSON) round-trip,
  case-insensitive + multi-word match, "Clear history does not touch dictionary".
- **`history.rs`** — insert + prune (over-`max_entries`, over-`max_age_days`), search,
  disable&erase drops the table + flips the flag. Use a temp DB path (no `tempfile` dep;
  inline helper).
- **`coordinator.rs`** — widen the Phase-1 state-machine tests: Toggle-mode transitions
  (tap→Recording, tap→Processing), PTT unchanged, Cancel in both.
- **`settings.rs`** — new fields default correctly; missing-keys still default via
  `#[serde(default)]`; no version field.
- **Polished (HTTP)** — mock endpoint (a tiny local listener or `mockito`-style) asserting
  request shape + error fallback to Raw.
- **Privacy/log test** — widen the Phase-1 assertion: run a transcript through finalize +
  post-proc (Smart + Polished-mock) + history insert and assert **no** transcript/dict/
  history substring appears in captured logs.
- **Spike** — the spike binary's own `cargo test`/report-generation self-check; not part of
  molvi's CI gate (separate crate, model-present-gated).
- **P-core affinity** — a smoke test that `p_core_mask()` returns a non-zero mask on a hybrid
  CPU and falls back gracefully (all-cores) on homogeneous CPUs or if the Win32 call fails;
  no perf assertion (measurement is a benchmark, not a unit test).
- **Updater** — a test against a mock manifest (valid + tampered → reject) verifying the
  pubkey check; no real download in CI.

Model-gated tests stay behind the `engine-model-test` feature; pure-logic CI stays fast.

---

## 16. Performance Budget (blaze preserved)

Phase-1 baselines (must not regress for the **default RU/PTT/Smart** user):

| Metric | Phase-1 baseline | Phase-2 budget |
|---|---|---|
| RTF (GigaAM e2e_ctc) | 0.029 | ≤ 0.029 (+ optional improvement from §11) |
| Cold-start to tray-ready | 1251 ms | ≤ ~1300 ms (settings DB open is lazy; updater check is async) |
| RSS | 292 MB | ≤ ~310 MB (rusqlite + two tiny DBs; no LLM) |
| NSIS installer | 9.43 MB | ≤ ~11 MB (bundled SQLite ~1 MB + start/stop wavs + inline SVGs; no font unless caption needs it) |

**Nemotron does not regress the default user:** `parakeet-rs` is a dep only of the spike
crate; the Nemotron model downloads **only** if the user picks that engine (and only after a
GO/Conditional-GO). RU/PTT users see no change. Polished mode adds no dependency weight (HTTP
client only); its cost is per-invocation latency, opted into explicitly.

---

## 17. Risk Register (Phase-2)

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Nemotron CPU-RTF not real-time-viable (RTF ≥ 1.0) | Med | Med | Spike measures first; NO-GO cleanly defers multilingual to Phase-3. RU users unaffected. |
| Nemotron model size (2.4 GB fp32 / ~600–700 MB int4) deters users | High | Med | int4 export preferred; download is opt-in (engine pick), progress UI, cached. RU default = GigaAM 214 MB. |
| Nemotron license is NVIDIA-OML, not the brief's OpenMDW | High (fact) | Low-Med | Verify at spike start; record license in `models/`; NOML is permissive enough for local use. Update docs copy. |
| rusqlite `bundled` build friction on Windows / MSRV bump | **Resolved** (verified 2026-08-03) | — | 0.40.1 `bundled` compiles the amalgamation via `cc` on MSVC, no system dep; no enforced MSRV (edition 2021). Risk retired. |
| Sequential+spin PR to transcribe-rs not accepted / lags | Med | Low | It's a pure win-saver, not load-bearing — thread-count + affinity ship regardless. Measure before pursuing. |
| Polished endpoint leaks transcript to cloud by user mistake | Med | Med (trust) | Explicit copy at the endpoint field; local-server path documented; no cloud default; key stored locally only. |
| History retention surprises users ("stored 6 months") | Med | Med (trust) | Tight 100/7d default; Disable & Erase panic button; honest promise copy. |
| Settings UI complexity creeps toward needing a framework | Low | Low | ~10 vanilla-TS components + tiny store; Preact only if a hard reactive problem appears. |
| Auto-updater signing key mishandled | Low | High | Private key only in CI secrets; document rotation; updater verifies pubkey. |
| Tray/menu API surface differs across Tauri 2.x point releases | Low | Low | ctx7/docs.rs verify `tauri::menu` at plan time (Tauri 2.11.5 pinned). |

---

## 18. Open Items for Phase-2 Kickoff

**Resolved 2026-08-03** by parallel subagent verification (ctx7 + docs.rs + crates.io +
windows-docs-rs + tauri-docs). Findings folded into §4 / §10 / §11:

- ✅ **rusqlite 0.40.1** — `bundled` static-links SQLite on MSVC, no MSRV issue, `Send`+`!Sync`.
- ✅ **tauri-plugin-autostart 2.5.1** — `ManagerExt::autolaunch()`, HKCU Run on Windows.
- ✅ **tauri-plugin-updater 2.10.1** — inline pubkey, env signing vars, `latest.json` schema, NSIS, `UpdaterExt` + `download_and_install` + `app.restart()` (via `tauri-plugin-process`).
- ✅ **tauri-plugin-dialog 2.7.2** — `DialogExt`/`FileDialogBuilder` for dictionary import/export.
- ✅ **tauri `menu` (2.11.5)** — `Menu/MenuItem/CheckMenuItem/PredefinedMenuItem::with_id`, `TrayIconBuilder::menu().on_menu_event(...)`, desktop-only (no extra feature).
- ✅ **ureq 3.3.0** chosen over reqwest — pure-Rust, no tokio, ~25 deps; `Agent`+`timeout_global`, distinct error variants.
- ✅ **parakeet-rs 0.3.7** — `Nemotron::from_pretrained`, `set_target_lang("auto"=101)`, offline `transcribe_audio` for Phase-2; `ort rc.13` clash noted.
- ✅ **windows 0.62.2** — `Win32_System_Threading` (`SetProcessAffinityMask`, `GetCurrentProcess`) + `Win32_System_SystemInformation` (`GetLogicalProcessorInformationEx` enumerates P-cores via `EfficiencyClass`). Mechanism = **process** affinity (worker-thread affinity doesn't reach ort's pool).

**Still open (resolve at the named task, not blockers):**

1. **cpal output** — output-stream build for short wav playback (cpal 0.18; verify tone
   playback latency is acceptable at the audio-feedback task).
2. **Nemotron model license** — confirm NVIDIA-OML terms at spike start; record in `models/`.
3. **Nemotron ONNX export choice** — `pantinor/...-onnx` (parakeet-rs layout) vs
   `onnx-community/...-int4` (size); decide in the spike.
4. **Auto-updater key generation + CI wiring** — generate the keypair, store the private key
   in CI secrets, publish the pubkey to `tauri.conf.json` at the updater task.

---

## 19. Phasing — Phase-3 preview (out of scope here)

- **Streaming + EOU + true sub-chunk partials** (Phase-1 item 4) — cache-aware streaming,
  once Nemotron streaming is proven; committed/tentative overlay styling becomes meaningful.
- **Full Nemotron streaming** + possible transcribe-rs migration (the 3-condition gate, §10.1).
- **Auto language routing** (`prompt_index=101`) after real-world quality evaluation.
- Silero neural VAD · DirectML EP option · i18n/RTL UI · macOS / Linux ports · CLI flags/
  Unix signals · overlay themes/top-bottom toggle/scroll-back · SQLCipher (if ever).

---

## 20. References (Phase-2 additions)

- **Handy** (frontend) — `cjpais/Handy` `src/components/settings/`, `ui/`, `model-selector/`,
  `onboarding/`, `update-checker/`: the same-stack peer confirming the IA, the post-proc API
  shape (Provider/BaseURL/ApiKey/Model/Prompts), and the AudioFeedback+SoundPicker grouping.
- **parakeet-rs** — `altunenes/parakeet-rs` (crates.io, docs.rs): Nemotron multilingual +
  `prompt_index` + cache-aware streaming loader (the spike's experimental backend).
- **Nemotron ONNX exports** — `pantinor/nemotron-3.5-asr-streaming-0.6b-onnx` (parakeet-rs
  layout), `onnx-community/nemotron-3.5-asr-streaming-0.6b-onnx-int4` (size).
- **transcribe-rs** — `cjpais/transcribe-rs` `src/onnx/session.rs`: `build_session`/
  `create_session_with_threads` (the ort threading levers, §11).
- **Fluent 2 Design System** — `fluent2.microsoft.design/color`: neutral/shared/brand
  palette discipline, Windows interaction states, semantic-color rule.
- **ui-ux-pro-max** (local skill) — Swiss Modernism 2.0 style, system-UI typography,
  accessibility/form UX rules (the targeted queries; `--design-system` misroute discarded).
- **rusqlite** — `docs.rs/rusqlite` (`bundled` feature).
- **tauri-plugin-updater / -autostart** — Tauri 2 plugin docs.

---

## 21. Glossary additions

- **Smart mode** — deterministic post-processing pipeline (no LLM); ms-fast, predictable.
- **Polished mode** — LLM post-processing via a user-supplied OpenAI-compatible endpoint
  (cloud or local server).
- **EOU** — End-Of-Utterance detection (Phase-3; drives when a streaming session finalizes).
- **prompt_index** — Nemotron's encoder input selecting the recognition language (101 = auto).
- **Cache-aware streaming** — RNNT inference reusing encoder/decoder state across chunks
  (Phase-3 for molvi; parakeet-rs already supports it).
- **NOML** — NVIDIA Open Model License (Nemotron's license; verify before integration).
- **P-core / E-core** — Performance / Efficiency cores (hybrid CPUs); affinity pins inference
  to P-cores to avoid E-core gating.
