# molvi multi-platform port — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn molvi (Windows 11-only) into a free, local, private, fast dictation
family across the desktop OSes — Windows (done) → macOS (Apple Silicon) → Linux —
porting the real GigaAM/Nemotron engines (not a rewrite), under a dual
`MIT OR Apache-2.0` license.

**Architecture:** Inline `#[cfg(target_os = "...")]` per feature module +
`[target.'cfg(windows)'.dependencies]` Cargo.toml gate (decision D2). **No
`mod platform` abstraction** — molvi's ~6 platform functions are each used in
exactly one feature module, so a central trait is cargo-cult. Platform dispatch
is compile-time only → zero runtime branches in the hot loop (blaze invariant).
Sequence (D3): cross-platform-build unblock → macOS-first → Linux (Wayland
scoping OPEN).

**Tech Stack:** Tauri 2.11, Rust stable (1.97.1), ort 2.0.0-rc.13, transcribe-rs
0.3.11 (GigaAM), parakeet-rs 0.3.7 (Nemotron), enigo 0.6.1, cpal 0.18.1,
arboard 3.6.1, Vite 8 + TypeScript 7. macOS adds `tauri-nspanel` (overlay
focus fix). Linux uses x11rb (transitive via enigo).

## Global Constraints

Copied verbatim from the design spec — every task's requirements implicitly
include these.

- **License:** dual `MIT OR Apache-2.0` (D1). `Cargo.toml` `license` field +
  root `LICENSE-MIT` + `LICENSE-APACHE` files + `package.json` `license`.
- **Architecture D2 (HARD):** inline `#[cfg(target_os = "...")]` per feature
  module; **NO `mod platform`, NO trait objects, NO dyn dispatch.** Platform is
  known at compile time.
- **Blaze (HARD — performance, not compat):** the default RU/PTT/Smart path may
  be refactored but its invariant holds on every shipping target: **RTF ≤ 0.03**
  (controlled long-utterance, measured), hot loop (capture→engine→finalize→paste)
  **allocation/lock/blocking-free**, **no runtime platform branches** (all
  `#[cfg]`, zero cost). **Nemotron feeds ONLY at the 8960-sample boundary — do
  NOT change.**
- **Privacy §10.1 (HARD RULE):** never log transcript/partials/post-proc/dict/
  history/snippet/command/prompt text at any level. The 6 `log_privacy`
  substrates stay green. Per-platform: `redact_appdata` must also redact `$HOME`
  on Unix (PII-adjacent), not just `%APPDATA%`.
- **ort-pin landmine (pre-verified, load-bearing):** `transcribe-rs 0.3.11`
  exact-pins `ort = "=2.0.0-rc.12"`; `parakeet-rs 0.3.7` needs rc.13. molvi's
  `[patch.crates-io]` (Cargo.toml:95-96, git rev `efc66111…`) relaxes the pin.
  **This override EXISTS and is confirmed — do not remove it.** A fresh
  `Cargo.lock` re-resolution relies on it.
- **Apple-Silicon only for macOS v1:** ort-sys dist.tsv has NO
  `x86_64-apple-darwin` row → Intel Mac build fails by default. Intel =
  best-effort `load-dynamic` follow-up, NOT v1. Gate macOS CoreML deps with
  `cfg(all(target_os = "macos", target_arch = "aarch64"))`.
- **Verify crates live, never from memory:** use find-docs skill (ctx7) +
  docs.rs/crates.io before coding. Live ctx7 IDs: `/pykeio/ort` (NOT
  `/pyke.io/ort`), `/enigo-rs/enigo`, `/ahkohd/tauri-nspanel`,
  `/websites/v2_tauri_app`, `/cjpais/transcribe-rs`, `/altunenes/parakeet-rs`.
  For transcribe-rs/parakeet-rs, ctx7 autodocs are unreliable — cross-check the
  resolved source in `~/.cargo/registry/src/index.crates.io-*/`.
- **Gates for every code task:** `cargo fmt` + `cargo clippy --manifest-path
  src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test
  --manifest-path src-tauri/Cargo.toml --lib` + (`cargo test --test
  log_privacy` if no `molvi.exe` is locked) + `npx tsc --noEmit` + `npm run
  build`. **Do NOT kill a running `cargo tauri dev`** — use `cargo check
  --all-targets` + `cargo test --lib` if the dev app holds the binary.

---

## File Structure

Boundary files only (the ASR brain — `engine.rs`, `engine_adapter.rs`,
`postproc.rs`, `commands.rs`, `coordinator.rs`, `pipeline.rs`, `dictionary.rs`,
`snippets.rs`, `history.rs`, `resample.rs`, `log.rs`, `tray*.rs`, `settings.rs`
— is platform-neutral and unchanged by this port unless a task names it).

| File | Responsibility | Phase |
|---|---|---|
| `LICENSE-MIT`, `LICENSE-APACHE` (NEW) | dual license text | 1 |
| `src-tauri/Cargo.toml` | move `windows` → target-dep; macOS/Linux target-deps; `license` field | 1, 2, 3 |
| `package.json` | `license` field | 1 |
| `src-tauri/src/audio.rs` | `play_sound_file` cfg-gate (Win32 → no-op stub) | 1 |
| `src-tauri/src/ort_affinity.rs` | `p_core_mask`/`apply_for_engine` cfg-gate (Win32 → no-op stubs) | 1 |
| `src-tauri/src/profiles.rs` | `foreground_exe` cfg dispatch (Win32 / macOS NSWorkspace / X11 / None) | 1, 2, 3 |
| `src-tauri/src/paste.rs` | `capture_target`/`ensure_focus` cfg; `paste_key()`/`paste_modifier()` helpers; re-key all chord sites | 1, 2, 3 |
| `src-tauri/src/model_store.rs` | non-Windows `has_disk_space` stub; (P2/3 `statfs`/`statvfs`) | 1 |
| `src-tauri/src/paths.rs` | `app_data_dir()` cfg dispatch; `redact_appdata` redacts `$HOME` | 2, 3 |
| `src-tauri/src/overlay.rs` | macOS NSPanel conversion branch (`tauri-nspanel`) | 2 |
| `src-tauri/src/commands.rs` | per-platform chord key tables | 2 |
| `src-tauri/src/lib.rs` | (setup: macOS permission prompts, NSPanel init) | 2 |
| `src-tauri/tauri.conf.json` | macOS bundle settings | 2 |
| `.github/workflows/ci.yml` (NEW) | windows/macos-14/ubuntu matrix = spikes #1/#2 | 1 |
| `AGENTS.md` | record per-platform NFRs + API corrections after measurement | 2, 3 |

---

# Phase 1 — Unblock cross-platform builds

**Purpose:** make `cargo build` succeed on Windows/macOS/Linux (it currently
fails: `windows` crate is an unconditional dep + 5 unconditional Win32 import
sites). This phase ALSO delivers the license and the CI matrix that doubles as
spikes #1 (Linux engine build) and #2 (macOS engine build). Phase 1 does NOT
implement any real macOS/Linux behavior — non-Windows bodies are fail-open
stubs (`None`/`Ok(true)`/no-op) that Phase 2/3 replace.

**Phase 1 done = all three CI jobs green** (cargo fmt + clippy + test --lib +
tsc + build on windows/macos-14/ubuntu).

---

### Task 1: Add the dual license (MIT OR Apache-2.0)

molvi is currently unlicensed → this is the first deliverable (D1).

**Files:**
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`
- Modify: `src-tauri/Cargo.toml` (package section)
- Modify: `package.json`

- [ ] **Step 1: Create `LICENSE-MIT`**

Write the canonical MIT License text, copyright line:
`Copyright (c) 2026 molvi contributors`. (Standard MIT text from
choosealicense.com/licenses/mit/ — the 0BSD-style "permission is hereby granted
… free of charge" paragraph + the no-warranty paragraph.)

- [ ] **Step 2: Create `LICENSE-APACHE`**

Write the canonical Apache License 2.0 text (full text from
apache.org/licenses/LICENSE-2.0.txt). It is long; copy it verbatim. Append the
`NOTICE` boilerplate note only if a `NOTICE` file is added (none here).

- [ ] **Step 3: Set the `license` field in `src-tauri/Cargo.toml`**

In the `[package]` section (after `version = "0.1.0"`), add:

```toml
license = "MIT OR Apache-2.0"
```

SPDX expression; crates.io renders both. (Add `repository`, `edition` is
already `2024`.)

- [ ] **Step 4: Set the `license` field in `package.json`**

Add `"license": "MIT OR Apache-2.0"` alongside `"private": true`.

- [ ] **Step 5: Verify the build is unchanged**

Run: `cargo build --manifest-path src-tauri/Cargo.toml` (or `cargo check
--all-targets --manifest-path src-tauri/Cargo.toml` if the dev app holds the
binary).
Expected: PASS (no code changed; `license` is metadata).

- [ ] **Step 6: Commit**

```bash
git add LICENSE-MIT LICENSE-APACHE src-tauri/Cargo.toml package.json
git commit -m "license: add dual MIT OR Apache-2.0 (D1)"
```

---

### Task 2: Step 0 — cfg-gate the 5 Win32 sites + move the `windows` dep

This is the single gate "compiles on Windows/macOS/Linux." All 5 sites land in
one task because they share one test cycle (`cargo check --all-targets` on all
3 OSes via the CI matrix in Task 3). Verified line numbers are current as of
this plan; re-confirm with the editor before editing.

**The 5 sites (verified by reading the source):**
1. `src-tauri/src/audio.rs:6-7` — unconditional `use windows::Win32::Media::Audio…` + `play_sound_file` (288-299).
2. `src-tauri/src/ort_affinity.rs:10-14` — unconditional `use windows::…` + `p_core_mask`/`apply_for_engine` (Win32 bodies).
3. `src-tauri/src/profiles.rs:13-18` — unconditional `use windows::…` + `foreground_exe` (29-82, Win32 body).
4. `src-tauri/src/paste.rs:9-10` — unconditional `use windows::…` + `capture_target`/`foreground_is`/`ensure_focus`.
5. `src-tauri/src/model_store.rs:214` — `has_disk_space` is ALREADY `#[cfg(target_os="windows")]`, BUT it is called **unconditionally** at `ipc.rs:584` → non-Windows won't link. Needs a non-Windows stub. AND the `has_disk_space_is_sane` test (355-369) asserts `!has_disk_space(u64::MAX)` → fails under the `Ok(true)` stub → must be gated.

