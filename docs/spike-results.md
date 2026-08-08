# Spike results — multi-platform port (2026-08-07)

Phase-1 CI matrix = the de-risking mechanism (plan Task 3). Green CI on the
non-Windows runners IS the spike pass. CI run (all green):
https://github.com/bumbaRasch/molvi/actions/runs/31178568587 (PR #1,
branch `multiplatform-port`).

## Spike #1 — Linux engine build (ubuntu-latest, x86_64-unknown-linux-gnu): PASS

`cargo clippy --all-targets -D warnings` + `cargo test --lib` green.
- ort 2.0.0-rc.13 (CPU EP) resolves + links on `x86_64-unknown-linux-gnu`
  (ort-sys fetches the prebuilt libonnxruntime; no local onnxruntime build).
- transcribe-rs 0.3.11 + parakeet-rs 0.3.7 compile under the `[patch]`
  ort-pin override (confirmed load-bearing — clean resolve on all 3 OSes).
- cpal 0.18 ALSA backend builds against `libasound2-dev` (mandatory on Linux).
- Tauri 2 Linux sys-crates build against the standard apt set
  (`libwebkit2gtk-4.1-dev` + the Tauri 2 prerequisite list).
- 180 model-free lib tests pass (the 4 Windows-only `paths` tests are gated to
  Windows for Phase 1; Task 5/Phase 2 makes `paths.rs` cross-platform).

## Spike #2 — macOS engine build (macos-14, aarch64-apple-darwin): PASS

`cargo clippy --all-targets -D warnings` + `cargo test --lib` green.
- ort 2.0.0-rc.13 (CPU EP) resolves + links on `aarch64-apple-darwin`.
  (Intel `x86_64-apple-darwin` remains unsupported by design — no ort-sys
  dist.tsv row; load-dynamic is a post-v1 follow-up.)
- transcribe-rs + parakeet-rs compile.
- cpal 0.18 CoreAudio backend (no system deps). enigo CGEvent, arboard AppKit
  — all build without extra system deps on the macos-14 runner.
- 180 lib tests pass (same paths-test gating as Linux).
- **CoreML EP feature wiring is Phase 2** (Task 4 adds the `ort/coreml`
  features for both engines; Task 10 measures whether CoreML accepts the graphs
  and whether it preserves/improves the ≤0.03 RTF moat). This Phase-1 job
  verified the ort-CPU baseline resolves + links on Apple Silicon.

## Build quirks found

- **`paths.rs` is Windows-only** (`app_data_dir()` reads `%APPDATA%`). The plan
  defers the cross-platform impl to Task 5 (Phase 2). For Phase 1, the 4
  `paths` tests are gated to `#[cfg(all(test, target_os = "windows"))]`
  (mirrors the `has_disk_space_is_sane` Windows-gate from Step 0). Task 5
  restores cross-platform paths + cross-platform test assertions.
- **ort-pin `[patch]` override is load-bearing** (Cargo.toml `[patch.crates-io]`
  transcribe-rs git rev `efc66111…`). A clean `Cargo.lock` re-resolution on all
  three OSes relied on it. Do NOT remove.
- **cpal on Linux needs `libasound2-dev`** even to build (ALSA is the default
  backend). CI installs it.
- **Tauri 2 on Linux needs the full webkit2gtk-4.1 apt set** even for
  `cargo check`/`clippy` (sys-crate build scripts probe pkg-config at compile
  time). CI installs it.

## Deferred to Phase 2

- Per-platform **RTF measurement** (needs the real GigaAM/Nemotron model
  on-device on a Mac). The Phase-1 jobs verify compile+link only.
- **CoreML EP** acceptance (does it load the GigaAM/Nemotron graphs; is it
  faster than ort-CPU). Task 4 wires the feature; Task 10 measures.
