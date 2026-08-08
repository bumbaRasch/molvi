# Contributing to molvi

Thanks for using **molvi** — a local, private, push-to-talk dictation app (Tauri 2 + CPU-only ASR). This is a short guide for filing issues and opening pull requests.

## Before filing an issue

1. **Search** [existing issues](https://github.com/bumbaRasch/molvi/issues) for duplicates first.
2. Pick the matching template:
   - **Bug** — something is broken or behaves wrong.
   - **Feature** — you want molvi to do something new.
   - **ASR** — a speech-recognition / transcription-quality or model-loading problem.
   - **Performance** — a slowdown, high latency, or real-time regression.
3. Give a **minimal repro**: the exact steps, the molvi version, your OS, and the engine you were using.

## Privacy (HARD, spec §10.1)

**NEVER paste** transcripts, audio, dictionary entries, snippets, command phrases, `settings.json`, or history rows into issues or PRs. molvi never logs these and they are private — describe problems with **synthetic / non-private examples** instead.

molvi's **logs ARE safe to paste**: they are metadata-only and `%APPDATA%` is redacted. Engine/language codes and the foreground-app basename count as metadata and may appear in logs.

## Building & running

Full detail lives in [`AGENTS.md`](./AGENTS.md). The essentials:

```bash
cargo tauri dev      # run the app (debug GUI)
cargo tauri build    # NSIS / MSI / dmg / deb / AppImage
```

Gates every PR must pass:

```bash
cargo fmt   --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml --lib
npx tsc --noEmit
npm run build
```

## Labels

| Label | Meaning |
| --- | --- |
| `bug` | Something is broken or behaves incorrectly. |
| `enhancement` | A new feature or improvement to existing behavior. |
| `asr` | Speech-recognition quality, model loading, or engine behavior. |
| `performance` | A slowdown, latency, or real-time-factor regression (the "blaze"). |
| `triage` | Not yet reviewed by the maintainer. |

## Pull requests

- Follow the **PR template checklist** (it appears when you open a PR).
- Do **not** touch the load-bearing items listed under "DO-NOT-TOUCH" in [`AGENTS.md`](./AGENTS.md) — e.g. the `transcribe-rs` `[patch.crates-io]` override, the 8960-sample Nemotron chunk boundary, the privacy log substrate, the per-platform paste / `letter_key` keys, and the release `[profile.release]`. They are load-bearing for the build or the latency target.
- No new dependency without justification. Dependabot is configured to ignore the ASR pins (`transcribe-rs`, `parakeet-rs`, `ort`) — they carry an ort-version landmine, see `AGENTS.md`.

## Scope

molvi is **solo-maintained**. Issues and PRs are welcome, but response times vary — please be patient. Searching first and writing a clear, minimal repro is the most helpful thing you can do.