Plus the Cargo.toml dep move (prerequisite — without it the `windows` crate
itself fails to resolve on non-Windows, and the gated `use windows` inside the
cfg arms won't compile).

**Interfaces:**
- Produces (unchanged signatures, cross-platform-compiling): `audio::play_sound_file(&str)`, `ort_affinity::p_core_mask() -> Option<usize>`, `ort_affinity::apply_for_engine(bool)`, `profiles::foreground_exe() -> Result<String>`, `paste::capture_target() -> Option<isize>`, `paste::ensure_focus(isize) -> Result<()>` (private), `model_store::has_disk_space(u64) -> Result<bool>`. Phase 2/3 consume these with real per-OS bodies.

- [ ] **Step 1: Move `windows` to a Windows-only target dependency**

In `src-tauri/Cargo.toml`, REMOVE line 49 (the unconditional `windows = { … }`
under `[dependencies]`), and ADD a target-specific section at the end of the
file (after `[patch.crates-io]`):

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = ["Win32_UI_WindowsAndMessaging", "Win32_Foundation", "Win32_System_Threading", "Win32_System_SystemInformation", "Win32_Media_Audio", "Win32_Storage_FileSystem"] }
```

(Same version + features as before — just gated. `Win32_Storage_FileSystem`
stays for `has_disk_space`; `Win32_Media_Audio` for `play_sound_file`; the
`Threading`/`SystemInformation`/`WindowsAndMessaging`/`Foundation` for the
other sites.)

- [ ] **Step 2: cfg-gate `audio.rs`**

Remove the top-level imports at lines 6-7:
```rust
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME};
use windows::core::PCWSTR;
```
Replace the single `pub fn play_sound_file(path: &str) { … }` (288-299) with
two cfg-gated definitions:

```rust
#[cfg(target_os = "windows")]
pub fn play_sound_file(path: &str) {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME};
    use windows::core::PCWSTR;
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0u16)).collect();
    let flags = SND_FILENAME | SND_ASYNC;
    // SAFETY: pszsound is a valid PCWSTR to our owned null-terminated wide
    // string that outlives the call; hmod=None; flags = filename + async.
    let ok = unsafe { PlaySoundW(PCWSTR::from_raw(wide.as_ptr()), None, flags) };
    if !ok.as_bool() {
        tracing::warn!("sound file playback failed");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn play_sound_file(_path: &str) {
    // ponytail: no portable wav player yet (Phase 2 CoreAudio / Phase 3 ALSA
    // playback); best-effort no-op. `play_tone` (cpal) is the cross-platform
    // default, so only the custom-.wav path is silent off-Windows.
}
```

- [ ] **Step 3: cfg-gate `ort_affinity.rs`**

Move the two `use windows::…` (lines 10-14) inside the windows function bodies
(see below), then split each public fn into a cfg pair. Keep the doc comments.
The `p_core_mask` body stays intact under `#[cfg(target_os = "windows")]`; add
stubs:

```rust
#[cfg(target_os = "windows")]
pub fn p_core_mask() -> Option<usize> {
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessAffinityMask};
    // … existing Win32 body unchanged …
}

#[cfg(not(target_os = "windows"))]
pub fn p_core_mask() -> Option<usize> {
    None
}
```

```rust
#[cfg(target_os = "windows")]
pub fn apply_for_engine(is_nemotron: bool) {
    // … existing Win32 body unchanged …
}

#[cfg(not(target_os = "windows"))]
pub fn apply_for_engine(is_nemotron: bool) {
    // ponytail: Apple Silicon P/E is scheduler-managed (no pinning); Linux
    // sched_setaffinity is an optional Phase-3 follow-up. Logging mirrors the
    // Windows path's info line so traces stay comparable across platforms.
    if is_nemotron {
        tracing::info!("process affinity: all-cores (nemotron — no pinning)");
    } else {
        tracing::info!("process affinity: not managed on this OS (fail-open)");
    }
}
```

The `#[cfg(test)] mod tests` test `p_core_mask_is_some_or_none_gracefully` calls
`p_core_mask()` — compiles on all OSes now (returns `None` off-Windows, so the
`if let Some(m)` arm is skipped). No change needed there.

- [ ] **Step 4: cfg-gate `profiles.rs` — `foreground_exe`**

Remove the top-level `use windows::…` (lines 13-18). Wrap the body of
`foreground_exe` in cfg arms (keep the signature + doc comment). The existing
Win32 body becomes the `#[cfg(target_os = "windows")]` arm; add a non-Windows
arm that fail-opens (Phase 2 adds macOS, Phase 3 adds X11):

```rust
pub fn foreground_exe() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        use windows::core::PWSTR;
        // … existing Win32 body (the HWND→PID→basename chain) unchanged …
        # Ok(base)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // ponytail: macOS (NSWorkspace) lands in Phase 2; Linux X11
        // (_NET_WM_PID) in Phase 3. Fail-open: caller treats Err as "no profile
        // match, use global settings" (pipeline.rs:122).
        Err(MolviError::Profile(
            "foreground_exe not implemented on this OS yet".into(),
        ))
    }
}
```

(The `# Ok(base)` line is illustrative — keep the real existing tail of the
Win32 body.) `resolve`/`apply_profile_override` are pure (no Win32) — untouched.
The test `foreground_exe_smoke` calls `foreground_exe()` — still compiles
(`if let Ok` skips the Err off-Windows).

- [ ] **Step 5: cfg-gate `paste.rs`**

Remove the top-level `use windows::…` (lines 9-10). Gate the three helpers and
add a non-Windows `ensure_focus` stub (it is called by `paste_text` and
`run_command_chord` on all platforms and must exist for linking):

```rust
#[cfg(target_os = "windows")]
pub fn capture_target() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    let h = hwnd.0 as isize;
    if h == 0 { None } else { Some(h) }
}

#[cfg(not(target_os = "windows"))]
pub fn capture_target() -> Option<isize> {
    // ponytail: macOS (pid_t) Phase 2; Linux (_NET_ACTIVE_WINDOW) Phase 3.
    // None → paste_text/run_command_chord error at the target guard before any
    // paste attempt (safe; text never misdelivered).
    None
}

#[cfg(target_os = "windows")]
fn foreground_is(target: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let fg = unsafe { GetForegroundWindow() };
    (fg.0 as isize) == target
}

#[cfg(target_os = "windows")]
fn ensure_focus(target: isize) -> Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};
    if foreground_is(target) {
        return Ok(());
    }
    tracing::warn!("paste: foreground mismatch, attempting SetForegroundWindow");
    unsafe {
        let _ = SetForegroundWindow(HWND(target as *mut _));
    }
    thread::sleep(Duration::from_millis(40));
    if foreground_is(target) {
        Ok(())
    } else {
        tracing::warn!("paste: could not restore focus; left on clipboard");
        Err(MolviError::Paste("focus mismatch; text left on clipboard".into()))
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_focus(_target: isize) -> Result<()> {
    // ponytail: real focus-guard (verify-only macOS / restore X11) in Phase 2/3.
    Ok(())
}
```

- [ ] **Step 6: Complete the `model_store.rs` gate — non-Windows stub + gate the test**

The function at line 214 is already `#[cfg(target_os = "windows")]`. ADD a
non-Windows stub immediately after it (before `ensure_model`):

```rust
/// Fail-open stub for non-Windows (Step 0). Phase 2 adds `statfs` (macOS),
/// Phase 3 `statvfs` (Linux); until then assume enough space (the download
/// itself fails cleanly on ENOSPC via hf-hub's io error). Privacy §10.1: a
/// byte count, no content.
#[cfg(not(target_os = "windows"))]
pub fn has_disk_space(_needed: u64) -> Result<bool> {
    Ok(true)
}
```

Then gate the test that would break under the stub. Change line 355 from:

```rust
    #[test]
    fn has_disk_space_is_sane() {
```

to:

```rust
    #[cfg(target_os = "windows")]
    #[test]
    fn has_disk_space_is_sane() {
```

(The test asserts `!has_disk_space(u64::MAX)` — meaningless + false under the
`Ok(true)` stub. It tests real Win32 `GetDiskFreeSpaceExW` behavior, so it stays
Windows-only. The Phase 2/3 `statfs`/`statvfs` impls get their own tests.)

- [ ] **Step 7: Verify the build on Windows**

Run (use `check` if the dev app holds the binary):
```
cargo check --all-targets --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
```
Expected: PASS — Windows behavior is byte-identical (the cfg arms select the
same code). `cargo test --lib` count unchanged (the gated test still runs on
Windows).

- [ ] **Step 8: Verify the frontend gate is untouched**

Run: `npx tsc --noEmit && npm run build`
Expected: PASS (no frontend change).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/audio.rs src-tauri/src/ort_affinity.rs \
        src-tauri/src/profiles.rs src-tauri/src/paste.rs src-tauri/src/model_store.rs
git commit -m "build: cfg-gate Win32 sites for cross-platform compile (Step 0, D2)"
```

> NOTE: You CANNOT verify the macOS/Linux build on this Windows box — that is
> exactly what Task 3's CI matrix proves. Step 0 is "done" when all three CI
> jobs go green.

---

### Task 3: CI matrix (windows / macos-14 / ubuntu) = spikes #1 + #2

The CI **IS** the de-risking mechanism: a green `macos-14` (Apple Silicon) job =
spike #2 passes (engines build on aarch64-apple-darwin; note: CoreML EP feature
wiring is Phase 2 — this Phase-1 job verifies the ort-CPU build resolves +
links); a green `ubuntu` job = spike #1 (Linux ort-CPU build).

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: CI

on:
  push:
    branches: [main, phase3]
  pull_request:

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: windows-latest
          - os: macos-14        # Apple Silicon (aarch64-apple-darwin)
          - os: ubuntu-latest   # x86_64-unknown-linux-gnu
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Rust cache
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: '24'

      - name: Install JS deps
        run: npm ci

      - name: Rust fmt
        run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check

      - name: Rust clippy
        run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

      - name: Rust unit tests
        run: cargo test --manifest-path src-tauri/Cargo.toml --lib

      - name: TS typecheck
        run: npx tsc --noEmit

      - name: Frontend build
        run: npm run build
```

> Linux audio caveat: cpal on Linux wants ALSA dev headers. If the ubuntu job
> fails at the cpal/ort link step with a missing `asound.h`, add a setup step:
> `- name: Install ALSA headers\n  if: runner.os == 'Linux'\n  run: sudo
> apt-get update && sudo apt-get install -y libasound2-dev pkg-config`. Add it
> only if needed (the `cpal` default backend resolves ALSA; newer ubuntu runners
> may already ship it). Do NOT preemptively add deps that aren't confirmed
> needed.

> macOS note: `macos-14` = Apple Silicon. This Phase-1 job builds ort-CPU (no
> CoreML feature yet). If it fails to resolve ort, confirm the `[patch]`
> override (Global Constraints) is present — a clean resolve needs it.

- [ ] **Step 2: Commit + push to trigger CI**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: windows/macos-14/ubuntu matrix (= spikes #1/#2)"
git push
```

- [ ] **Step 3: Read the CI results and record the spike outcome**

Open the GitHub Actions tab. For each job:
- **windows-latest** — must be green (no regression).
- **macos-14** — green = spike #2 (engines build on Apple Silicon, ort-CPU).
  If the `transcribe-rs`/`parakeet-rs` link fails, capture the error — it may be
  the ort-pin override or a missing CoreML feature (Phase 2 fixes; for Phase 1
  the ort-CPU build must resolve).
- **ubuntu-latest** — green = spike #1 (Linux ort-CPU build). Add the ALSA
  headers step if cpal fails.

Record results in `AGENTS.md` Phase-3 status / a `docs/spike-results.md` note:
which spikes passed, any macOS/Linux build quirks, RTF measurement deferred to
Phase 2 (needs the real model on-device).

**Phase 1 is complete when all three jobs are green.**

---

# Phase 2 — macOS port (Apple Silicon)

**Purpose:** make molvi actually RUN on macOS (Apple Silicon) with the full
feature set. Gated on Phase 1 CI green. macOS needs (verified via ctx7 + the
spike): `tauri-nspanel` overlay (Tauri `focusable:false` is broken on macOS —
tauri#14102), ⌘V paste (`Key::Meta` + **`Key::Other(9)`** — virtualKey 9 = physical V; layout-robust per Handy/VoiceInk; NOT `Key::Unicode('v')` or `Key::Control`),
Accessibility permission, NSWorkspace profiles, and (optionally) CoreML EP.

> **Verification constraint:** macOS code can only be compiled/ran on a Mac.
> Each task ends with a `cargo check`/smoke step that MUST run on a `macos-14`
> runner or a local Apple-Silicon Mac. From the Windows dev box, only the
> Windows-arm correctness (compile of the `#[cfg(not(target_os="windows"))]`
> arms is NOT exercised — Windows selects its own arms) is verifiable; treat
> the macOS steps as "write here, verify on Mac."

---

### Task 4: macOS platform dependencies in Cargo.toml

Wire the macOS-only deps: CoreML engine features (Apple Silicon only) +
`tauri-nspanel` (overlay). Verify the resolve succeeds.

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Produces: the crate graph resolves on `aarch64-apple-darwin` with CoreML EP available to both engines + `tauri_nspanel` importable.

- [ ] **Step 1: Add the macOS target dependencies**

Append to `src-tauri/Cargo.toml` (after the Windows target block from Task 2):

```toml
# CoreML EP for both engines, Apple Silicon only (D3; ort-sys dist.tsv has no
# x86_64-apple-darwin row → Intel Mac unsupported without load-dynamic).
# Feature names verified against the resolved crate source
# (~/.cargo/registry/.../transcribe-rs-0.3.11 + parakeet-rs-0.3.7): both map to
# `ort/coreml`.
[target.'cfg(all(target_os = "macos", target_arch = "aarch64"))'.dependencies]
transcribe-rs = { version = "0.3.11", features = ["onnx", "audio-features", "ort-coreml"] }
parakeet-rs = { version = "0.3.7", default-features = false, features = ["cpu", "ort-defaults", "api-28", "coreml"] }

# Overlay focus fix (spike #3): Tauri focusable:false is broken on macOS
# (tauri#14102). tauri-nspanel converts the webview window to a non-activating
# NSPanel. Verified live: /ahkohd/tauri-nspanel, v2.x.
[target.'cfg(target_os = "macos")'.dependencies]
tauri-nspanel = "2"
```

> NOTE: `parakeet-rs` default features = `["cpu","ort-defaults","api-28"]`; we
> re-declare them with `coreml` added so feature-unification doesn't silently
> drop `cpu`. Confirm the resolved `parakeet-rs` default set still matches
> (Cargo.toml lines 49-53 in the registry) before finalizing — if it changed,
> mirror the new defaults.

- [ ] **Step 2: Resolve on macOS (run on a Mac or macos-14 runner)**

Run: `cargo generate-lockfile --manifest-path src-tauri/Cargo.toml` then
`cargo check --manifest-path src-tauri/Cargo.toml`.
Expected: resolves + links. If `ort`/`ort-sys` fails to find the
`aarch64-apple-darwin+coreml` prebuilt, confirm the CoreML feature spelling and
that the runner is Apple Silicon (Intel → no prebuilt, by design).

- [ ] **Step 3: Confirm Windows resolve is unaffected**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS — the macOS target-deps don't touch the Windows graph.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(macos): coreml engine features + tauri-nspanel (Apple Silicon)"
```

---

### Task 5: `paths.rs` — macOS app data dir + `$HOME` redaction

**Files:**
- Modify: `src-tauri/src/paths.rs`

**Interfaces:**
- Produces: `app_data_dir()` returns `~/Library/Application Support/com.molvi.app` on macOS; `redact_appdata()` redacts the `$HOME` prefix on Unix (PII-adjacent).

- [ ] **Step 1: Make `app_data_dir()` cross-platform**

Replace the body of `app_data_dir()` with a cfg dispatch. Windows keeps
`%APPDATA%`; macOS uses `~/Library/Application Support`; Linux uses
`$XDG_CONFIG_HOME` (default `~/.config`) — the Linux arm lands here too (Phase 3
consumes it). Use `std::env::var` (no `dirs` dep — matches the existing ponytail
call at the old line 11):

```rust
pub fn app_data_dir() -> Result<PathBuf> {
    let base = {
        #[cfg(target_os = "windows")]
        {
            // ponytail: %APPDATA% = C:\Users\<name>\AppData\Roaming (std only).
            std::env::var("APPDATA")
                .map_err(|_| MolviError::Paths("APPDATA env var not set".into()))?
                .into()
        }
        #[cfg(target_os = "macos")]
        {
            // ~/Library/Application Support (osx-conventional; std only).
            let home = std::env::var("HOME")
                .map_err(|_| MolviError::Paths("HOME env var not set".into()))?;
            format!("{home}/Library/Application Support")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // $XDG_CONFIG_HOME (default ~/.config) — XDG Base Dir spec.
            std::env::var("XDG_CONFIG_HOME")
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or_default();
                    format!("{home}/.config")
                })
        }
    };
    let dir = PathBuf::from(base).join(IDENTIFIER);
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::Paths(format!("create app data dir: {e}")))?;
    Ok(dir)
}
```

> If `base.into()` to the common `dir` variable fights the type checker (three
> branches → `String`), assign into a `let base: String = { … };` block. Keep
> one `create_dir_all` + `Ok(dir)` after the cfg block.

- [ ] **Step 2: Extend `redact_appdata` to redact `$HOME` on Unix**

The username lives in the path on every OS (`C:\Users\<name>\…`,
`/Users/<name>/…`, `/home/<name>/…`). Rename-in-spirit: keep the function name
`redact_appdata` (callers unchanged) but redact the per-OS home-ish prefix.
Add a Unix `$HOME` arm:

```rust
pub fn redact_appdata(path: &Path) -> String {
    // Windows: redact %APPDATA% prefix (existing behavior).
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            if let Ok(rel) = path.strip_prefix(&appdata) {
                return format!("%APPDATA%\\{}", rel.display());
            }
        }
    }
    // Unix (macOS + Linux): redact $HOME prefix — username is PII-adjacent in
    // shared bug-report logs (~/Library/…, ~/.config/…).
    #[cfg(unix)]
    {
        if let Some(home) = std::env::var_os("HOME") {
            if !home.is_empty() {
                if let Ok(rel) = path.strip_prefix(&home) {
                    return format!("~{}", rel.display());
                }
            }
        }
    }
    path.display().to_string()
}
```

- [ ] **Step 3: Update/extend the `redact_appdata` test**

The existing test (`redact_appdata_strips_prefix_and_falls_back`) is
Windows-specific (uses `%APPDATA%`). Gate the Windows assertions and add a Unix
assertion. Replace the test body:

```rust
    #[test]
    fn redact_appdata_strips_prefix_and_falls_back() {
        // Windows branch: %APPDATA% prefix → "%APPDATA%\…".
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var_os("APPDATA").expect("APPDATA set");
            let under = PathBuf::from(&appdata).join("com.molvi.app").join("models");
            assert_eq!(redact_appdata(&under), r"%APPDATA%\com.molvi.app\models");
        }
        // Unix branch (macOS + Linux): $HOME prefix → "~/…".
        #[cfg(unix)]
        {
            let home = std::env::var_os("HOME").expect("HOME set");
            let under = PathBuf::from(&home).join(".config").join("com.molvi.app");
            let r = redact_appdata(&under);
            assert!(r.starts_with('~'), "redacted: {r}");
            assert!(!r.contains(std::env::var("HOME").unwrap().as_str()));
        }
        // Foreign path (not under any home prefix): raw passthrough (all OSes).
        let foreign = if cfg!(windows) { Path::new(r"C:\foreign\dict.csv") }
                      else { Path::new("/srv/foreign/dict.csv") };
        assert_eq!(redact_appdata(foreign), foreign.display().to_string());
    }
