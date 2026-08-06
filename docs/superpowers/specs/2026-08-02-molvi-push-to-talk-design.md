# molvi — Push-to-Talk Speech-to-Text Desktop App (Design Spec)

- **Date:** 2026-08-02
- **Status:** Draft, pending user review
- **Target platform (Phase 1):** Windows 11 (x64)
- **Future platforms:** Linux, macOS
- **Surface:** Tauri 2 desktop application (web UI, native Rust core)
- **Reference projects (ideas only):** [Handy](https://github.com/cjpais/Handy), [SilentKeys](https://github.com/gptguy/silentkeys), [Whispering](https://github.com/chand1012/whispering), Wispr Flow, Superwhisper, Windows Live Captions

---

## 1. Vision

**molvi** is a small, fast, privacy-first push-to-talk dictation app for Windows 11. Press and hold a global hotkey, speak, watch your words stream live in a floating caption bubble, release, and the punctuated transcript is pasted into whatever window was focused. All recognition runs **locally on the CPU** via ONNX Runtime — no cloud, no GPU required, no audio leaves the machine. Russian is a first-class target via GigaAM-v3; a second engine (NVIDIA Nemotron 3.5 ASR Streaming) is added later for true low-latency multilingual streaming.

The architecture is deliberately thin: one inference dependency (`transcribe-rs`) provides the speech model abstraction, streaming chunker, VAD, and ONNX feature extraction. We write only the Tauri shell, audio capture, hotkey, paste, overlay, and settings — the parts that are genuinely application logic.

### Design philosophy

- **Minimal / blazing-fast stack — measured, not marketed.** Fewest dependencies that solve the problem; smallest working diff; stdlib/platform features before new deps. "Blaze" is molvi's differentiator vs feature-rich Handy (Handy ships React + LLM post-processing + Apple Intelligence + onboarding + update-checker + model catalog; molvi ships vanilla TS + inference only), so it is a **measurable NFR**: cold-start (ms to tray-ready), RSS (MB), installer size (MB) — each benchmarked against Handy on the same machine and recorded before Phase-1 ships. Inference latency is engine-bound (same transcribe-rs/`ort`/CPU as Handy), so "blaze" lives in footprint and startup, not recognition speed.
- **Reuse over reinvent.** The `Transcriber` trait, streaming loop, VAD, and mel/CMVN feature extraction already exist in `transcribe-rs` — we do not reimplement them.
- **Calibration knobs stay.** Real audio hardware drifts and varies (sample rate, latency, mic sensitivity). Resampling, VAD thresholds, and chunk sizes remain tunable, not hardcoded to an ideal.
- **One thing well.** molvi transcribes speech and pastes it. LLM post-processing, history, cloud sync, etc. are explicitly out of scope for v1.

---

## 2. Goals & Non-Goals

### Phase 1 goals (MVP)

- Global push-to-talk hotkey (press-and-hold) on Windows 11.
- Capture microphone via `cpal`, resample to 16 kHz mono f32.
- Streaming partial transcription via `transcribe-rs` `VadChunked` + GigaAM-v3 `e2e_rnnt` (punctuation + text normalization).
- Live caption overlay: growing transcript text + caret, waveform, cancel, timer (committed/tentative styling arrives with the Phase-2 streaming engine).
- On release: finalize transcript and paste into the previously-focused window (clipboard-paste primary, keystroke-type fallback).
- First-run model download via `hf-hub` with progress UI.
- JSON-file settings (hotkey, paste mode, overlay, model, VAD sensitivity).
- System tray + single-instance.
- Robust error handling: the app never wedges; a failed transcription resets to idle.

### Phase 2 goals

- Second engine: NVIDIA Nemotron 3.5 ASR Streaming (true cache-aware streaming, 40 languages), switchable in settings.
- Settings UI (model selector, language, hotkey picker, VAD/overlay controls).

### Phase 3 goals (polish)

- Silero neural VAD (upgrade from EnergyVad where accuracy matters).
- Optional DirectML EP acceleration on Windows (for machines with a GPU/iGPU).
- Autostart, update checker (signed Tauri updater), transcription history (SQLite), i18n/RTL.
- Linux (X11/Wayland) and macOS ports.
- CLI remote-control flags (`--toggle`, `--cancel`), Unix-signal integration on Linux.

### Non-Goals (v1)

- Cloud transcription. Everything is local.
- GPU-only inference. CPU is the target; GPU is an optional accelerator later.
- LLM post-processing / "polish" pass.
- Speaker diarization, speaker identification.
- Streaming audio *output* / TTS / a voice-agent loop.
- Mobile.
- A browser-only web app (global hotkey, tray, and paste-into-any-app are impossible in a pure browser; this is why Tauri was chosen).

---

## 3. Key Decisions Log

These are the load-bearing decisions, with rationale, so future maintainers (and the implementation plan) can rely on them without re-deriving.

| # | Decision | Chosen | Rejected alternative | Rationale |
|---|---|---|---|---|
| D1 | App surface | **Tauri 2 desktop** | Browser web app; Electron | True push-to-talk requires a global hotkey, system tray, and paste-into-arbitrary-windows — impossible in a browser. Electron is heavy; Tauri gives a small native binary with a web UI. |
| D2 | Inference location | **Local CPU, ONNX Runtime** | Cloud NIM API; local GPU | Privacy (no audio leaves the machine), no per-minute cost, no internet dependency after first model download. CPU target because GigaAM-v3 is only ~220M params and runs faster-than-real-time on a modern CPU. |
| D3 | Inference runtime | **`transcribe-rs` (ONNX/`ort`)** | NIM Docker container; NeMo Python sidecar; raw `ort` + hand-rolled RNN-T | `transcribe-rs` already provides the `SpeechModel` trait, `VadChunked` streaming loop, VAD, and mel/CMVN feature extraction for GigaAM and Parakeet. No Python, no Docker. Caveat (from grilling, §6.4/§16): if its GigaAM loader doesn't cover the **3-file** e2e_rnnt RNN-T decomposition, we drive the three ONNX files via `ort` ourselves using transcribe-rs's low-level feature-extraction utils + a greedy transducer loop — still no Python/Docker. |
| D4 | Phase-1 model | **GigaAM-v3 `v3_e2e_ctc`** (primary, native); `v3_e2e_rnnt` (WER-gated fallback) | base `v3_rnnt` + rule-based punctuation; `e2e_rnnt` as primary | **Updated 2026-08-02 (source-verified):** transcribe-rs's `GigaAMModel` (`gigaam/mod.rs`) is **CTC-only, single-file** — it does *not* load the 3-file `e2e_rnnt` graph. `v3_e2e_ctc` (same 220M backbone, same e2e fine-tuning → punctuation + normalized text, single 225 MB int8 file) loads **natively with zero custom inference code**, restoring the "one inference dependency" thesis (§6.4). The "e2e" benefit D4 originally wanted from `e2e_rnnt` is delivered equally by `e2e_ctc`. `e2e_rnnt` is retained as a **fallback gated on measured WER** (Task 0): if `e2e_ctc` WER ≥ 0.15 on RU fixtures, adapt transcribe-rs's Parakeet RNN-T loop (`parakeet/mod.rs::decode_sequence`) to the 3 `e2e_rnnt` files. MIT license (`istupakov/gigaam-v3-onnx`). |
| D5 | Phase-2 model | **Nemotron 3.5 ASR Streaming** (via `transcribe-rs` Nemotron or `parakeet-rs`) | — | True cache-aware streaming (low-latency partials) + 40 languages. Complementary to GigaAM (Russian SOTA). Both plug into the same `SpeechModel` abstraction. |
| D6 | Core loop | **Streaming live partials + paste on release** | Record-then-transcribe (batch) | Uses the streaming strength of the engines; gives immediate visual feedback (overlay). |
| D7 | Resampling | **`rubato`** | Force cpal to 16 kHz; naive linear resampler | `transcribe-rs` strictly requires 16 kHz and has no resampler. Windows WASAPI shared-mode default is usually 48 kHz, not reliably 16 kHz. Naive resampling hurts ASR WER (no anti-aliasing); `rubato` is band-limited and matches the proven Handy approach. |
| D8 | Global hotkey | **`global-hotkey` 0.8** | `rdev`; raw Windows input hook | 0.8 explicitly targets push-to-talk and fixes the press/release event-order bugs. Provides both key-down and key-up. `rdev` is heavier and not needed. |
| D9 | Paste strategy | **Clipboard-paste (Ctrl+V) primary + keystroke-type fallback** | Always-type; always-clipboard | Clipboard-paste is reliable for Cyrillic/IME text and fast. Fallback covers terminals/games where Ctrl+V is intercepted. |
| D10 | Model distribution | **Download on first run (`hf-hub`)** | Bundle in installer | Small installer (~MB), model cached once (~150–250 MB int8), fully offline afterward. Avoids a bloated installer and lets us swap models without re-releasing the app. |
| D11 | Overlay default | **On (caption bubble)** | Off by default | Core to the chosen "streaming live" UX. Made safe by non-focusable, transparent, always-on-top window flags (mirrors Handy). |
| D12 | App identity | **molvi**, identifier `com.molvi.app` | — | Matches the project folder; short, brandable. |

---

## 4. Technology Stack

All versions are the latest on crates.io as of 2026-08-02. `ort` is a transitive dependency pulled by `transcribe-rs`; we do **not** pin it manually (let `transcribe-rs` drive it — `ort` 2.x is still release-candidate).

### Rust (crates.io)

| Layer | Crate | Version | Notes |
|---|---|---|---|
| App shell / IPC / tray / windows | `tauri` | 2.11.5 | + `tauri-cli` 2.11.4 (build) |
| Single instance | `tauri-plugin-single-instance` | 2.4.3 | Second launch forwards to running instance |
| Persistent settings (optional) | `tauri-plugin-store` | 2.4.4 | Alternative to raw JSON; pick one in implementation |
| Autostart (Phase 3) | `tauri-plugin-autostart` | 2.5.1 | Deferred |
| Dialogs (errors, first-run) | `tauri-plugin-dialog` | 2.7.2 | |
| Notifications | `tauri-plugin-notification` | 2.3.3 | |
| **Speech inference (one dep)** | `transcribe-rs` (`onnx` + `audio-features` features) | 0.3.11 | `SpeechModel`, `VadChunked`, VAD, mel/CMVN, GigaAM + Parakeet loaders. **Both features required:** `onnx` (model loaders) + `audio-features` (`compute_mel` / `features` / `decode`, used by the gigaam loader) |
| ONNX Runtime (transitive) | `ort` | 2.0.0-rc.12 | Pinned by `transcribe-rs` 0.3.11 at `=2.0.0-rc.12` (docs.rs-verified); do not pin manually |
| Phase-2 Nemotron engine (alt) | `parakeet-rs` | 0.3.7 | Only if `transcribe-rs` lacks Nemotron streaming |
| Audio capture | `cpal` | 0.18.1 | WASAPI on Windows |
| Resample → 16 kHz | `rubato` | 4.0.0 | Band-limited sinc; anti-aliasing for WER |
| Global hotkey (PTT) | `tauri-plugin-global-shortcut` + Handy-style local keys lib | latest | Follows Handy's proven PTT path (press + release events). Raw `global-hotkey` 0.8 deferred unless a concrete Windows failure appears |
| Keystroke injection | `enigo` | 0.6.1 | SendInput on Windows; Ctrl+V simulation + type fallback |
| Clipboard | `arboard` | 3.6.1 | Set clipboard for paste |
| Foreground-window control | `windows` | latest | `GetForegroundWindow` / `SetForegroundWindow` for paste target (Win32) |
| Model download | `hf-hub` | 1.0.0 | First-run fetch + resume |
| Async runtime | `tokio` | 1.53.1 | |
| Serde | `serde`, `serde_json` | 1.0.229 | Settings, events |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | latest | File logs for diagnostics |

### Frontend

| Tool | Version source | Notes |
|---|---|---|
| Vite | npm (`package.json`) | Not a Rust crate; the crates.io `vite` 0.1.0 is an unrelated placeholder |
| TypeScript | npm | |
| UI approach | **Vanilla TS + a tiny signal store** (Phase 1) | Add Preact later only if reactivity demands it |

### Toolchain

- **Rust:** latest stable (MSRV dictated by Tauri 2.11 + `transcribe-rs` 0.3.11; expected ≥ 1.80). Confirm in `rust-toolchain.toml`.
- **C/C++ toolchain:** MSVC (Tauri/`ort` native build on Windows). Visual Studio Build Tools.
- **WebView2:** bundled/present on Windows 11 (Tauri runtime).
- **Node/npm:** for the Vite frontend build.

---

## 5. Architecture Overview

```
                ┌─────────────────────────────────────────────────────────────┐
                │                        TAURI MAIN THREAD                     │
                │   webview event loop · tray · IPC commands · event emit      │
                └──────▲───────────────────────────────────────▲──────────────┘
                       │ stream-text / mic-level / phase        │ final text
                       │ (Tauri emit)                           │
                ┌──────┴─────────────────┐          ┌───────────┴──────────────┐
                │  INFERENCE WORKER       │          │  PASTE (on main)          │
                │  owns ort Session       │          │  restore focus → clipboard│
                │  + transcribe-rs model  │          │  → Ctrl+V → type fallback │
                │  + VadChunked           │          └───────────────────────────┘
                └──────▲─────────────────┘
                       │ 16 kHz mono f32 frames (SPSC ring)
                ┌──────┴─────────────────┐
                │  AUDIO (cpal thread)    │
                │  capture → f32 → levels │
                └──────▲─────────────────┘
                       │ Command::Input / Cancel / ProcessingFinished
                ┌──────┴──────────────────────────────────────────────────────┐
                │  COORDINATOR THREAD (single-threaded state machine)          │
                │  Idle → Recording → Processing → Idle                        │
                │  serializes hotkey / cancel / pipeline events (mpsc)         │
                └──────▲──────────────────────────────────────────────────────┘
                       │ press / release
                ┌──────┴─────────────────┐
                │  global-hotkey (OS hook)│
                └────────────────────────┘
```

Four cooperating threads, each with a single clear job, communicating through narrow channels:

1. **Tauri main thread** — owns the webview(s), tray, IPC commands, and is the only thread that emits Tauri events to the frontend and manipulates windows.
2. **Coordinator thread** — a dedicated `std::thread` running an `mpsc::channel` select loop. It is the single owner of the lifecycle state machine. All input (hotkey press/release, cancel, processing-finished) funnels through it as `Command` values. This eliminates races between concurrent shortcut events and the async transcribe/paste pipeline (Handy's proven pattern).
3. **cpal audio thread** — cpal's data callback. Must be non-blocking. Writes captured PCM into a SPSC ring buffer consumed by the worker; periodically computes mic-level buckets for the waveform.
4. **Inference worker thread** — owns the `ort` sessions (RNN-T = three: encoder, decoder, joint; `ort` sessions require `&mut self` and ONNX Runtime EP allocators are not thread-safe, so all sessions for one inference path live on this one thread), the `transcribe-rs` model, and the `VadChunked` chunker. Pulls PCM, resamples if needed, feeds the chunker, emits partials.

---

## 6. Components (Rust modules)

Target layout, kept flat (one file per concern). Files stay small and focused; a file growing past ~400 lines is a signal it has taken on a second responsibility.

```
src-tauri/src/
  main.rs            — bootstrap: Tauri setup, plugins, tray, single-instance,
                       spawn coordinator + worker, manage state
  coordinator.rs     — lifecycle state machine (mpsc, catch_unwind)
  audio.rs           — cpal input stream, format → f32, mic-level metering,
                       SPSC ring buffer handoff
  resample.rs        — rubato 48k→16k (skipped/no-op if device is already 16k)
  engine.rs          — transcribe-rs wrapper: load model, VadChunked config,
                       worker loop, partial/final channels
  hotkey.rs          — global-hotkey registration (press + release → coordinator)
  paste.rs           — foreground-window capture/restore + clipboard-paste +
                       keystroke-type fallback
  overlay.rs         — overlay webview window create/show/hide; non-focusable,
                       transparent, always-on-top flags
  settings.rs        — serde model, JSON load/save, schema, defaults
  model_store.rs     — hf-hub download (resume + progress), cache paths,
                       quantization selection
  paths.rs           — app data dir resolution (Windows %APPDATA%\com.molvi.app)
  errors.rs          — error enum, thiserror, user-facing messages
  log.rs             — tracing init, file appender
```

### 6.1 `coordinator.rs` — lifecycle

Adapted from Handy's `TranscriptionCoordinator` (proven, race-free). A single `std::thread` owns the `Stage` state machine and reads commands from an `mpsc::channel`.

```rust
enum Command {
    Input { binding_id: String, is_pressed: bool, push_to_talk: bool },
    Cancel,
    ProcessingFinished,
}

enum Stage { Idle, Recording, Processing }
```

Behavior:
- **Idle + press (PTT)** → `start()`: capture foreground window handle, start cpal stream, reset worker session, show overlay (state `recording`), transition `Idle → Recording`.
- **Recording + release (PTT)** → `stop()`: signal worker to finalize, transition `Recording → Processing`, emit phase `working`.
- **Recording/Processing + Cancel** → abort worker, hide overlay, transition → `Idle`.
- **ProcessingFinished** (worker sent final text) → run paste on main, transition `Processing → Idle`.
- Press events are debounced (~30 ms) to suppress key-repeat / double-tap. Releases always pass through (PTT semantics).
- The whole loop is wrapped in `std::panic::catch_unwind` so an inference panic resets to `Idle` instead of killing the app.

The coordinator is the only writer of `Stage`; `start`/`stop` are free functions that call into the action map and audio/engine handles looked up from Tauri managed state.

### 6.2 `audio.rs` — capture

- Uses `cpal::default_input_device()` (or device from settings).
- Reads the device's default/SupportedStreamConfig; notes native sample rate (often 48 kHz).
- Builds an input stream whose data callback:
  1. Converts samples to `f32` mono (downmix if stereo).
  2. Pushes into a `rtrb` (lock-free SPSC) ring buffer toward the worker.
  3. Every N frames, computes a small energy/FFT bucket vector (16 buckets) and forwards to main as `mic-level` events for the waveform.
- Stream config picks a low-latency buffer size; tunable via settings if glitches/underruns appear.
- The callback must never block — no allocation in the hot path beyond ring writes.

### 6.3 `resample.rs` — rubato

- If the device runs at 16 kHz → fast-path no-op (frames pass through).
- Otherwise → a `rubato::FftFixedIn` resampler 48 kHz (or 44.1) → 16 kHz. The worker owns the resampler and drains the ring buffer in chunks it can convert.
- This is the calibration knob for "real audio devices vary"; it stays even if most devices happen to be 16 kHz.

### 6.4 `engine.rs` — transcribe-rs wrapper & worker

Owns the inference session(s) on a dedicated thread (the **worker**). Responsibilities:
- Load the configured model. **Primary path (Task 0-A, native):** `v3_e2e_ctc` via `transcribe_rs::onnx::gigaam::GigaAMModel::load` — single `ort` session, CTC greedy decode, loads one `model.int8.onnx` + `vocab.txt`. No custom inference code; this is the default. **Fallback path (Task 0-B, only if `e2e_ctc` WER ≥ 0.15):** `v3_e2e_rnnt` — a 3-file graph. `istupakov/gigaam-v3-onnx` ships it as `v3_e2e_rnnt_{encoder,decoder,joint}.int8.onnx` (+ `v3_e2e_rnnt.yaml` + `v3_e2e_rnnt_vocab.txt`). `GigaAMModel::load` does **not** handle this (confirmed from source: CTC-only), so the fallback drives the three ONNX files directly via `ort`, reusing transcribe-rs's public `compute_mel` (`pub mod features`, MelConfig: 64 mels / n_fft 320 / hop 160 / Hann / f_max 8000) plus a greedy RNN-T transducer loop **ported from `parakeet/mod.rs::decode_sequence`** (`DecoderState = (Array3<f32>, Array3<f32>)`, blank-token logic, `MAX_TOKENS_PER_STEP`), adapted from Parakeet's preprocessor+encoder+decoder_joint layout to GigaAM's compute_mel+encoder+decoder+joint layout. Verify against `v3_e2e_rnnt.yaml` whether LFR/CMVN stages apply. Either way the model artifacts come from `model_store` under `models/gigaam-v3-e2e-{ctc,rnnt}/`.
- Build VAD + chunker once per session:
  ```rust
  let inner = EnergyVad::new(480, 0.01);                 // 30 ms frame @16k
  let vad   = SmoothedVad::new(Box::new(inner), 15, 15, 2);
  let mut chunker = VadChunked::new(
      Box::new(vad),
      VadChunkedConfig {
          min_chunk_secs: 1.0,                            // tunable (latency vs punctuation)
          max_chunk_secs: 20.0,                           // < 25 s safety cap
          padding_secs: 0.1,
          smart_split_search_secs: Some(3.0),
          merge_separator: " ".into(),
      },
      TranscribeOptions { language: Some("ru".into()), ..Default::default() },
  );
  ```
- Worker loop (each iteration):
  1. Drain ring buffer → resample to 16 kHz mono f32 → accumulate into a **fixed 480-sample (30 ms) hop buffer** before feeding the VAD (rubato emits blocks; the VAD consumes exact 480-sample frames).
  2. `chunker.feed(&mut model, &chunk)?` → `Vec<TranscribeResult>`.
  3. Append finalized chunk text to the running transcript string and send the whole growing string to main as a `stream-text` event (see §9 — Phase 1 has no within-chunk "tentative" because the model is offline; text grows one finalized chunk at a time).
  4. Repeat until a `Finalize` signal arrives.
- On `Finalize`: `chunker.finish(&mut model)?` → final text → send to main for paste → call `chunker`/`model` reset for next session.
- Because the model is an offline RNN-T, "partial" text is emitted at chunk boundaries (~`min_chunk_secs`, default 1.0 s). Each chunk is **final** when it arrives; there is no sub-chunk refinement on GigaAM. The caption therefore grows in ~1 s increments. True tentative refinement arrives only with the Phase-2 Nemotron streaming engine — at that point the committed/tentative dual styling becomes meaningful.
- **RTF (real-time-factor) guard:** measure GigaAM e2e RTF on the target CPU in the first Phase-1 milestone. If RTF > ~0.7 (i.e. inference can't comfortably keep up), the worker switches to **degraded mode**: it stops emitting live partials, shows the overlay in a `processing` state while recording, and finalizes once on release (batch). Streaming UX is gated on RTF being adequate; we never present stale/lagging "live" text.
- `EnergyAdaptiveChunked` is available in `transcribe-rs` as a fallback chunker if `SmoothedVad` misbehaves on a given mic.

**Why we don't write a `Transcriber` trait ourselves:** `transcribe-rs`'s `SpeechModel` trait already abstracts GigaAM and Parakeet. For Phase 1 (one model) there is exactly one implementation behind it — adding our own trait would be a speculative abstraction. We introduce a thin local adapter trait **only** if Phase 2's Nemotron uses a *different crate* (`parakeet-rs`) with an incompatible API. Until then, YAGNI.

### 6.5 `hotkey.rs` — global hotkey

- **Follow Handy's proven PTT path (updated 2026-08-02).** This spec originally chose raw `global-hotkey` 0.8 to fix PTT release-event bugs, but Handy — our proven reference — *already* solves reliable PTT press+release with `tauri-plugin-global-shortcut` plus a custom `handy-keys` lib (runtime-swappable). Reuse beats reinvent: we adopt Handy's approach and **do not introduce `global-hotkey` 0.8** unless a concrete Windows failure appears. (Ponytail: use what Handy runs; revisit only with evidence.)
- Concrete Phase-1 choice: start with whichever single mechanism Handy defaults to on Windows (`handy-keys` primary path). **Defer the runtime-swappable abstraction** (Handy's two-backend swap layer) — YAGNI for a one-platform MVP; add only if a real second-backend need emerges.
- Register the user's binding. On Windows the manager must live on a thread running a Win32 message pump; verify at kickoff whether Tauri's main thread satisfies this or we need a dedicated thread (Handy runs it integrated — strong signal it works).
- On key-down → `coordinator.send_input(binding, is_pressed=true, push_to_talk=true)`.
- On key-up → `coordinator.send_input(binding, is_pressed=false, push_to_talk=true)`.
- Default binding: configurable; pick something unlikely to clash (e.g. `Alt+\``) and make it user-configurable.
- Re-registration after config change, and graceful handling of "hotkey already taken by another app" (surface a clear error).
- **Open Phase-1 verification (in-line, Task 3):** integration test that press **and** release both fire on Windows for the chosen binding.

### 6.6 `paste.rs` — paste into focused window

**Primary path relies on the overlay being non-focusable, so the paste target keeps keyboard focus throughout** — no focus "restore" is needed, and we deliberately avoid `SetForegroundWindow` because Windows foreground-lock makes it silently fail when the caller isn't already foreground.

Sequence on finalize:
1. **Hide the overlay** (visual cleanup; it never had focus anyway).
2. Set clipboard text via `arboard`.
3. Simulate `Ctrl+V` via `enigo` — the keystroke is delivered to whatever window currently has focus, which (because the overlay is `focusable:false`) is still the original paste target captured at hotkey-press.
4. **Fallback layer 1 — `SetForegroundWindow(target)`:** if a focus check (`GetForegroundWindow() == target`) shows focus was lost (e.g. the user clicked away, or the non-focusable flag didn't hold on some WebView2 build), *then* attempt `SetForegroundWindow(target)` + a ~40 ms settle, and re-send `Ctrl+V`. This is a best-effort heuristic, not the primary mechanism.
5. **Fallback layer 2 — type:** for surfaces that intercept `Ctrl+V` (terminals, some games) or a configurable "always type" app list, simulate typing the text via `enigo`. Cyrillic via clipboard-paste is reliable; type-fallback exists for these exceptions.
- Settings expose paste mode: `clipboard` (default), `type`, or `auto`.
- **Invariant:** the app must never take foreground from the target during a session. If it ever does (modal error dialog, settings window opened mid-record), paste is routed to clipboard + a toast rather than risk pasting into the wrong window.

### 6.7 `overlay.rs` — caption bubble window

- A separate Tauri webview window (its own `index.html`, `overlay.ts`) — independent from the settings window so it can be transparent, borderless, and dismissed independently.
- **Window flags (Windows):** `decorations: false`, `transparent: true`, `always_on_top: true`, `skip_taskbar: true`, `resizable: false`, `focused: false`, and crucially **`focusable: false`** so it never steals focus from the paste target (`setFocusable` / window config — ctx7-verified to exist in Tauri 2). Position: bottom-center; position computed to clear the taskbar. On Windows, also set **`noRedirectionBitmap: true`** to mitigate the white flash that transparent windows exhibit on creation (Tauri 2 docs, ctx7-verified).
- Show on `recording`/`streaming`; hide before paste.
- The Rust side emits: `show-overlay {state}`, `hide-overlay`, `stream-text {text}` (Phase 1 growing string; widens to `{committed, tentative}` in Phase 2), `mic-level [f32;16]`, `phase {listening|working, kind}`.
- **Open Phase-1 verification:** confirm `focusable: false` + `transparent: true` render correctly on Windows WebView2 in the Tauri 2.11 build. Handy achieves this; we mirror their approach but must verify on our toolchain.

### 6.8 `settings.rs` — configuration

JSON file at `%APPDATA%\com.molvi.app\settings.json`. Schema (Phase 1):

```jsonc
{
  "version": 1,
  "hotkey": "Alt+`",
  "push_to_talk": true,
  "model": "gigaam-v3-e2e-rnnt",
  "language": "ru",
  "paste_mode": "clipboard",       // "clipboard" | "type" | "auto"
  "overlay": {
    "enabled": true,
    "position": "bottom",          // Phase 3: also "top"
    "show_waveform": true,
    "show_timer": true
  },
  "audio": {
    "input_device": null,          // null = default
    "buffer_frames": null          // null = cpal default
  },
  "vad": {
    "min_chunk_secs": 1.0,
    "max_chunk_secs": 20.0,
    "padding_secs": 0.1,
    "energy_threshold": 0.01
  },
  "logging": { "level": "info" }
}
```

Defaults are mergeable; missing keys take defaults. Schema versioning (`"version"`) for future migrations.

### 6.9 `model_store.rs` — model download & cache

- Models live in `%APPDATA%\com.molvi.app\models\<id>\` (`model.onnx` or `model.int8.onnx`, plus `vocab.txt` / token files).
- First run (or when the configured model is missing): `hf-hub` downloads `istupakov/gigaam-v3-onnx` (e2e_rnnt files), int8 quantization, with a progress event to the UI.
- Resume on interruption (`hf-hub` supports it). Verify checksums if the source provides them.
- Cache is reused across runs; "delete model" command in settings to re-download.
- Quantization: default **int8** for CPU size/speed; fp32 available as a quality fallback.

---

## 7. Data Flow — Push-to-Talk Lifecycle (end to end)

1. **Hotkey down** (global-hotkey, OS hook thread) → `Command::Input { is_pressed: true, push_to_talk: true }` → coordinator.
2. **Coordinator (Idle → Recording):**
   - `GetForegroundWindow` → store `target` window handle for later paste.
   - Start cpal stream (if not running), reset `VadChunked` chunker + model state.
   - Emit `show-overlay { state: "recording" }` to overlay window.
3. **cpal callback (audio thread):**
   - Convert frames → f32 mono → push to SPSC ring.
   - Periodically compute 16 mic-level buckets → main thread → emit `mic-level`.
 4. **Worker loop (inference thread):**
   - Drain ring → resample to 16 kHz (rubato, or no-op) → accumulate into 480-sample frames.
   - `chunker.feed(&mut model, chunk)` → finalized chunk text → append to the running transcript string.
   - Emit `stream-text { text }` (the full growing transcript so far); overlay renders it + caret.
5. **Hotkey up** → `Command::Input { is_pressed: false }` → coordinator (Recording → Processing):
   - Emit `phase { listening→working, kind: "transcribing" }`.
   - Send `Finalize` to worker.
6. **Worker finalize:** `chunker.finish(&mut model)` → final text → send to main.
 7. **Paste (main thread):** hide overlay → check `GetForegroundWindow() == target` (should hold — overlay is non-focusable) → set clipboard (`arboard`) → `Ctrl+V` (`enigo`). On focus mismatch: try `SetForegroundWindow(target)` + settle + retry; if still no good, clipboard + toast. Terminals/games in the "always type" list → `enigo` type fallback (full ladder in §6.6).
8. **Coordinator (Processing → Idle)** via `ProcessingFinished`. Ready for next utterance.

---

## 8. Threading & Concurrency Model

| Thread | Owns | Blocking? | Communication |
|---|---|---|---|
| Tauri main | webviews, tray, AppHandle, windows | event loop | emits events; receives final text from worker |
| Coordinator | `Stage` state machine | `mpsc::recv` loop | `Command` inbox from hotkey/cancel/finished |
| cpal audio | (none of our own) | real-time callback | writes SPSC ring; sends mic-level |
| Inference worker | `ort` sessions (RNN-T = 3: encoder/decoder/joint), model, chunker | drain + infer loop | reads SPSC ring; sends partials + final |

Channels:
- `hotkey → coordinator`: `std::sync::mpsc::Sender<Command>` (simple, single consumer).
- `cpal → worker`: `rtrb` SPSC ring of `f32` (lock-free, real-time-safe writes).
- `worker → main`: partial/final via `tauri::AppHandle::emit` (callable from any thread in Tauri 2) — no extra channel needed; for the *final* result we additionally signal the coordinator via its existing `mpsc` (`ProcessingFinished`) after paste.
- `main → overlay`: Tauri events (`stream-text`, `mic-level`, `phase`, `show-overlay`, `hide-overlay`).

Shared state is held as `Arc<...>` in Tauri managed state; the only mutable runtime state (`Stage`, the recording flag) lives behind the coordinator's single thread, so there is no locking on the hot path.

---

## 9. Overlay UX

Inspired by Handy, Whispering, Wispr Flow, Superwhisper, Windows Live Captions. Phase 1 delivers the minimum that supports the "streaming live + paste" loop; richer interactions are deferred.

### Visual

- A **caption bubble** anchored bottom-center, clearing the taskbar.
- **Phase 1 (GigaAM, offline):** a single growing text line. Each finalized ~1 s chunk appends to the string, with a blinking caret at the end while listening. There is **no `tentative` styling in Phase 1** — the model emits only finalized chunks, so dual committed/tentative styling would be a lie. (The committed/tentative split becomes real and worth styling only in Phase 2 with Nemotron's true streaming partials.)
- A small **status row**: pulsing dot + simple waveform (driven by `mic-level`) + elapsed timer + cancel button (×).
- On release: brief spinner labeled "Transcribing", then the bubble hides and text is pasted.

### States

| State | Trigger | Overlay shows |
|---|---|---|
| `recording` | hotkey down (before first partial) | dot + waveform + timer + cancel |
| `streaming` | first partial arrives | growing caption text + caret + waveform + timer + cancel |
| `transcribing` | hotkey up | spinner + "Transcribing…" + cancel |
| (hidden) | paste complete | closed |

### Events (Rust → frontend)

- `show-overlay { state }`
- `hide-overlay`
- `stream-text { text: string }`  — Phase 1: the full growing transcript so far (one finalized chunk appended per emit). Phase 2 (Nemotron): widens to `{ committed, tentative }`.
- `mic-level { buckets: [f32;16] }`  (throttled, e.g. ~30 fps)
- `phase { phase: "listening"|"working", kind: "transcribing"|"polishing" }`

### Frontend actions

- `cancelOperation()` → coordinator `Cancel`.
- (Phase 3) scroll-back within session; top/bottom position toggle; theme.

### Focus safety

The overlay is `focusable: false` + `always_on_top` + `skip_taskbar`, so the paste target **retains keyboard focus for the entire session** — paste is then just clipboard-set + `Ctrl+V` to the still-focused target (see §6.6). This non-focusable invariant is the single most important Windows-specific correctness rule of the app: if it ever breaks, `Ctrl+V` goes to the wrong window. The app additionally checks `GetForegroundWindow() == target` before sending `Ctrl+V` and, on mismatch, routes to clipboard + toast rather than risk mispasting.

---

## 10. Error Handling

Every failure has a defined recovery; the app never remains stuck in `Recording`/`Processing`.

| Failure | Detection | Recovery |
|---|---|---|
| Model missing / download failed | `model_store` returns error | Settings shows error + retry/resume; engine stays `Idle` |
| Mic unavailable / permission denied | cpal stream build error | Overlay + tray error; link to Windows mic privacy settings |
| Inference panic | `catch_unwind` in worker | Coordinator resets to `Idle`; log + notify |
| No foreground target / target closed | `GetForegroundWindow` null or paste refused | Fall back to copying transcript to clipboard + toast |
| Hotkey already registered by another app | `global-hotkey` error | Surface clear message; fall back to configurable secondary |
| Audio underrun / glitch | cpal callback | Log; auto-increase buffer on repeated underruns (Phase 3) |
| Settings file corrupt | serde parse error | Back up corrupt file, load defaults, notify |

### 10.1 Privacy & logging discipline

molvi is "privacy-first" (all recognition local). That promise is voided if logs leak transcript content to disk. Rule, enforced by code review and a logging test:

- **Never log transcript text, partial transcripts, or audio samples** at any log level — not even `trace`. Logs carry only metadata: stage transitions, chunk counts, durations (ms), error types, model id, RTF measurements.
- The `stream-text` payload is emitted to the overlay over IPC only; it is never written to the log file.
- Audio PCM is held in memory and dropped after transcription; it is never persisted unless a future opt-in debug-recording mode is added (Phase 3, behind an explicit toggle, default off).
- Transcript history (Phase 3) is opt-in and stored in a user-controlled file; absent that, nothing a user says is retained after paste.

---

## 11. Model Choice Deep-Dive

### Phase-1 primary: GigaAM-v3 `v3_e2e_ctc` (native)

**Updated 2026-08-02 (source-verified):** transcribe-rs's `GigaAMModel` (`gigaam/mod.rs`) is CTC-only and single-file, so it loads `v3_e2e_ctc` natively. `v3_e2e_ctc` is the **same 220M backbone** with the **same e2e fine-tuning** (punctuation + text normalization) as `e2e_rnnt` — the "e2e" benefit D4 originally wanted is delivered equally by the CTC variant. Single 225 MB int8 file, one `ort` session, zero custom inference code → restores the "one inference dependency" thesis (§6.4). The remaining open question is **WER (CTC vs RNN-T)** — measured by Task 0 against the §13 threshold (0.15). Cross-chunk punctuation behavior (below) applies identically, since both e2e variants punctuate per chunk.

**WER evidence (official, `evaluation.md`, verified 2026-08-02):** e2e_ctc average WER = **12.0%** (Golos Farfield 6.1 / Golos Crowd 9.7 / Russian LibriSpeech 6.4 / Common Voice 19: 3.2) vs e2e_rnnt **11.2%** — a **~0.8-point gap** that does **not** justify a custom 3-file RNN-T pipeline. Clean dictation speech is 3–10%, comfortably under the §13 0.15 gate. This is the evidence base for making e2e_ctc primary; the rnnt fallback (Task 0-B) is retained only as a safety net, not the expected path. Rigorous golden-clip WER on molvi's own fixtures lands in Task 11.

### GigaAM-v3 `e2e_rnnt` (WER-gated fallback — Task 0-B)

- **What:** Sber's conformer foundation model (220M params), fine-tuned end-to-end RNN-T with **punctuation and text normalization**, SOTA on Russian.
- **Why not base `v3_rnnt` + rule-based punctuation:** `e2e_rnnt` is the same backbone, same size and speed; punctuation built-in is strictly better for paste UX, and rule-based Russian punctuation is poor.
- **License:** MIT (confirmed on the model card).
- **Source:** `istupakov/gigaam-v3-onnx` on HuggingFace (provides `ctc`, `rnnt`, `e2e-ctc`, `e2e-rnnt`; we use e2e-rnnt, int8).
- **Input format:** 16 kHz mono PCM (feature extraction — mel + CMVN — handled internally by `transcribe-rs`).
- **Streaming character:** offline RNN-T; "streaming" is achieved by VAD-chunked inference. Partials arrive at chunk boundaries (~`min_chunk_secs`), not sub-second — each chunk is final when it arrives. This is the documented UX expectation (§9).
- **Cross-chunk punctuation risk (GigaAM-specific):** e2e punctuates **per chunk**. With short (~1 s) chunks, output can be choppy / over-punctuated (e.g. each chunk capitalized and period-terminated). Two mitigations, both tunable: (a) raise `min_chunk_secs` (longer chunks → better punctuation context, worse latency) — this is the primary knob; (b) a light post-pass that strips inter-chunk sentence-final punctuation except at true pauses (kept optional, Phase-1 tuning). The chunk-size vs punctuation trade-off must be measured on real Russian speech at kickoff.
- **"25-second limit" (corrected):** that number is the Python `gigaam.transcribe()` high-level helper's limit, **not** the ONNX model's. Our chunker caps chunks at ≤20 s anyway (and VAD splits far more often), so neither the helper limit nor any plausible encoder receptive-field limit is ever approached; arbitrary-length speech is supported.

### Nemotron 3.5 ASR Streaming (Phase 2)

- **What:** NVIDIA 600M-param cache-aware streaming conformer-transducer, 40 languages, true low-latency partials.
- **Why:** Complementary — true streaming (vs GigaAM's chunked approach) and multilingual.
- **Engine (decide at Phase-2 kickoff):** prefer `transcribe-rs` Nemotron support (uniform API, zero new abstraction); else `parakeet-rs` 0.3.7 behind a thin local adapter trait.
- **Source:** `smcleod/nemotron-3.5-asr-streaming-0.6b-int8` (laid out for `parakeet-rs` ≥0.3.6) or `onnx-community/...-onnx-int4`.

---

## 12. Build, Toolchain & Project Bootstrap

- `rust-toolchain.toml`: pin stable channel; record MSRV.
- `cargo create-tauri-app` equivalent → Tauri 2 project `com.molvi.app`, two windows (settings, overlay), tray.
- Frontend: Vite + TS; vanilla Phase 1.
- Build: `cargo tauri build` (produces NSIS/MSI on Windows). Debug build via `cargo tauri dev`.
- Dependencies in `Cargo.toml` with the versions in §4. Feature flags: `transcribe-rs/onnx` + `transcribe-rs/audio-features` (the gigaam loader uses `compute_mel` from the `features` module, gated behind `audio-features`).
- Native deps: MSVC build tools; WebView2 (Win11 built-in).
- `AGENTS.md` will record the canonical commands (`cargo tauri dev`, `cargo test`, lint).

---

## 13. Testing Strategy

Ponytail-aligned: the smallest check that fails if the logic breaks. No heavy framework sprawl; `cargo test` with focused unit/integration tests.

- **`engine.rs`:** golden clips — a few short Russian WAV files with known reference transcripts → compare via **WER (word error rate)** after normalization (lowercase, strip punctuation, collapse whitespace), not raw substring match (Russian morphology makes substring matching flaky). Assert WER < an explicit threshold (e.g. 0.15). This is the one place we tolerate an ASR-flakiness budget, and we quantify it.
- **`coordinator.rs`:** state-machine transitions — feed `Command` sequences, assert `Stage` sequence (Idle→Recording→Processing→Idle), cancel mid-flight, debounce.
- **`resample.rs`:** known sine-wave at 48k → 16k, assert output rate and approximate amplitude/anti-aliasing.
- **`settings.rs`:** default-merge, schema migration, corrupt-file recovery.
- **`paste.rs`:** clipboard set + paste-command sequencing (mocked window focus on CI).
- **`log.rs` (privacy):** an assertion-based test that plumbing a transcript through the engine path produces **no transcript substring** in the captured log output (enforces §10.1).
- **Self-check:** each non-trivial module has a small `#[test]` or a `demo()`/`__main__` assert. Trivial one-liners are not tested.

Golden Russian clips: record two or three short utterances locally, store under `tests/fixtures/`, commit reference transcripts. **CI note:** engine tests require the model present (~200 MB download); gate them behind a feature flag so pure-logic CI runs stay fast and model-free.

---

## 14. Phasing / Roadmap

### Phase 1 — MVP (Windows 11)
Tauri shell · coordinator · cpal+rubato · transcribe-rs GigaAM-v3 e2e_rnnt + VadChunked · global-hotkey PTT · overlay (growing caption + waveform + cancel + timer) · clipboard-paste+fallback (non-focusable invariant) · hf-hub first-run download · JSON settings · tray · single-instance · error recovery · privacy-logging discipline. No settings UI (file-edited), no second model.

### Phase 2 — Second engine + UI
Nemotron 3.5 ASR Streaming via transcribe-rs (or parakeet-rs adapter) · model/language switcher settings UI · hotkey picker · overlay/vad controls.

### Phase 3 — Polish
Silero VAD · DirectML EP option · autostart · signed updater · history (SQLite) · i18n/RTL · Linux (X11/Wayland) + macOS ports · CLI flags/Unix signals · top/bottom toggle + scroll-back + themes in overlay.

---

## 15. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `transcribe-rs` GigaAM loader is single-file CTC, not 3-file RNN-T | **Resolved** | High | **Confirmed 2026-08-02 from source** (`gigaam/mod.rs`): loader is CTC-only, single-file. Mitigation applied: Phase-1 primary switched to `v3_e2e_ctc` (native, D4). `e2e_rnnt` retained as Task 0-B fallback (port `parakeet/mod.rs` RNN-T loop to 3 files) only if `e2e_ctc` WER ≥ 0.15. |
| Overlay non-focusable invariant fails on some WebView2 build → paste misroutes | Med | High | `focusable:false` + `GetForegroundWindow()==target` check before `Ctrl+V`; on mismatch, clipboard + toast (never mispaste). Verified against Handy. |
| cpal default rate not 16 kHz → need rubato (already planned) | High | Low | rubato in place; no-op fast path if device is already 16 kHz. |
| Inference RTF > 1 on target CPU → "streaming" lags reality | Med | Med | int8 default; RTF guard in worker switches to batch-on-release + `processing` overlay if worker falls behind (§6.4). Measure RTF in first Phase-1 milestone; gate streaming UX on RTF < ~0.7. |
| Cross-chunk over-punctuation degrades paste quality (GigaAM-specific) | Med-High | Med | Primary knob: raise `min_chunk_secs`. Optional: light inter-chunk punctuation post-pass. Measure on real Russian speech at kickoff. |
| `ort` 2.0 RC rough edges | Low | Med | Driven transitively by `transcribe-rs`; do not pin; follow `transcribe-rs` upgrades. |
| Hotkey conflicts with other apps | Med | Low | Configurable binding; clear conflict error; secondary default. |
| Hotkey release-event reliability (PTT key-up) on Windows | Low | Med | Adopt Handy's proven `global-shortcut`+`handy-keys` path (reuse over reinvent). Integration test for PTT press/release on Windows in Task 3. Switch to raw `global-hotkey` 0.8 only if a concrete failure appears. |
| Mic permission blocked by Windows privacy | Low | Med | Detect cpal failure; surface actionable message + link to settings. |
| Transcript content leaks into file logs (privacy regression) | Med | High | Logging discipline (§10.1): never log transcript/audio; enforced by an assertion test. |
| First launch offline = app cannot function until model downloaded | Med | Low | Clear offline-first-launch messaging; model cached permanently after; Phase-3 offline installer (bundle) for air-gapped use. |
| GigaAM-v3 license change | Low | High | MIT today; record license file in `models/`; pin to a specific revision. |

---

## 16. Open Items for Phase-1 Kickoff (to verify, not blockers)

1. ~~**`transcribe-rs` 0.3.11 RNN-T support**~~ **— RESOLVED 2026-08-02 (source-verified):** `GigaAMModel::load` (`gigaam/mod.rs`) is CTC-only, single-file; it does **not** load 3-file `e2e_rnnt`. Decision: Phase-1 primary is now `v3_e2e_ctc` (native). The `e2e_rnnt` 3-file path (port `parakeet/mod.rs::decode_sequence` RNN-T loop) becomes Task 0-B, triggered only if `e2e_ctc` WER ≥ 0.15. Remaining sub-question for Task 0-B (if reached): exact feature pipeline from `v3_e2e_rnnt.yaml` (whether LFR/CMVN stages apply).
2. **Tauri 2.11 `transparent:true` + `focusable:false` overlay** on Windows WebView2 — confirm it renders and that the paste target retains focus throughout (the core paste invariant, §6.6/§9).
3. **cpal native 16 kHz capture** on the dev machine — if available, rubato becomes a no-op fast path; if not, confirm 48 kHz capture + rubato pipeline.
4. **PTT key-up reliability** — integration-test that `global-hotkey` 0.8 fires release on Windows for the chosen binding (the PTT-defining event); confirm Win32 message-pump threading.
5. **RTF measurement** — measure GigaAM e2e RTF on the target CPU early; gate the streaming overlay on RTF < ~0.7, else ship batch-on-release first.
6. **Cross-chunk punctuation tuning** — on real Russian speech, sweep `min_chunk_secs` and decide whether the optional inter-chunk punctuation post-pass is needed for clean paste output.
7. **GigaAM-v3 e2e-rnnt int8 download size** — for the first-run progress UI; files confirmed present in `istupakov/gigaam-v3-onnx`.

---

## 17. References

- **Handy** — https://github.com/cjpais/Handy (Tauri 2 PTT, transcribe-rs, coordinator pattern, overlay)
- **transcribe-rs** — https://github.com/cjpais/transcribe-rs (SpeechModel, VadChunked, GigaAM/Parakeet loaders)
- **parakeet-rs** — https://github.com/altunenes/parakeet-rs (Nemotron streaming, Phase 2 candidate)
- **GigaAM** — https://github.com/salute-developers/GigaAM (MIT, model family)
- **GigaAM-v3 ONNX** — https://huggingface.co/istupakov/gigaam-v3-onnx
- **SilentKeys** — https://github.com/gptguy/silentkeys (Rust PTT on Parakeet)
- **Whispering** — open-source local-first dictation
- **Wispr Flow / Superwhisper** — commercial dictation UX references
- **whisper-overlay** (crate) — Wayland PTT overlay reference
- **ort** — https://github.com/pykeio/ort (ONNX Runtime Rust)
- **global-hotkey** — https://github.com/tauri-apps/global-hotkey

---

## 18. Glossary

- **ASR** — Automatic Speech Recognition.
- **RNN-T** — Recurrent Neural Network Transducer; streaming-friendly end-to-end ASR architecture (encoder + predictor + joiner).
- **CTC** — Connectionist Temporal Classification; frame-level ASR decoding, typically offline.
- **e2e (end-to-end)** — model fine-tuned to emit final-style output directly (here: with punctuation + normalization).
- **VAD** — Voice Activity Detection; separates speech from silence to chunk audio.
- **CMVN** — Cepstral Mean-Variance Normalization; feature-space normalization.
- **LFR** — Low Frame Rate; feature downsampling to reduce compute.
- **EP (Execution Provider)** — ONNX Runtime backend (CPU, CUDA, DirectML).
- **TTFP** — Time To First Partial; latency from speech start to first streaming partial.
- **PTT** — Push-to-Talk.
- **ONNX** — open neural-network exchange format; the model packaging we run on CPU.
- **WASAPI** — Windows Audio Session API; cpal's audio backend on Windows.
