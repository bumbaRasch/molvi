# molvi platform portability (future)

> Status: **documented option, not a build plan.** Records the engine choice for
> a possible multi-platform future so the decision isn't re-researched later.

## Vision

molvi today ships **Windows 11 x64 only** (a Tauri 2 webview shell with local
CPU ASR — GigaAM / Nemotron on ort/ONNX). The long-term direction is a
**multi-platform family of builds**, so the user picks the build that fits the
device:

- **Windows** (today) — full desktop UI, GigaAM/Nemotron, push-to-talk.
- **Linux** — two variants:
  - **With UI** — the Tauri desktop shell + full Settings panel, like Windows
    (Ubuntu, Fedora, Debian, Arch, …).
  - **Headless / no-UI** — a dictation daemon/service with no window, driven by
    hotkey or IPC (same distros; also the path for servers + Raspberry Pi).
- **macOS** — full desktop UI (Intel + Apple Silicon).
- **Raspberry Pi** — embedded / always-on (typically the headless Linux build).
- **Mobile** — Android / iOS.

This note captures the engine-level enabler for the low-resource / non-Windows
targets: **Vosk**.

## Which build for which device

molvi's engine picker is **filtered by device capability, not fixed per OS**.
The principle:

- **Capable targets** (desktop, server-class Linux) show **every** engine — the
  user chooses. The small Vosk model is offered *there too*: a Windows user who
  wants maximum speed or a small memory footprint can pick Vosk-small over the
  heavier GigaAM / Nemotron.
- **Constrained targets** (phones, Raspberry Pi, weak headless boxes) **hide**
  engines they can't run (the multi-hundred-MB / multi-GB ort models) and show
  only what fits — Vosk. A 2.6 GB model isn't a *real* choice there: it OOMs,
  cellular-downloads for minutes, and thermal-throttles to multi-second latency
  — showing it would be a trap, not freedom. (This is also why every shipped
  mobile ASR app caps at small/tiny models.)

Planned per-target offering (only Windows ships today):

| Target | Engines shown | Default | Rationale |
|---|---|---|---|
| Windows / macOS / Linux desktop | **GigaAM + Nemotron + Vosk** | GigaAM | full power → user chooses; Vosk-small offered for max speed / low RAM |
| Linux headless, server-class | **GigaAM + Nemotron + Vosk** | GigaAM / Nemotron | powerful Linux servers run the big ort engines fine |
| Linux headless, weak / Pi-class | **Vosk only** | Vosk (small) | can't run ort / 2.6 GB; big models hidden |
| Raspberry Pi | **Vosk only** | Vosk (small ~50 MB) | real-time on Pi 3/4 — **best for Pi / embedded** |
| Android / iOS | **Vosk only** | Vosk (small ~50 MB) | battery/RAM-friendly, native streaming — **best for phones** |

So "big vs small" is a **user choice on capable hardware** and a **forced
constraint on weak hardware** — not a fixed mapping per platform.

## Vosk

[Vosk](https://alphacephei.com/vosk/) is an **offline (on-device) speech
recognition toolkit**, Kaldi-based, by Alpha Cephei. Apache-2.0.

Key properties that matter for molvi:

- **Natively streaming** — `Recognizer::accept_waveform(samples)` returns
  *partial* results during speech. This maps **1:1** onto molvi's existing
  `SpeechEngine::feed_chunk(samples, on_partial)` contract — the live growing
  caption works with no chunked-offline hack (unlike Whisper, which is a
  one-shot whole-utterance decode).
- **Model tiers** — a **small model is ~50 MB** (~300 MB runtime RAM) and runs
  **real-time on a Raspberry Pi 3/4 and on smartphones**; larger models exist
  for server-side high-accuracy. ([alphacephei.com/vosk/models](https://alphacephei.com/vosk/models))
- **Cross-platform** — Linux, Windows, macOS, Android, iOS, Raspberry Pi;
  "scales from small devices like Raspberry Pi or Android smartphones to big
  clusters." ([github.com/alphacep/vosk-api](https://github.com/alphacep/vosk-api))
- **20+ languages incl. Russian** (`vosk-model-ru`, `vosk-model-small-ru`) —
  covers molvi's default audience.
- **Local + private** (nothing leaves the device) — matches molvi's privacy
  posture (spec §10.1).
- **Rust bindings** — `vosk` (FFI, [docs.rs/vosk](https://docs.rs/vosk)) and
  `vosk-rust` (pure Rust).

## Why Vosk is the portability engine

molvi's current engines (GigaAM via `transcribe-rs`, Nemotron via `parakeet-rs`)
both run on **ort (ONNX Runtime)**, target **~214 MB – 2.6 GB** models, and the
build is **Windows-x64-focused**. That stack is too heavy for Raspberry Pi or
phone-class targets, and multi-GB models are impractical on mobile.

Vosk's **~50 MB small models + native streaming + ARM/mobile support** make it
the natural engine for the low-resource / non-Windows targets in the vision
above. Whisper, by contrast, is the *wrong* third engine for molvi: offline
one-shot decode (~5–7 s latency reported on-device), ~1.5 GB for the accurate
variant, and it duplicates Nemotron's multilingual coverage while regressing
the blaze (≤0.03 RTF) default path. See the multi-engine research notes.

## The adapter already exists

molvi **already has the engine abstraction** — the `SpeechEngine` trait
(`feed_chunk` / `finish` / `had_speech`) and the `load_engine` factory that
dispatches on `settings.model` (`src-tauri/src/engine_adapter.rs`). Adding Vosk
is **~one new `SpeechEngine` impl** over the `vosk` Rust binding + one branch in
`load_engine` / `model_store::source` + a UI row — not a cross-cutting rewrite.

The portability work *beyond the engine* is the larger, separate effort: Tauri
webview-shell → headless Linux daemon, mobile audio capture / clipboard / hotkey
(which differ from Win32), and a UI strategy per target.

## Decision recorded

- **molvi's curated 2-engine set (GigaAM default RU, Nemotron multilingual)
  covers the Windows mission.** No third engine is needed today.
- **When (not if-ever) a Linux / RPi / mobile target is greenlit, the engine to
  reach for is Vosk** — for its streaming contract, small models, and
  Pi/mobile/ARM reach.
- The broader ecosystem has independently converged on **curated 2–3 engine
  lists** (MacWhisper, WhisperNotes, Talon, sherpa-onnx) rather than open plugin
  systems; molvi stays curated. A universal runtime-plugin adapter is **not**
  the goal — see the multi-engine research.

## Sources

- [alphacephei.com/vosk/models](https://alphacephei.com/vosk/models) — model
  tiers + sizes (small ~50 MB / ~300 MB RAM; big for server)
- [github.com/alphacep/vosk-api](https://github.com/alphacep/vosk-api) — platform
  support (Android, iOS, Raspberry Pi, Linux, Windows, macOS)
- [docs.rs/vosk](https://docs.rs/vosk) — Rust FFI binding
- Multi-engine adapter research (internal) — competitor analysis + the
  streaming-contract problem + the Vosk-vs-Whisper recommendation