```

The `app_data_dir_ends_with_identifier` + `subpaths_are_nested` tests stay
OS-agnostic (they assert the `com.molvi.app` tail) — no change.

- [ ] **Step 4: Verify on Windows + (Mac) and commit**

```
cargo test --manifest-path src-tauri/Cargo.toml --lib paths
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
(macOS: run the same to exercise the `$HOME` branch.)
Commit: `git commit -m "paths: macOS app-data dir + \$HOME redaction (D5)"`.

---

### Task 6: `paste.rs` — per-platform paste key + macOS verify-only focus guard

The core blocker-#2 fix. Introduce `paste_key()` / `paste_modifier()` helpers
(inline cfg, no abstraction) and route all chord sites through them. Add the
macOS `capture_target`/`ensure_focus` (verify-only — a background app cannot
re-activate another app on macOS, per spike #3).

**Files:**
- Modify: `src-tauri/src/paste.rs`
- Modify: `src-tauri/src/commands.rs` (chord key tables)

**Interfaces:**
- Produces: `paste::paste_modifier() -> enigo::Key`, `paste::paste_key() -> enigo::Key`. Consumed by `paste_text`, `run_command_chord`, and `commands.rs` chord construction.

- [ ] **Step 1: Add the key/modifier helpers (top of `paste.rs`, after imports)**

```rust
/// The modifier held for a paste/replace chord. Windows + Linux = Ctrl;
/// macOS = ⌘ (`Key::Meta`). Verified via ctx7 /enigo-rs/enigo: `Key::Meta`
/// is "Command (macOS), Super (Linux), Windows key" — there is NO
/// `Key::Command` variant (spike #3's "Key::Command" was shorthand; the real
/// variant is `Key::Meta`).
pub fn paste_modifier() -> Key {
    #[cfg(target_os = "macos")]
    { Key::Meta }
    #[cfg(not(target_os = "macos"))]
    { Key::Control }
}

/// The key clicked for a paste. Windows = VK_V (`Key::Other(0x56)` — enigo's
/// Unicode path is rejected as Ctrl+V by some Windows apps). macOS =
/// `Key::Other(9)` (virtualKey 9 = physical V) — NOT `Key::Unicode('v')`:
/// FR/DE layouts remap to QWERTY when ⌘ is held, so a Unicode 'v' hits the
/// wrong physical key. Verified against `cjpais/Handy` (input.rs) + VoiceInk
/// (CursorPaster.swift) — both production macOS dictation apps use virtualKey 9.
/// This is the macOS twin of molvi's Windows `Key::Other(0x56)` reasoning.
/// Linux/X11 = `Key::Unicode('v')` + Ctrl (enigo x11rb, no layout landmine).
pub fn paste_key() -> Key {
    #[cfg(target_os = "windows")]
    { Key::Other(0x56) }
    #[cfg(target_os = "macos")]
    { Key::Other(9) }
    #[cfg(all(unix, not(target_os = "macos")))]
    { Key::Unicode('v') }
}
```

- [ ] **Step 2: Route the paste chord through the helpers**

In `paste_text`, replace the Replace-mode Ctrl+A block (the `Key::Control` /
`Key::Other(0x41)` chord) and the paste Ctrl+V block with helper calls. The
select-all key is also per-OS (Windows VK_A vs Unicode 'a'):

```rust
    // Replace mode: select-all first. macOS = ⌘A, Windows/Linux = Ctrl+A.
    if mode == PasteMode::Replace {
        let select_all_key = if cfg!(target_os = "windows") { Key::Other(0x41) } else { Key::Unicode('a') };
        enigo.key(paste_modifier(), Press).map_err(paste_err("modifier down (select-all)"))?;
        enigo.key(select_all_key, Click).map_err(paste_err("a click"))?;
        enigo.key(paste_modifier(), Release).map_err(paste_err("modifier up (select-all)"))?;
        thread::sleep(Duration::from_millis(20));
        tracing::info!("paste: select-all delivered (replace mode)");
    }

    enigo.key(paste_modifier(), Press).map_err(paste_err("modifier down"))?;
    enigo.key(paste_key(), Click).map_err(paste_err("v click"))?;
    enigo.key(paste_modifier(), Release).map_err(paste_err("modifier up"))?;
    tracing::info!("paste: paste chord delivered");
    Ok(())
```

- [ ] **Step 3: Route the command-mode chord modifier through the helper**

In `run_command_chord`, the `hold_ctrl` arm currently hardcodes `Key::Control`.
The chord keys come from `commands.rs` (per-platform tables — Step 4). Change
the modifier to `paste_modifier()`:

```rust
    if chord.hold_ctrl {
        enigo.key(paste_modifier(), Press).map_err(paste_err("modifier down"))?;
    }
    // chord.keys are already per-platform (commands.rs) — click them as-is.
    for k in &chord.keys { enigo.key(*k, Click).map_err(paste_err("key click"))?; }
    if chord.hold_ctrl {
        enigo.key(paste_modifier(), Release).map_err(paste_err("modifier up"))?;
    }
```

- [ ] **Step 4: Per-platform chord key tables in `commands.rs`**

`commands.rs` builds `KeyChord { hold_ctrl, keys: Vec<Key> }` for command-mode
actions. The keys are currently Windows VKs (`Key::Other(u32)`). On macOS/Linux
they must be `Key::Unicode(char)` / proper keysyms. Add a small per-OS key
resolver where the chord is built (find the `KeyChord` construction site — grep
`Key::Other\|KeyChord` in `commands.rs`). The cleanest inline-cfg shape:

```rust
/// Resolve a command chord's "letter key" per platform. Windows = VK
/// (`Key::Other`); macOS/Linux = Unicode.
fn letter_key(c: char) -> Key {
    #[cfg(target_os = "windows")]
    { let vk = c.to_ascii_uppercase() as u32; Key::Other(vk) }
    #[cfg(not(target_os = "windows"))]
    { Key::Unicode(c) }
}
```

Route every `Key::Other(<vk>)` chord key in `commands.rs` through `letter_key`.
Read `commands.rs` first to enumerate the exact sites (grep `Key::Other`), then
replace each. If a chord uses a non-letter key (arrows, etc.), keep its named
`Key::` variant (those are already cross-platform in enigo).

> `Key::Meta` for the chord modifier is already handled by `paste_modifier()`
> (Step 3). Only the letter keys need re-keying here.

- [ ] **Step 5: Add the macOS focus-guard arms (verify-only)**

Replace the Phase-1 non-Windows `capture_target`/`ensure_focus` stubs in
`paste.rs` with macOS-specific bodies. macOS: `capture_target` = the frontmost
app's `pid_t`; `ensure_focus` = **verify-only, NO restore** (spike #3 — a
background/accessory app cannot reliably re-activate another app):

```rust
#[cfg(target_os = "macos")]
pub fn capture_target() -> Option<isize> {
    crate::macos_frontmost_pid()
}

#[cfg(target_os = "macos")]
fn ensure_focus(target: isize) -> Result<()> {
    // Verify-only: if the frontmost pid changed mid-dictation (user ⌘-tabbed),
    // refuse — do NOT attempt to re-activate (spike #3: not reliable for a
    // background app). The NSPanel overlay (Task 7) keeps focus on the user's
    // app, so a mismatch here means an explicit user switch → safe to refuse.
    if crate::macos_frontmost_pid() == Some(target) {
        Ok(())
    } else {
        tracing::warn!("paste: macOS frontmost app changed; left on clipboard");
        Err(MolviError::Paste("focus mismatch; text left on clipboard".into()))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn capture_target() -> Option<isize> {
    None // Linux X11 (_NET_ACTIVE_WINDOW) lands in Phase 3.
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn ensure_focus(_target: isize) -> Result<()> {
    Ok(())
}
```

`macos_frontmost_pid()` is a tiny objc2 helper added in Task 8 (it reads
`NSWorkspace.shared.frontmostApplication.processIdentifier`). To keep this task
compiling in isolation, define a temporary private stub here and let Task 8
replace it — OR (cleaner) do Task 8's pid helper first. **Recommended: do Task 8
Step 1 (the `macos_frontmost_pid` helper) before Step 5 of this task.** The two
tasks share that one function.

- [ ] **Step 6: Add a compile-gated test for the helpers**

```rust
    #[test]
    fn paste_modifier_matches_platform() {
        #[cfg(target_os = "macos")]
        assert_eq!(paste_modifier(), Key::Meta);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(paste_modifier(), Key::Control);
    }

    #[test]
    fn paste_key_matches_platform() {
        #[cfg(target_os = "windows")]
        assert_eq!(paste_key(), Key::Other(0x56));
        #[cfg(target_os = "macos")]
        assert_eq!(paste_key(), Key::Other(9));
        #[cfg(all(unix, not(target_os = "macos")))]
        assert_eq!(paste_key(), Key::Unicode('v'));
    }
```

(`Key` is `PartialEq`, so this asserts the real variant. Runs on every OS in CI.)

- [ ] **Step 7: Verify (Windows now; Mac in CI) + commit**

```
cargo test --manifest-path src-tauri/Cargo.toml --lib paste
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Commit: `git commit -m "paste: per-platform key/modifier (⌘V macOS) + verify-only focus guard"`.

---

### Task 7: `overlay.rs` — macOS NSPanel conversion (focus fix)

Tauri's `focusable:false` is **broken on macOS** (tauri#14102 / tao#1210): the
overlay steals focus, misrouting paste. Fix = convert the overlay webview
window to a non-activating `NSPanel` via `tauri-nspanel` (verified API:
`WebviewWindowExt::to_panel`, `StyleMask::nonactivating_panel`,
`PanelLevel::Status`, `CollectionBehavior::full_screen_auxiliary`,
`set_hides_on_deactivate(false)`).

**Files:**
- Modify: `src-tauri/src/overlay.rs`
- Modify: `src-tauri/src/lib.rs` (init the panel once in `setup`)

**Interfaces:**
- Produces: `overlay::show`/`hide`/`window` work on macOS without stealing focus.

- [ ] **Step 1: Add the macOS panel init in `lib.rs` setup**

In the `.setup` closure (after the window manager is ready), convert the
overlay window to a panel ONCE. Use `tauri-nspanel`'s macro + trait:

```rust
// macOS-only overlay focus fix (spike #3; tauri#14102). Define the panel type
// + convert the existing "overlay" webview window into a non-activating NSPanel
// so show/hide never steals keyboard focus from the user's app.
#[cfg(target_os = "macos")]
mod macos_overlay {
    use tauri::Manager;
    use tauri_nspanel::{tauri_panel, ManagerExt, PanelLevel, StyleMask, WebviewWindowExt};

    tauri_panel! {
        panel!(OverlayPanel {
            config: {
                // can_become_key_window: false → the panel CANNOT take keyboard
                // focus (the whole point — paste stays routed to the user's app).
                can_become_key_window: false,
                can_become_main_window: false,
            }
        })
    }

    /// Convert the "overlay" window to a non-activating panel. Idempotent +
    /// best-effort: logs + skips on any error (the app still runs, just with
    /// the focus-stealing Tauri default — better than a startup crash).
    pub fn init_overlay_panel(app: &tauri::AppHandle) {
        let Some(win) = app.get_webview_window("overlay") else {
            tracing::warn!("macOS overlay panel: window not found");
            return;
        };
        let panel = match win.to_panel::<OverlayPanel>() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("macOS overlay to_panel failed: {e}");
                return;
            }
        };
        // Non-activating style mask; show over full-screen windows; join all
        // spaces; never hide-on-deactivate (a non-activating app is permanently
        // "deactivated", so the default hide-on-deactivate would vanish it).
        panel.set_style_mask(
            StyleMask::empty().nonactivating_panel().into(),
        );
        panel.set_level(PanelLevel::Status.value());
        panel.set_collection_behavior(
            tauri_nspanel::CollectionBehavior::new()
                .full_screen_auxiliary()
                .can_join_all_spaces()
                .into(),
        );
        panel.set_hides_on_deactivate(false);
        tracing::info!("macOS overlay panel initialized");
    }
}
```

Call `macos_overlay::init_overlay_panel(&app.handle())` inside `.setup` (gate
the call with `#[cfg(target_os="macos")]` or put it behind a `if cfg!(macos)`
check — keep the module itself cfg-gated so non-Mac builds never see the
`tauri_nspanel` import).

> Verify the exact `tauri_panel!` / `panel!` macro shape + `set_style_mask` /
> `set_collection_behavior` signatures against the resolved
> `tauri-nspanel` version on a Mac (ctx7 `/ahkohd/tauri-nspanel` shows v2.1; the
> API above is from the verified fullscreen + to_panel examples). If a method
> name differs, adjust — the *behavior* (non-activating, status-level,
> full-screen-auxiliary, never-hide-on-deactivate) is the invariant.

- [ ] **Step 2: Keep `overlay.rs` show/hide cross-platform**

`overlay.rs` already uses `w.show()`/`w.hide()`/`w.set_focusable(false)` — these
operate on the underlying window whether or not it's been panel-converted, so
**no change needed to `overlay.rs` for basic show/hide.** The `set_focusable`
call in `hide()` is a harmless no-op on a panel-converted window (it's already
non-activating). Confirm via the macOS smoke (Task 10) that the overlay shows
without stealing focus.

If the smoke reveals the panel needs explicit `show`/`hide` via the panel API
(rather than the window API), add a thin cfg branch in `overlay::show`/`hide`
that retrieves the stored panel from `app.state` and calls `panel.show()` /
`panel.order_out(None)`. Defer until the smoke proves it necessary (ponytail:
don't pre-build the branch).

- [ ] **Step 3: Verify (Mac) + commit**

```
cargo check --manifest-path src-tauri/Cargo.toml        # Mac
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Commit: `git commit -m "overlay(macOS): non-activating NSPanel via tauri-nspanel (focus fix)"`.

---

### Task 8: `profiles.rs` — macOS NSWorkspace `foreground_exe` + pid helper

Profiles are fully functional on macOS (D5): `foreground_exe()` via
`NSWorkspace.shared.frontmostApplication`. Also provides the
`macos_frontmost_pid()` helper that Task 6's focus guard consumes.

**Files:**
- Modify: `src-tauri/src/profiles.rs` (or a small `lib.rs` helper)

**Interfaces:**
- Produces: `profiles::foreground_exe()` returns the uppercased app basename on macOS; `crate::macos_frontmost_pid() -> Option<isize>` for the paste focus guard (Task 6).

- [ ] **Step 1: Add `macos_frontmost_pid()` (objc2, macOS-only)**

`objc2-app-kit` (NSWorkspace) + `objc2-foundation` are transitively available
via enigo/arboard on macOS. Add a macOS helper (in `lib.rs` or `profiles.rs`):

```rust
#[cfg(target_os = "macos")]
pub fn macos_frontmost_pid() -> Option<isize> {
    use objc2_app_kit::NSWorkspace;
    let ws = NSWorkspace::sharedWorkspace();
    let app = ws.frontmostApplication()?;
    Some(app.processIdentifier() as isize)
}
```

> Verify the exact `objc2-app-kit` method names against the resolved version on
> a Mac (`NSWorkspace::sharedWorkspace` / `shared`, `frontmostApplication`,
> `processIdentifier`). If the transitively-resolved objc2-app-kit isn't a direct
> dep, add `objc2-app-kit` to the macOS target-deps in Cargo.toml (Task 4's
> block). The *contract* (`pid_t` of the frontmost app) is the invariant; the
> selector names are version-specific.

- [ ] **Step 2: Implement macOS `foreground_exe()`**

Replace the Phase-1 non-Windows stub arm with a macOS branch (basename of the
frontmost app's bundle/executable URL, UPPERCASED — mirrors the Windows
basename contract so `resolve()` matches unchanged):

```rust
pub fn foreground_exe() -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        // … existing Win32 body …
    }
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws.frontmostApplication()
            .ok_or_else(|| MolviError::Profile("no frontmost app".into()))?;
        // bundleURL.path → basename, UPPERCASED (e.g. "WORD.APP"). Tolerate
        // both separators. Fail-open on a missing/odd URL.
        let url = app.bundleURL().ok_or_else(|| MolviError::Profile("no bundleURL".into()))?;
        let path = url.path().to_string();
        let base = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_ascii_uppercase();
        if base.is_empty() {
            return Err(MolviError::Profile("empty app basename".into()));
        }
        tracing::debug!("foreground exe: {base}");
        Ok(base)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Err(MolviError::Profile("foreground_exe not implemented on this OS yet".into()))
    }
}
```

> The Windows arm keeps its `use windows::…` imports inside its block (from
> Task 2 Step 4). Verify `bundleURL().path()` against the resolved objc2-app-kit
> on a Mac — the exact accessor may be `.path()` returning `Retained<NSString>`
> (then `.to_string()`).

- [ ] **Step 3: Verify (Mac) + commit**

```
cargo test --manifest-path src-tauri/Cargo.toml --lib profiles
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Commit: `git commit -m "profiles(macOS): NSWorkspace foreground_exe + pid helper (D5)"`.

---

### Task 9: macOS permissions (Accessibility + mic) + bundle config

enigo macOS needs **Accessibility** permission (`open_prompt_to_get_permissions`
is default `true` in enigo — a prompt fires on first `Enigo::new`). Mic capture
needs the macOS mic permission (Tauri/cpal handle the system prompt; add the
`NSMicrophoneUsageDescription` Info.plist key). Wire a first-run check + the
bundle config.

**Files:**
- Modify: `src-tauri/tauri.conf.json` (bundle)
- Modify: `src-tauri/src/lib.rs` (ensure enigo is constructed with the prompt enabled — already the default; explicit is safer)

- [ ] **Step 1: Add the macOS bundle config to `tauri.conf.json`**

In the `bundle` section, add macOS-specific keys (Tauri merges these into the
app's Info.plist):

```json
    "macOS": {
      "minimumSystemVersion": "12.0",
      "infoPlist": {
        "NSMicrophoneUsageDescription": "molvi needs microphone access for local on-device dictation. Audio never leaves your device.",
        "LSUIElement": true
      }
    }
```

`LSUIElement: true` makes molvi a background/accessory app (no Dock icon) —
correct for a PTT overlay app, and required for the non-activating NSPanel model
(spike #3). Verify the Tauri 2.11 bundle schema accepts `macOS.infoPlist`
(ctx7 `/websites/v2_tauri_app`).

- [ ] **Step 2: Confirm enigo's Accessibility prompt path**

`Settings::default()` already sets `open_prompt_to_get_permissions = true`
(verified ctx7). The paste code constructs `Enigo::new(&Settings::default())`
(paste.rs:88, 136, 155) — so the prompt fires on first paste. No code change
needed for the prompt itself. Add a one-line note in `paste.rs` above the first
`Enigo::new`:

```rust
    // macOS: Enigo::new fires the Accessibility-permission prompt on first use
    // (Settings::default() sets open_prompt_to_get_permissions = true). The
    // user grants it once; subsequent pastes work.
```

- [ ] **Step 3: Build the `.app` + verify (Mac)**

Run: `cargo tauri build` (on a Mac). Verify the `.app` launches, the mic
permission prompt fires on first dictation, and the Accessibility prompt fires
on first paste. (Notarization + code signing are deployment work — out of scope
for this task; the unsigned `.app` runs locally for testing.)

- [ ] **Step 4: macOS Secure Input detection (landmine — Handy finding)**

Verified from `cjpais/Handy` (`src-tauri/src/secure_input.rs`): when another app
grabs `SecureEventInput` (password fields, 1Password, some terminals), macOS
**silently disables global hotkeys** — molvi's PTT stops working with no error.
Handy ships a whole module: `is_enabled_now()` / `reconcile_fallback()` /
`note_recorder_blocked()` + a user-facing warning. Add the detection:

```rust
#[cfg(target_os = "macos")]
pub fn secure_input_held() -> bool {
    // objc2/AppKit: GetCurrentEventKeyModifiers + IsSecureEventInputGuaranteed
    // (verify exact API against the resolved SDK). Returns true when another app
    // holds SecureEventInput → molvi's global hotkey is suppressed.
    # false
}
```

Call it from the hotkey-registration path; on `true`, emit a toast (once per
session) like "Another app (password field / 1Password) is holding Secure Input
— global hotkey paused." This is a known macOS trap molvi WILL hit; detecting it
turns a mystifying failure into an explainable one.

- [ ] **Step 5: ⚠ Verify whether stock Tauri 2.11 + `tauri-nspanel` suffices (Handy forks the runtime)**

`cjpais/Handy` (the closest analog — same Tauri + transcribe-rs stack) ships a
`[patch.crates-io]` override of `tauri-runtime` / `tauri-runtime-wry` /
`tauri-utils` → `cjpais/tauri` branch `handy-2.10.2`. The NSPanel focus fix
(Task 7) may need runtime-level patches beyond the plugin. **Before finalizing
Task 7, verify on a Mac** whether stock Tauri 2.11.5 + `tauri-nspanel` v2.1
produces a non-activating overlay, or whether molvi must mirror Handy's runtime
fork. If the fork is required, document it as a `[patch]` (mirrors molvi's
existing transcribe-rs patch posture).

Commit: `git commit -m "macOS: mic/LSUIElement + Secure Input detection"`.

---

### Task 10: macOS blaze smoke (RTF measurement gate)

The hard NFR: default RU/PTT/Smart RTF ≤ 0.03 on Apple Silicon, verified by
measurement (not assumed). CoreML EP is the path to preserve/improve the moat;
if CoreML rejects the graphs or isn't faster, ort-CPU is the fallback (still
works, document the baseline).

**Files:**
- Modify: `AGENTS.md` (record the macOS NFR row)

- [ ] **Step 1: Measure the default-path RTF on Apple Silicon**

On a Mac with the GigaAM model cached, run a controlled long-utterance
dictation (RU, PTT, Smart mode) and read the RTF from the molvi log (the
finalize path logs RTF metadata). Compare ort-CPU vs CoreML EP (toggle the
CoreML feature if both are measurable). Record:
- macOS Apple Silicon, GigaAM, RTF: `<value>` (CoreML on / ort-CPU).
- macOS Apple Silicon, Nemotron, RTF: `<value>` (8960-boundary unchanged).
- Whether CoreML accepts the graphs (spike #2's open question).

- [ ] **Step 2: Record the result + decide**

If RTF ≤ 0.03 → record in `AGENTS.md` NFR table, macOS port is blaze-clean.
If RTF > 0.03 but close → document as macOS's measured baseline (never a silent
regression); investigate hot-loop allocations on the Mac. If CoreML fails to
load the graph → fall back to ort-CPU, document.

- [ ] **Step 3: Human smoke the full feature set**

Dictate in Russian + English (Nemotron), test profiles (open a known app,
verify `foreground_exe` matches → profile applies), test the overlay (no focus
steal), test command-mode chords (⌘-keyed), test paste into several apps. All
must work.

**Phase 2 is complete when:** `.app` builds + runs on Apple Silicon, blaze gate
measured + recorded, full feature set smoke-tested. Commit any final fixes.

---

# Phase 3 — Linux (X11 + Wayland via compositor keybinding)

**Purpose:** ship molvi on Linux. X11 comes relatively free (global-hotkey
works on X11; enigo x11rb; `_NET_WM_PID`/`_NET_ACTIVE_WINDOW`).

**Wayland decision — RESOLVED by competitor research (2026-08-07):** the
`ashpd` `GlobalShortcuts` portal is **NOT viable for molvi** — it fails for
overlay/layer-shell/unfocusable windows (`ashpd#213`; `oddlama/whisper-overlay`
abandoned it for exactly this reason, and `cjpais/Handy`'s Linux overlay uses
GTK Layer Shell, not the portal). The converged, proven, zero-permission path
(voxtype / whisrs / hyprwhspr / nerd-dictation) is: **compositor keybinding → a
`molvi record toggle` CLI/IPC subcommand → signals the running app**, with an
**optional evdev fallback** (`input` group + shipped udev rule) for users who
want app-internal PTT. This covers KDE/Hyprland/Sway/Niri/GNOME at near-zero
cost. Portal paths (GlobalShortcuts hotkey, RemoteDesktop/libei paste) are
**deferred** until upstream matures.

> Linux code is only verifiable on a Linux box / the ubuntu CI runner.

---

### Task 11: Wayland PTT trigger — compositor keybinding + `molvi` IPC subcommand

**RESOLVED (not open):** competitor research (`gh`-verified against voxtype,
whisrs, hyprwhspr, nerd-dictation, whisper-overlay, speedofsound) proved the
`ashpd` GlobalShortcuts portal is broken for overlay apps (`ashpd#213`). The
converged path is a **compositor keybinding → CLI subcommand → IPC signal to the
running daemon**. This task builds molvi's half of that (the subcommand + IPC),
and documents the user's half (the one-line compositor config).

**Files:**
- Create: `docs/linux-install.md` (the compositor keybinding instructions)
- Modify: `src-tauri/src/lib.rs` (single-instance already guards "is molvi running"; add: a CLI-arg path that, if molvi is already running, sends the toggle signal and exits)
- Modify: `src-tauri/src/coordinator.rs` (the toggle IPC target — likely already exists for the tray toggle)

**Interfaces:**
- Produces: `molvi record toggle` (and `start`/`stop`) as a CLI subcommand that signals a running instance; the same signal the tray/hotkey already emit.

- [ ] **Step 1: Confirm the existing toggle signal path**

molvi already has a PTT toggle used by the tray (and the global-hotkey on
Windows/X11). Grep `coordinator::Command` for the toggle/start/stop commands.
The CLI subcommand will emit the SAME command over the single-instance channel
(`tauri-plugin-single-instance` forwards argv to the running instance) — so the
CLI path is a thin argv-parser that calls into the existing single-instance
`init` callback. No new IPC mechanism.

- [ ] **Step 2: Add the argv dispatch in `lib.rs`**

In the `tauri_plugin_single_instance` init handler (which receives argv when a
2nd instance launches), parse `record toggle|start|stop` and forward to the
existing coordinator command channel. The 2nd instance then exits. The 1st
instance (the running app) acts on it exactly like a tray click. This makes
`molvi record toggle` from any compositor keybinding equivalent to pressing the
PTT hotkey. Verify the single-instance plugin's argv-forwarding API (ctx7
`/websites/v2_tauri_app`).

- [ ] **Step 3: Write the compositor-keybinding docs**

`docs/linux-install.md` — one section per compositor (Hyprland/Sway/Niri/
River/GNOME/KDE), each a one-liner, e.g. Hyprland:
`bind = SUPER, V, exec, molvi record toggle` (and `bindr` for key-release if
molvi wants press-to-start/release-to-stop). This is the user-facing half; molvi
ships no global-hotkey on Wayland by design.

- [ ] **Step 4: (Optional) evdev fallback hotkey**

For users who want app-internal PTT (no compositor config), add an opt-in evdev
reader (`/dev/input/event*`, `input` group + shipped
`/etc/udev/rules.d/99-molvi.rules`) behind a settings toggle. whisrs/whisper-
overlay prove the path. This is a follow-up, not v1-blocking — the compositor
keybinding covers everyone with zero permissions.

- [ ] **Step 5: Commit**

```bash
git add docs/linux-install.md src-tauri/src/lib.rs
git commit -m "linux: compositor-keybinding PTT via molvi record toggle IPC (Wayland)"
```

---

### Task 12: Linux platform bodies (X11 + Wayland)

Implement the Linux arms of the cfg dispatches. **X11 bodies are full**; the
**Wayland paste path = wl-clipboard** (Task 11 already covers the trigger;
Step 4 here covers paste). No portal/libei deps for v1.

**Files:**
- Modify: `src-tauri/src/profiles.rs` (X11 `foreground_exe`; Wayland → Err for now)
- Modify: `src-tauri/src/paste.rs` (X11 `capture_target`/`ensure_focus`; Wayland paste = wl-clipboard)
- Modify: `src-tauri/src/paths.rs` (already done in Task 5 — Linux arm is in place)
- Modify: `src-tauri/src/model_store.rs` (Linux `has_disk_space` via `statvfs`)
- (Follow-up only) `src-tauri/src/profiles_linux.rs` per-compositor adapters (Step 5)

**Interfaces:**
- Produces: X11 `foreground_exe()` (via `_NET_WM_PID`), `capture_target()` (via `_NET_ACTIVE_WINDOW`), `ensure_focus()` (verify + restore), `has_disk_space()` (via `statvfs`).

- [ ] **Step 1: X11 `foreground_exe()` — `_NET_WM_PID` → `/proc/<pid>/exe`**

x11rb is transitively available via enigo. Replace the Linux stub arm in
`profiles.rs`:

```rust
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // X11 EWMH: _NET_ACTIVE_WINDOW → its _NET_WM_PID → /proc/<pid>/exe
        // basename. Wayland session → per-compositor IPC (NOT "impossible" —
        // whisrs proves hyprctl activewindow / swaymsg / niri msg all work;
        // GNOME/Mutter → AT-SPI accessibility bridge fallback). v1 ponytail:
        // detect Wayland by $WAYLAND_DISPLAY and return Err (profiles degrade);
        // a follow-up adds the per-compositor adapters (Task 12 Step 5).
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Err(MolviError::Profile("Wayland: foreground_exe deferred to per-compositor adapter".into()));
        }
        // … x11rb connect → intern _NET_ACTIVE_WINDOW/_NET_WM_PID → query root
        //   → read _NET_WM_PID → readlink /proc/<pid>/exe → basename → UPPERCASE …
        // Verify the exact x11rb EWMH API against docs.rs/x11rb at execution time.
        # Err(MolviError::Profile("X11 foreground_exe: TODO body".into()))
    }
```

> The x11rb EWMH query is ~30 lines (connect, intern the atoms, get_property on
> the root window for `_NET_ACTIVE_WINDOW`, then on that window for
> `_NET_WM_PID`, then `readlink /proc/<pid>/exe`). Implement it fully at
> execution time against the live `x11rb` docs. Fail-open (`Err`) at every step.

- [ ] **Step 2: X11 `capture_target()` + `ensure_focus()` (verify + restore)**

spike #3: X11 `capture_target` = active window id (`_NET_ACTIVE_WINDOW`);
`ensure_focus` = verify + restore (send a `_NET_ACTIVE_WINDOW` client message
to the root window) + fallback. Replace the Linux stubs in `paste.rs`:

```rust
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Wayland → None (no active-window API); X11 → _NET_ACTIVE_WINDOW id.
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            None
        } else {
            // x11rb: read _NET_ACTIVE_WINDOW → window id as isize
            # None
        }
    }
```

```rust
    #[cfg(all(unix, not(target_os = "macos")))]
    fn ensure_focus(target: isize) -> Result<()> {
        // Wayland → Ok (no restore possible; the §6.6 fallback is clipboard+toast,
        // handled by the caller when capture_target was None anyway). X11 →
        // verify via _NET_ACTIVE_WINDOW; if mismatched, send a _NET_ACTIVE_WINDOW
        // client message to the root window (message_type=_NET_ACTIVE_WINDOW,
        // format=32, data=[2, CurrentTime, 0, 0, target]); re-verify; else Err.
        # Ok(())
    }
```

> Implement the x11rb bodies fully against docs.rs/x11rb at execution time.

- [ ] **Step 3: Linux `has_disk_space()` via `statvfs`**

Replace the Phase-1 `Ok(true)` stub with a real `statvfs` call (libc, or
`std::os::unix::fs::MetadataExt` if it exposes free blocks — verify; otherwise
a tiny `libc::statvfs` FFI). Gate to Unix:

```rust
    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn has_disk_space(needed: u64) -> Result<bool> {
        // statvfs on the models dir; compare frsize * bavail vs needed.
        // macOS could also use this (statfs), but Task 5's macOS path may keep
        // the stub until measured — coordinate.
        # Ok(true)
    }
```

- [ ] **Step 4: Wayland paste = `wl-copy`/`wl-paste` (NOT enigo libei)**

Verified against espanso + wdotool + the enigo issue tracker: enigo's libei/
Wayland path is **fragile** (panics inside a Tokio runtime — enigo#453, and
molvi's finalize runs under `tauri::async_runtime`; dies if the Enigo struct is
recreated per-call — molvi does this at paste.rs:88/136/155; rejected by
Chromium/Electron — enigo#336). The robust, compositor-agnostic path is
`wl-clipboard` (`wl-copy`/`wl-paste`), same as espanso's Wayland clipboard.

For the Wayland session, the paste chord becomes a **clipboard-primary** path
(it already is on every platform — molvi pastes via clipboard + a keystroke):
1. **`wl-copy <text>`** — set the clipboard (shells out to the `wl-copy` binary;
   compositor-agnostic, no portal, no root). Verify `wl-clipboard` is installed
   (document as a dep; or `arboard` with the opt-in `wayland-data-control` feature
   already wraps wl-clipboard — prefer arboard so the clipboard API stays uniform).
2. **Blast-release all modifiers, then send Ctrl+V.** PTT-release-then-paste race
   (wdotool `--clearmodifiers`): the hotkey modifier may still be logically down
   on Wayland (you can't read modifier state), so unconditionally send KeyUp for
   Ctrl/Shift/Alt/Super/AltGr before the Ctrl+V chord.
3. If enigo keystroke inject fails entirely on a given compositor, fall back to
   `wl-paste`-into-focused-field is impossible (no paste trigger) → degrade to
   "text copied to clipboard, press Ctrl+V" toast (the §6.6 safe fallback).

> Gate this whole branch with `#[cfg(all(unix, not(target_os="macos")))]` +
> runtime `WAYLAND_DISPLAY` check. X11 keeps the enigo-x11rb path (Step 2). Do
> NOT enable enigo's `libei`/`wayland` Cargo features for v1 (they pull fragile
> deps; arboard's `wayland-data-control` covers clipboard).

- [ ] **Step 5: (Follow-up) per-compositor foreground-app adapters**

Not v1-blocking. Add `src-tauri/src/profiles_linux.rs` (or inline cfg arms)
mirroring whisrs's `src/window/{hyprland,sway,niri,x11,dbus}.rs`: shell out to
`hyprctl -j activewindow` / `swaymsg -t get_tree` / `niri msg --json
focused-window` → basename → profile match. GNOME/Mutter has no such IPC →
AT-SPI accessibility bridge (hyprwhspr's approach) or degrade. Until then,
profiles on Wayland no-op gracefully (Step 1's `Err`).

- [ ] **Step 5: Verify (ubuntu CI) + commit**

```
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```
Commit: `git commit -m "linux: X11 foreground_exe + focus-guard + statvfs"`.

---

### Task 13: Linux packaging (AppImage + deb + rpm)

**Files:**
- Modify: `src-tauri/tauri.conf.json` (bundle.linux)

- [ ] **Step 1: Add the Linux bundle config**

```json
    "linux": {
      "deb": { "depends": ["libasound2"] },
      "appImage": { "bundleMediaFramework": false },
      "rpm": { "depends": ["alsa-lib"] }
    }
```

(Tauri 2 builds AppImage + deb + rpm from this. Verify the exact schema keys
against ctx7 `/websites/v2_tauri_app` at execution time — Tauri 2.11 bundle
config may differ slightly from v1.)

- [ ] **Step 2: Build + smoke on Linux**

`cargo tauri build` on Linux (ubuntu CI or a Linux box). Smoke the full feature
set on X11 (and Wayland if Task 11 chose it). Record the Linux NFR row (RTF ≤
0.03 measured).

- [ ] **Step 3: Record the Linux NFR + commit**

Update `AGENTS.md` NFR table with the Linux RTF. Commit the bundle config.

---

## Self-Review

**1. Spec coverage** — checked each spec section against the tasks:
- D1 (license MIT/Apache) → Task 1. ✓
- D2 (inline cfg, no mod platform) → every task uses inline `#[cfg]`; no `mod platform` introduced. ✓
- D3 (sequence spikes→macOS→Linux) → Phase 1 (CI=spikes) → Phase 2 (macOS) → Phase 3 (Linux). ✓
- D4 (Vosk deferred) → not in any task. ✓
- D5 (full feature parity; Wayland degrades) → Task 8 (macOS profiles), Task 12 Step 1 (Wayland → None degrade). ✓
- 3 blockers: Wayland hotkey → Task 11 (decision) + Task 12 Step 4; per-platform paste key → Task 6; Intel Mac → Task 4 cfg-gates Apple-Silicon-only. ✓
- 3 spikes: #1 Linux engine → Task 3 ubuntu job; #2 macOS engine → Task 3 macos-14 job + Task 10 CoreML; #3 paste focus-guard → Tasks 6-8 consume the spike findings. ✓
- Platform-coupling inventory (6 boundaries) — `foreground_exe` (T2/T8/T12), `app_data_dir` (T5), `capture_paste_target`/`ensure_focus` (T2/T6/T12), `has_disk_space` (T2/T12), `play_sound_file` (T2), `ort_affinity` (T2). All 6 covered. ✓
- NFRs (blaze RTF ≤0.03, privacy §10.1) → Global Constraints + Task 10 (blaze) + Task 5 (redaction). ✓
- Success criteria 1-7 (spec) → T3 (build+CI), T4-T10 (macOS), T11-T13 (Linux), T1 (license), T9/T13 (packaging). ✓

**2. Placeholder scan** — Phase 1 (Tasks 1-3) has zero placeholders: every step
has exact code. Phase 2/3 mark `# TODO body` / "verify against resolved version"
**only** for OS-specific FFI (objc2 NSWorkspace, x11rb EWMH) that physically
cannot be compiled/verified on the Windows dev box — each is flagged with a
verification step + the *contract* (return type + behavior) is fully specified.
This is the honest boundary of cross-OS planning, not a knowledge placeholder.

**3. Type consistency** — `paste_key() -> Key`, `paste_modifier() -> Key`,
`macos_frontmost_pid() -> Option<isize>`, `capture_target() -> Option<isize>`,
`ensure_focus(isize) -> Result<()>`, `has_disk_space(u64) -> Result<bool>`,
`foreground_exe() -> Result<String>` — signatures are consistent across the
tasks that share them (Task 6 ↔ Task 8 share `macos_frontmost_pid`; Task 2
stubs ↔ Task 6/8/12 real bodies share all signatures). `letter_key(char) ->
Key` (Task 6 Step 4) matches `commands.rs`'s `KeyChord.keys: Vec<Key>`.

---

## Execution notes

- **Phase 1 is independently shippable** (license + cross-platform build + CI)
  and should land before Phase 2 starts. It is the only phase fully verifiable
  from the Windows dev box (via the CI matrix).
- **Phase 2/3 require their target OS** to verify. Execute them on a Mac / the
  ubuntu runner, or accept that the Windows box only checks the Windows arm +
  type-checks the cfg structure.
- **Re-verify crates live before each Phase 2/3 task** (objc2-app-kit,
  tauri-nspanel, x11rb, ashpd) via ctx7/docs.rs — the API names flagged
  "verify" are the ones most likely to have version-specific drift.
- **Gates per task:** cargo fmt + clippy `-D warnings` + `cargo test --lib` +
  (binary-unlocked) `cargo test --test log_privacy` + `npx tsc --noEmit` +
  `npm run build`. Never kill a running `cargo tauri dev`.

---

# Appendix — competitor research findings (2026-08-07)

Source-verified via `gh` (GitHub CLI) against live repos/READMEs/issues. The
findings below DROVE several corrections to this plan (macOS paste key, Wayland
decision, Wayland paste path, Secure Input). Re-runnable `gh` commands at the
end of each subsection.

## A. macOS dictation apps

**`cjpais/Handy` (28,930★, Rust/Tauri 2.10 + transcribe-rs)** — molvi's closest
sibling (SAME engine author — transcribe-rs powers both). macOS-first, also
Win/Linux. The single most relevant reference.
- **Overlay**: `tauri-nspanel` `PanelBuilder` + `tauri_panel!{}` with
  `can_become_key_window: false, is_floating_panel: true` + `.no_activate(true)`
  + `.style_mask(borderless().nonactivating_panel())` + `.level(Status)` +
  `.collection_behavior(can_join_all_spaces().full_screen_auxiliary())`.
  **Validates molvi's Task 7 approach.** Linux overlay = GTK Layer Shell.
- **Paste** (`src-tauri/src/input.rs` `send_paste_ctrl_v`): macOS =
  `Key::Meta` + **`Key::Other(9)`**; Windows = `Key::Control` +
  `Key::Other(0x56)` (matches molvi); Linux = `Key::Control` + `Key::Unicode('v')`.
  → **Drove the Task 6 correction.** Also: `hold_ms` modifier-hold; 6
  user-selectable paste methods (CtrlV / Direct / CtrlShiftV-terminal /
  ShiftInsert / ExternalScript / None).
- **Secure Input** (`src-tauri/src/secure_input.rs`): full detection module. →
  **Drove Task 9 Step 4.**
- **⚠ Runtime fork**: Handy `[patch.crates-io]`-es tauri-runtime/wry/utils →
  `cjpais/tauri#handy-2.10.2`. → **Drove Task 9 Step 5 (verify stock Tauri).**
- Perf: `emit_to` + atomic `OVERLAY_ENABLED` cache (issue #1279) to stop 24 Hz
  WebKit `mic-level` allocations to a hidden overlay.

**`Beingpax/VoiceInk` (5,791★, Swift)** — native NSPanel + CGEvent reference.
- `MiniRecorderPanel: NSPanel` (`.nonactivatingPanel`, `canJoinAllSpaces`,
  `fullScreenAuxiliary`). Sets `canBecomeKey:true` (interactive) — molvi wants
  `false` (Handy's choice).
- `CursorPaster.swift`: CGEvent ⌘V with **virtualKey 9**, layout-remap
  detection, **clipboard-restore-with-session-marker** (snapshot all pasteboard
  types → set text+sessionID → restore only if still owned). Race-safe — worth
  upgrading molvi's replace-paste to this.
- Hotkey: `CGEventTap` (not Carbon). molvi keeps Carbon (simpler).

**`ahkohd/tauri-nspanel` (412★)** — molvi's planned plugin. `no_activate(true)`
works by temporarily setting `NSApplicationActivationPolicy::Prohibited` during
window creation. Pin `branch = "v2.1"` (Handy does).

## B. Linux dictation tools (Wayland hotkey + paste)

Researched 8 tools. **None successfully use the ashpd GlobalShortcuts portal for
the hotkey.** Converged pattern = compositor-keybinding + evdev-fallback trigger;
wtype/dotool/clipboard paste.

| Tool | Trigger | Paste | Wayland |
|---|---|---|---|
| nerd-dictation (1909★) | WM-bound CLI | xdotool/wtype/ydotool/dotool (user picks) | works |
| numen (sourcehut, AGPL) | always-on voice daemon | dotool (/dev/uinput) | works |
| **voxtype** (1047★, Rust) | **compositor keybinding → SIGUSR daemon** + evdev fallback | wtype→dotool→ydotool→clipboard chain | works |
| **whisrs** (81★, Rust) | compositor keybinding → daemon + evdev | layout-aware wtype | works (daily-driver Hyprland) |
| hyprwhspr (1135★) | compositor keybinding | wl-clipboard/wtype + xclip/xdotool | works |
| whisper-overlay (Rust) | **pure evdev**; tried GlobalShortcuts → **FAILED (ashpd#213)** | `virtual-keyboard-v1` native | Wayland-only |
| speedofsound (Kotlin) | DE-bound trigger.sh (portal "not widely supported") | RemoteDesktop/EIS portal | works |

**Decisive evidence for the Task 11 decision:** `oddlama/whisper-overlay`'s
README: *"I didn't manage to get the GlobalShortcuts desktop portal to work with
windows using the layer-shell protocol (ashpd#213)."* molvi IS an overlay app →
the portal is broken for it specifically. `tauri-apps/global-hotkey` README still
says "Linux (X11 Only)" (issue #28 open; maintainer FabianLars: no capacity,
route via ashpd). → **Compositor-keybinding → `molvi record toggle` IPC is the
proven zero-permission v1 path.**

**Foreground-app (profiles) on Wayland = per-compositor, NOT impossible.** whisrs
ships `src/window/{hyprland,sway,niri,x11,dbus}.rs` (`hyprctl activewindow` /
`swaymsg` / `niri msg`). GNOME/Mutter → AT-SPI accessibility bridge (hyprwhspr).
→ **Drove Task 12 Step 5 (per-compositor adapters as follow-up).**

## C. Cross-platform ASR engine layer

**`k2-fsa/sherpa-onnx` (14,023★)** — the reference cross-platform ASR.
- **Engine layer = hardcoded curated factory** (`offline-recognizer-impl.cc`:
  ~20 `#include "offline-recognizer-<family>-impl.h"` + `if/else` dispatch). **No
  plugin registry, no dlopen.** → **Validates molvi's curated `SpeechEngine`
  trait + 2 engines decision** — even the most engine-rich project does this.
  Cleanly separates *model family* (compile-time) from *execution provider*
  (runtime ort EP: CPU/CoreML/CUDA/…). molvi already mirrors this.
- CI matrix worth stealing: `[ubuntu-latest, macos-latest, macos-15-intel,
  ubuntu-22.04-arm, windows-latest]` (the Rust-wrapper slice of its 204 workflows).
- CoreML = ort `CoreMLExecutionProvider` (one feature) — **NOT** whisper.cpp's
  bespoke encoder-only bridge. Validates molvi's `ort/coreml` plan.
- Ships Nemotron-3.5-ASR-streaming export with chunk matrix
  `[80,160,320,560,1120]ms` — **560 ms = molvi's exact 8960-sample boundary**.
  Confirms molvi's streaming boundary is canonical.
- Official Tauri v2 examples (`tauri-examples/`) — another reference port.

**`ggerganov/whisper.cpp` (52,648★)** — CoreML = bespoke ggml bridge, encoder
ONLY, ~3× faster, slow first-run ANE compile. **Do NOT copy** (molvi's
ort/CoreML gives whole-graph offload with one feature). Audio = `miniaudio.h`
(cpal is molvi's Rust analog — confirmed correct choice).

**`SYSTRAN/faster-whisper` (24,788★)** — minimal CI (1 ubuntu job), delegates
per-platform binaries to upstream CTranslate2 wheels. Principle: **let ort-sys
fetch+link the prebuilt per-target, don't build onnxruntime yourself** — exactly
what molvi already does.

## D. Cross-platform hotkey + text-injection tools

**`espanso/espanso` (14,239★)** — text expander. Splits cleanly into
`espanso-detect` (hotkey) / `espanso-inject` (keystroke) / `espanso-clipboard`.
- Wayland hotkey: **evdev backend, source comment `"Hotkeys don't work under the
  EVDEV backend yet (Wayland)"`** — confirms NO solved global-hotkey on Wayland.
- Wayland inject: uinput (`/dev/uinput`, `input` group); notable
  `KeyboardStateProvider` "wait for key releases when injected string contains a
  currently-pressed key" — **exactly molvi's PTT-modifier-still-held problem.**
- Wayland clipboard: shells out to `wl-copy`/`wl-paste` (compositor-agnostic).
  → **Drove Task 12 Step 4 (wl-clipboard primary).**

**`cushycush/wdotool` (30★, lib `wdotool-core`)** — "xdotool for Wayland", maps
the whole injection landscape: GNOME/KDE = libei (RemoteDesktop portal + reis,
caches `restore_token`); Sway/Hyprland/river = wlr-protocols
(`zwp_virtual_keyboard_v1`); else = uinput. **`--clearmodifiers`** = blast-release
all modifiers before inject (can't read modifier state on Wayland). → **Drove
Task 12 Step 4 (blast-release modifiers).** MIT/Apache — candidate to vendor for
molvi's Linux paste (low bus-factor risk).

**`enigo-rs/enigo` (1,760★, v0.6.1)** — molvi's keystroke crate. Wayland gotchas
that HIT molvi: **#453** (libei panics inside a Tokio runtime — molvi's finalize
runs there; must `std::thread::spawn` paste off the runtime), **persist Enigo
struct** (libei works once per instance — molvi recreates per-call), **#336**
(Chromium/Electron under Wayland reject enigo input). → All reinforce: **Wayland
paste via wl-clipboard, NOT enigo libei.**

**`tauri-apps/global-hotkey` (259★)** — README "Linux (X11 Only)"; issue #28
(Wayland) open; maintainer: route via `ashpd`, no capacity. → **Do NOT wait for
it**; compositor-keybinding is molvi's path.

## Re-verification commands (run from any shell with `gh`)

```bash
# macOS refs
gh api repos/cjpais/Handy/contents/src-tauri/src/input.rs -H "Accept: application/vnd.github.raw"
gh api repos/cjpais/Handy/contents/src-tauri/src/overlay.rs -H "Accept: application/vnd.github.raw"
gh api repos/cjpais/Handy/contents/src-tauri/Cargo.toml -H "Accept: application/vnd.github.raw"
gh api repos/Beingpax/VoiceInk/contents/VoiceInk/Paste/CursorPaster.swift -H "Accept: application/vnd.github.raw"
# Linux refs
gh api repos/espanso/espanso/contents/espanso-inject/src/lib.rs -H "Accept: application/vnd.github.raw"
gh api repos/espanso/espanso/contents/espanso-clipboard/src/wayland/fallback/mod.rs -H "Accept: application/vnd.github.raw"
gh issue view 28 --repo tauri-apps/global-hotkey --json title,state,body
gh issue view 453 --repo enigo-rs/enigo --json title,state,body
# Engine refs
gh api repos/k2-fsa/sherpa-onnx/contents/sherpa-onnx/csrc/offline-recognizer-impl.cc -H "Accept: application/vnd.github.raw"
gh api repos/k2-fsa/sherpa-onnx/contents/.github/workflows/test-rust-package.yaml -H "Accept: application/vnd.github.raw"
```

## Summary of plan changes driven by this research

| # | Change | Source |
|---|---|---|
| 1 | macOS paste key `Key::Unicode('v')` → **`Key::Other(9)`** (Task 6) | Handy `input.rs` + VoiceInk `CursorPaster.swift` |
| 2 | Wayland decision: portal → **compositor-keybinding + `molvi record toggle`** (Task 11) | ashpd#213 (whisper-overlay); voxtype/whisrs/hyprwhspr convergence |
| 3 | Wayland paste: enigo libei → **wl-clipboard + blast-release modifiers** (Task 12 Step 4) | espanso clipboard + wdotool `--clearmodifiers`; enigo#453/#336 |
| 4 | Add **macOS Secure Input detection** (Task 9 Step 4) | Handy `secure_input.rs` |
| 5 | Add **tauri-runtime fork risk** verification (Task 9 Step 5) | Handy `[patch]` of tauri-runtime/wry/utils |
| 6 | Wayland foreground-app: "impossible" → **per-compositor IPC** (Task 12 Step 5) | whisrs `src/window/*.rs` |
| 7 | (Future) VoiceInk clipboard-restore-with-session-marker; Handy `hold_ms` + multi-method paste; `emit_to` overlay cache | VoiceInk / Handy |

**Spec corrections needed (next session, not this plan):** the design spec
`2026-08-07-molvi-multiplatform-port-design.md` §Architecture still says macOS
paste = `Key::Unicode('v')` + `Key::Command`, and §"Wayland scoping — OPEN"
leans portal. Both are superseded by this research — update the spec to match
this plan (Key::Other(9); compositor-keybinding; wl-clipboard) when the spec is
next revised. AGENTS.md "Hotkey" section also names `Key::Command` (the variant
is `Key::Meta`); fix on next edit.
