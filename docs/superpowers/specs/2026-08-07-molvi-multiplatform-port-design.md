# molvi multi-platform port — design spec (Track A)

> Date: 2026-08-07 · Status: **design (brainstorm-approved), pre-implementation**
> Author: brainstorm session 2026-08-07 (OSS strategic lens)
> Supersedes the "documented option" framing in `docs/platform-portability.md`
> (which becomes a historical seed). Companion: `docs/mobile-strategy.md`
> (mobile = separate product, Phase-5+).

## TL;DR

molvi today ships **Windows 11 x64 only** (Tauri 2 webview shell + local CPU
ASR). This spec turns it into a **free, local, private, fast dictation family
on the desktop OSes** — Windows (done) → macOS → Linux — sharing the existing
GigaAM/Nemotron engines (real port, not a rewrite). It is an **open-source**
effort (license = `MIT OR Apache-2.0`). Servers / Raspberry Pi / headless are
**excluded** (no product sense). Mobile is a **separate product** (gated,
documented separately).

**Approach:** inline `#[cfg(target_os = "...")]` per feature module +
`[target.'cfg(windows)'.dependencies]` Cargo.toml gate — **no `mod platform`
abstraction** (doc-verified as premature for ~6 single-use functions).
**Sequence:** 3 de-risking spikes → **macOS-first** (smoother, CoreML speed
path, no Wayland blocker) → **Linux/X11** → **Wayland gated** (global-hotkey
is X11-only upstream).

## Strategic frame (why this, why now)

molvi is **open source** (success metric = reach / freedom / community, not
revenue). The desktop family is the real port: shared GigaAM/Nemotron engines,
shared Tauri shell, shared PTT. Research findings that shaped the scope:

- **Servers / Pi / headless: excluded.** Dictation = paste into the focused app
  you're looking at. A headless box has no microphone, no screen, no foreground
  window → nowhere for text to go. Zero evidenced demand (GH + Reddit + forums).
  The only working "remote" model (medical PowerMic Mobile) keeps the ASR+paste
  on the desktop and uses the phone as a thin mic. → "Server brain" is an
  anti-pattern for molvi's model.
- **Mobile: separate product.** Tauri mobile cannot be an Android IME; GigaAM/
  Nemotron (2.6 GB) don't fit phones → Vosk-only; no global hotkey on mobile.
  This is a second product, gated on the desktop family proving a privacy-OS
  user base. See `docs/mobile-strategy.md`.
- **Linux-desktop:** real, loud demand, OSS heartland; **but saturated** (Handy
  28.9k★ on molvi's *exact* stack, same engine author; nerd-dictation 1.9k★;
  hyprwhspr 1.1k★/12mo; murmure 990★). In OSS this is coexistence, not a
  zero-sum fight. molvi's wedge = GigaAM-RU-native + offline command grammar.
- **macOS:** crowded (Superwhisper paid $249, 30+ OSS clones), **but the speed
  moat has a path via CoreML** (`ort/coreml` on Apple Silicon) and the cleanest
  single wedge = free RU-native local dictation vs paid Superwhisper's weak RU.

## Decisions (locked in brainstorm)

| # | Decision | Rationale (fact-grounded) |
|---|---|---|
| D1 | **License = `MIT OR Apache-2.0`** (dual) | Rust-ecosystem convention (std, tokio, serde); MIT simplicity + Apache §3 patent grant (relevant for ML/ASR patent thickets); compatible with Vosk (Apache) + Handy (MIT); strongest FUTO-contrast wedge. molvi is currently unlicensed → this is the first deliverable. |
| D2 | **Inline `#[cfg]`, no `mod platform`** | Doc-verified (Rust Reference + Cargo Book; cpal/arboard/enigo/std centralize *only* for coherent subsystems). molvi's 6 platform fns are each used in exactly ONE feature module → no dedup benefit; a central `mod platform` is cargo-cult. |
| D3 | **Sequence: spikes → macOS (Apple Silicon) → Linux (Wayland scoping OPEN)** | macOS is smoother (no Wayland fragmentation; global-hotkey Carbon ✓; enigo CGEvent ✓; CoreML speed path). Linux Wayland is now the default session on current distros (Aug-2026 data) and the PTT hotkey has no upstream Wayland backend — so Linux scoping is OPEN (lean: Wayland-in-v1, gated on a Wayland-hotkey spike). |
| D4 | **Vosk deferred** | Port's job = get existing 2 engines cross-platform. Vosk is additive (mobile/Pi engine), separate task when greenlit. |
| D5 | **Full feature parity target** (profiles work on macOS + X11) | `foreground_exe()` is implementable on macOS (NSWorkspace) + X11 (`_NET_WM_PID`); only **Wayland degrades** (returns `None` → profiles no-op). |
| D6 | **OSS umbrella: "molvi — free local private dictation everywhere"** | Desktop family now; mobile keyboard later (separate product). |

## Platform-coupling inventory (what actually changes)

The ASR brain (`engine.rs`, `engine_adapter.rs`, `postproc.rs`, `commands.rs`,
`coordinator.rs`, `pipeline.rs`, `dictionary.rs`, `snippets.rs`, `history.rs`,
`resample.rs`, `log.rs`, `tray*.rs`, `settings.rs`) is **platform-neutral** —
zero Win32. The entire Win32 surface is **5 files / 6 boundaries**:

| Boundary | Today (Win32) | macOS | Linux X11 | Linux Wayland |
|---|---|---|---|---|
| `foreground_exe()` — `profiles.rs` | HWND→PID→`QueryFullProcessImageNameW` | `NSWorkspace.frontmostApplication` | `_NET_WM_PID` | **`None`** (profiles degrade) |
| `app_data_dir()` — `paths.rs` | `%APPDATA%\com.molvi.app` | `~/Library/Application Support/com.molvi.app` | `$XDG_CONFIG_HOME/com.molvi.app` | same as X11 |
| `capture_paste_target()` / `ensure_focus()` — `paste.rs` | HWND save/restore (focus-guard §6.6) | *(spike #3 verifies semantics)* | *(spike #3)* | *(experimental)* |
| `has_disk_space()` — `model_store.rs` | `GetDiskFreeSpaceExW` (already cfg-gated) | `statfs` | `statvfs` | same as X11 |
| `play_sound_file()` — `audio.rs` | `PlaySoundW` | portable wav player | portable | portable |
| `ort_affinity` (P-core pin) — `ort_affinity.rs` | Win32 topology (fail-open) | no-op (Apple Silicon scheduler-managed) | `sched_setaffinity` (optional) | n/a |

Everything else — hotkey plugin, tray, clipboard (arboard), cpal capture, rubato
resample, updater, single-instance — is **already cross-platform**.

## Doc-verified crate matrix (August 2026, via ctx7 + docs.rs/crates.io)

All molvi-pinned crates are at the **Aug-2026 latest** — nothing to bump.

| Crate | Ver | Windows | macOS (Apple-Si) | macOS (Intel) | Linux X11 | Linux Wayland |
|---|---|---|---|---|---|---|
| cpal | 0.18.1 | ✓ WASAPI | ✓ CoreAudio | ✓ | ✓ ALSA | ~ (ALSA under most compositors) |
| enigo | 0.6.1 | ✓ | ✓ CGEvent | ✓ | ✓ (x11rb) | **~ experimental (libei/ashpd)** |
| global-hotkey (via plugin 2.3.2) | 0.8.0 | ✓ | ✓ Carbon | ✓ | ✓ x11rb | **✗ X11-only, no upstream** |
| ort | 2.0.0-rc.13 | ✓ | **✓ CoreML** (`ort/coreml`) | **✗ no prebuilt** | ✓ x64 (gnu; no musl) | n/a |
| transcribe-rs (GigaAM) | 0.3.11 | ✓ | ✓ (`ort-coreml`) | ✓ | ✓ | n/a |
| parakeet-rs (Nemotron) | 0.3.7 | ✓ | ✓ (`coreml`) | ✓ | ✓ | n/a |
| arboard | 3.6.1 | ✓ | ✓ objc2 | ✓ | ✓ X11 | ✓ (wl-clipboard; **opt-in** `wayland-data-control`) |

> **Intel-Mac column sharpened (verified):** ort-sys dist.tsv has **no `x86_64-apple-darwin`
> row at all** — not "CPU-only prebuilt", but *no prebuilt of any kind*. Under the default
> `download-binaries`, `cargo build` for Intel Mac **errors** ("no prebuilt binaries available
> … compile from source"). Intel Mac needs `load-dynamic` (user-supplied libonnxruntime,
> CPU-only, no CoreML) or a from-source build. Apple Silicon is first-class (`aarch64-apple-darwin
> +coreml` prebuilt). → Intel Mac is effectively **unsupported** without extra packaging work.
| Tauri 2 | 2.11.5 | ✓ NSIS/MSI | ✓ `.app`/`.dmg` | ✓ | ✓ AppImage/deb/rpm/Flatpak | ✓ (bundles run) |

**AGENTS.md correction to record:** "enigo does NOT work on Wayland" is
**stale** — enigo 0.6.x added experimental `libei_smol`/`libei_tokio` (reis +
ashpd portal) and `wayland` (wayland-client) features. Still flagged "stability
concerns" upstream, but no longer impossible.

## The 3 blockers

1. **Global hotkey (PTT) on Wayland — HARD, no upstream; AND Wayland is now the
   default/only session on current distros (verified Aug 2026).** `global-hotkey`
   0.8.0 docs.rs: "Platforms: Windows, macOS, Linux (X11 Only)" — no Wayland
   backend; `tauri-plugin-global-shortcut` 2.3.2 → `global-hotkey ^0.8` inherits
   it. **Wayland reality:** KDE Plasma 6.8 *removed the X11 session* from the
   login screen; Fedora 40+ KDE ships Wayland-only; Kubuntu Wayland-default;
   GNOME Wayland-default (X11 deprecated). → An **X11-only Linux v1 would not
   run on the default session of current Fedora/Ubuntu/KDE/GNOME.** X11 sessions
   are still installable on most distros (Xorg packaged), but that is a
   **shrinking minority.** This forces a scoping decision (see "Wayland
   scoping — OPEN" below); the earlier "X11-only v1, Wayland gated" framing is
   no longer defensible as a complete Linux release. Mitigations for the hotkey
   itself: custom XDG-portal `GlobalShortcuts` via `ashpd`, or `evdev`+`uinput`
   (root/input group) — both heavy.
2. **`Key::Other(0x56)` = Windows VK — per-platform re-keying.** molvi pastes
   with `Key::Other(0x56)` (VK_V) and command-mode chords with
   `Key::Other(<vk>)`. That `u32` is a Windows virtual-key code; on macOS it's a
   CGEvent keycode, on X11 a keysym. `Key::Other(0x56)` will NOT type 'V' on
   Mac/Linux. **Every paste/chord site must be re-keyed per-platform** (e.g.
   `Key::Unicode('v')` for the paste char, or platform key tables).
3. **ort on Intel Mac — BUILD FAILS by default (MEDIUM).** Verified: ort-sys
   dist.tsv has **no `x86_64-apple-darwin` row** → under default
   `download-binaries`, `cargo build` for Intel Mac **errors out**, not "runs
   slower". Paths: `load-dynamic` (bundle a manual CPU-only libonnxruntime, no
   CoreML EP) or compile ort from source. Apple Silicon is first-class (CoreML
   prebuilt). **→ macOS v1 = Apple-Silicon-only; Intel Mac = unsupported (or
   best-effort `load-dynamic` follow-up).** Intel-Mac share is shrinking.

## The 3 de-risking spikes (GATES before product work)

Each is cheap (~1 day) and answers "is this even possible" before any
architecture bet. Run **before** the full port.

1. **Linux/X11 engine spike.** `cargo build` `transcribe-rs` + `parakeet-rs` on
   Linux x64 (after minimal Cargo.toml target-dep gate, see Architecture).
   Measure RTF. (Low risk — ort CPU on Linux is proven — but verify.)
2. **macOS Apple-Silicon engine spike.** `cargo build` both crates with the
   `coreml`/`ort-coreml` features on `aarch64-apple-darwin`. Measure RTF ort-CPU
   **vs** CoreML-EP. **Main risk:** does CoreML actually *accept* the GigaAM/
   Nemotron ONNX graphs at accelerated speed? If CoreML rejects them or isn't
   faster, fall back to ort-CPU (still works, just slower — the blaze moat may
   narrow on Mac but doesn't break).
3. **Paste focus-guard semantics spike.** Verify via ctx7/docs (enigo, cpal,
   Tauri windowing) how the §6.6 invariant (never misdeliver paste into a
   stranger's window) maps to macOS + X11. Does showing the overlay steal focus?
   What is the portable shape of `capture_paste_target()` / `ensure_focus()`?

## Architecture — inline cfg pattern (D2)

**Step 0 (prerequisite for spikes): minimal Cargo.toml gate** so it compiles on
non-Windows at all:

```toml
# was unconditional: windows = { ... }
[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = ["Win32_Foundation", /* … */] }
```

**Step 1 (during the port): inline `#[cfg]` per feature module**, following the
existing `model_store.rs:214` idiom. Each boundary stays in its feature module:

```rust
// profiles.rs — foreground_exe() stays HERE, in profiles context
pub fn foreground_exe() -> Option<String> {
    #[cfg(target_os = "windows")]
    { /* Win32 HWND→PID→QueryFullProcessImageNameW→basename */ }
    #[cfg(target_os = "macos")]
    { /* NSWorkspace.frontmostApplication.executableURL basename */ }
    #[cfg(target_os = "linux")]
    { /* X11 _NET_WM_PID → /proc/<pid>/exe; Wayland session → None */ }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { None }
}
```

Paste/chord re-keying (blocker #2) follows the same pattern: a per-platform
`fn paste_key() -> enigo::Key` helper in `paste.rs` + platform key tables in
`commands.rs`. **No backward-compat constraint** → refactor the existing Windows
call sites to route through the helper too (cleaner than branching at every
site). Per upstream enigo docs: `Key::Unicode('v')` + `Key::Control` on
macOS/Linux; Windows keeps `Key::Other(0x56)` (SendInput/Unicode robustness —
verified).

**What is NOT abstracted:** no `mod platform`, no trait objects, no dyn dispatch.
Platform is known at compile time; zero cost; matches the codebase's existing
single cfg-gate.

## Per-platform specifics

### macOS (first target, gated on spike #2)
- Engines: GigaAM + Nemotron via ort, **CoreML EP** on Apple Silicon
  (`ort/coreml` feature + `transcribe-rs/ort-coreml` + `parakeet-rs/coreml`).
  **Apple-Silicon only** — Intel Mac has no ort prebuilt (build fails by
  default); `load-dynamic`/source-build Intel support is a best-effort
  follow-up, not v1.
- Hotkey: `tauri-plugin-global-shortcut` → Carbon. Works.
- Paste: enigo CGEvent. Per-platform `paste_key()` helper — on macOS the
  documented enigo idiom is `Key::Unicode('v')` + `Key::Control`; **no backward
  compat needed → refactor the Windows call sites too** (see Architecture).
  Verify focus-guard (spike #3).
- Profiles: `foreground_exe()` via `NSWorkspace.frontmostApplication`. Works →
  **profiles fully functional on macOS**.
- Paths: `~/Library/Application Support/com.molvi.app`.
- Packaging: `.app`/`.dmg` + **notarization + code signing** ($99/yr Apple Dev,
  Gatekeeper). CI work (separate from code).
- `ort_affinity`: no-op (Apple Silicon P/E is scheduler-managed).

### Linux (Wayland-first — X11 comes free; see Wayland scoping)
- Engines: GigaAM + Nemotron via ort-CPU. Works.
- Hotkey: `tauri-plugin-global-shortcut` → x11rb (**X11 only**). On Wayland
  there is no upstream global hotkey (verified, blocker #1) → requires the
  portal/evdev path. **X11 works as a free bonus** wherever the hotkey plugin
  runs; Wayland is the hard part and now the default session (blocker #1).
- Paste: enigo x11rb (X11) / experimental libei (Wayland). Per-platform key via
  `paste_key()` (`Key::Unicode('v')` + Control is the documented idiom).
- Profiles: `foreground_exe()` via X11 `_NET_WM_PID` → `/proc/<pid>/exe`.
  **Functional on X11. On Wayland: structurally impossible** → `None` →
  profiles degrade silently (all other features still work).
- Paths: `$XDG_CONFIG_HOME/com.molvi.app` (default `~/.config/com.molvi.app`).
- `redact_appdata` (paths.rs) must also redact `$HOME` prefix (PII-adjacent),
  not just `%APPDATA%`.
- Packaging: AppImage + deb + rpm (Flatpak optional).
- `ort_affinity`: optional `sched_setaffinity` (P-core equiv), else no-op.

### Wayland scoping — OPEN decision (forced by Aug-2026 data)
The data (blocker #1) shows current Fedora/Ubuntu/KDE Plasma 6.8/GNOME are
**Wayland-default or Wayland-only** — an X11-only Linux v1 would not run on the
default session of today's distros. X11 sessions remain installable (Xorg
packaged) but are a shrinking minority. **Three options** (decision deferred to
planning, after the macOS port proves the engine/abstraction layer):

1. **Wayland-in-v1 (heaviest, most correct):** build the `ashpd` GlobalShortcuts
   portal hotkey + enigo libei paste as part of the Linux port. Reaches today's
   default sessions. Cost: portal-permission UX, compositor-support matrix
   (KDE/Hyprland/Sway yes; GNOME Mutter gaps), experimental libei stability.
2. **X11-now, Wayland-v1.5:** ship X11 first (works for X11-session users),
   Wayland follow-up. Lower risk now, but serves a shrinking segment on current
   distros.
3. **Defer all Linux until macOS ships + Wayland solution is ready:** macOS is
   the smoother port (no Wayland wall); tackle Linux once the portal/libei story
   matures upstream.

**Given "no backward compat needed" + Wayland dominance, the lean is (1)
Wayland-in-v1** — but it is gated on macOS shipping first and a focused
Wayland-hotkey spike (portal vs evdev).

## Feature parity / degradation matrix

| Feature | Windows | macOS | Linux X11 | Wayland |
|---|---|---|---|---|
| PTT (global hotkey) | ✓ | ✓ | ✓ | **✗ upstream → scoping OPEN (blocker #1)** |
| GigaAM (RU, punctuated) | ✓ | ✓ (CoreML/CPU) | ✓ (CPU) | ✓ (once hotkey lands) |
| Nemotron (multilingual stream) | ✓ | ✓ | ✓ | ✓ |
| Paste + focus-guard | ✓ | ✓ (re-keyed) | ✓ (re-keyed) | ~ (experimental) |
| Per-app profiles | ✓ | ✓ (NSWorkspace) | ✓ (`_NET_WM_PID`) | **degrade (`None`)** |
| Command-mode chords | ✓ | ✓ (re-keyed) | ✓ (re-keyed) | ~ |
| Overlay, history, dictionary, snippets, i18n | ✓ | ✓ | ✓ | ✓ (overlay may steal focus — verify) |

## NFRs (load-bearing — must hold across the port)

**Constraint confirmed with maintainer (2026-08-07): backward compatibility is
NOT required — the main/Windows code may be refactored freely for cross-platform
clarity. The hard rule is PERFORMANCE, not code-preservation.**

- **Blaze (HARD — performance, not compat):** the default RU/PTT/Smart path may
  be refactored, but its **performance invariant** must hold on every shipping
  target, verified by measurement: **RTF ≤ 0.03** (controlled long-utterance);
  hot loop (capture→engine→finalize→paste) **allocation/lock/blocking-free**;
  Nemotron feeds ONLY at the **8960-sample boundary** (load-bearing — do NOT
  change). Spikes #1/#2 measure RTF per platform. CoreML EP on Apple Silicon is
  the path to *preserve/improve* the moat; a platform that can't hold ≤0.03 on
  the default path is a blaze regression to fix (or document as that platform's
  measured baseline — never a silent regression), not "ship and hope."
- **Privacy §10.1 (HARD RULE):** unchanged. Never log transcript/partials/
  post-proc/dict/history/snippet/command/prompt text at any level. The 6
  `log_privacy` substrates stay green; per-platform path redaction extends
  (`redact_appdata` → also redact `$HOME` on Unix).
- **Hot-path discipline (reframed):** the inference→post-proc→paste hot loop may
  be edited, but must contain **no runtime platform branches** — all platform
  dispatch is compile-time (`#[cfg]`, zero cost). The reframe (e.g. a
  `paste_key()` helper, a `foreground_exe()` dispatcher) is encouraged if it
  improves cross-platform clarity; the gate is *measured RTF + zero-alloc hot
  loop*, not "the diff is byte-identical to Windows."

## Out of scope (this spec)

- **Servers / Raspberry Pi / headless** — no product sense (no mic/screen/
  focused-app), zero demand.
- **Mobile (Android/iOS)** — separate product, gated; see `docs/mobile-strategy.md`.
- **Vosk** (third engine) — deferred; additive later.
- **Updater endpoints / signing keys** — release blocker regardless of platform;
  `tauri.conf.json` pubkey/endpoint still placeholder. Cross-platform *plugin*
  works; the *feed* is deployment work, separate from this port.
- **Wayland scoping is OPEN** (blocker #1 + Aug-2026 data: Wayland is now the
  default/only session on current distros). See "Wayland scoping — OPEN
  decision". Not a blanket "out of scope" — the lean is Wayland-in-v1.

## Success criteria

1. `cargo build` succeeds on `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`
   (Apple Silicon; CoreML). `x86_64-unknown-linux-gnu` once the Wayland scoping
   (below) is resolved. (`x86_64-apple-darwin` / Intel Mac is NOT a default-build
   target — no ort prebuilt; best-effort `load-dynamic` only.)
2. Spikes #1–#3 pass: engines build+run with measured RTF on Linux and macOS
   (CoreML on Apple Silicon); paste focus-guard semantics documented per platform.
3. **Blaze gate (performance, measured):** the default RU/PTT/Smart hot loop
   holds **RTF ≤ 0.03** (controlled long-utterance) + **zero-alloc/lock/blocking**
   on each shipping target, verified by measurement and recorded in the AGENTS.md
   NFR row. The hot loop MAY be refactored (no backward-compat constraint) but
   must contain no runtime platform branches (compile-time `#[cfg]` only). If a
   target can't reach ≤0.03 in the controlled measurement, that is documented as
   that platform's measured baseline — never a silent regression.
4. Full molvi feature set runs on macOS (Apple Silicon) + Linux (per the Wayland
   scoping decision). Profiles work on macOS (NSWorkspace) + X11
   (`_NET_WM_PID`); degrade to `None` on Wayland (structurally impossible).
5. CI matrix (windows/macos/ubuntu runners) green: `cargo fmt` + `clippy
   --all-targets -D warnings` + `cargo test --lib` + `cargo test --test
   log_privacy` + `npx tsc --noEmit` + `npm run build`.
6. License files present (`LICENSE-MIT`, `LICENSE-APACHE`) + `license` field set.
7. Packaging: signed+notarized `.dmg` (macOS), AppImage+deb+rpm (Linux).

## Open follow-ups (not blocking the spec)

- **Updater feed + signing** (deployment) — needed for any cross-platform release.
- **Wayland scoping decision** — see "Wayland scoping — OPEN decision"; resolve
  (portal vs evdev vs defer) after macOS ships. A focused Wayland-hotkey spike
  is the gate.
- **CoreML graph acceptance** — spike #2 may reveal CoreML rejects parts of the
  GigaAM/Nemotron graphs; fallback = ort-CPU on Mac (slower but functional).
- **Controlled blaze re-measurement** — fill the AGENTS.md NFR row per platform
  after spikes.

## Re-verification corrections (2026-08-07, vs live docs)

Re-checked every crate claim against ctx7 + docs.rs + crates.io + local cargo
registry source. All versions current (nothing bumped). Corrections applied
inline above; standing notes:

- **transcribe-rs ort pin (latent landmine):** `transcribe-rs 0.3.11` pins
  `ort = "=2.0.0-rc.12"` (exact), while `parakeet-rs 0.3.7` requires `rc.13` —
  mutually unsatisfiable under cargo's resolver. molvi's `Cargo.lock` resolves to
  rc.13, implying a `[patch]`/override is in play. **Confirm the override exists
  before any fresh `Cargo.lock` re-resolution** (engine-spike prereq — a clean
  resolve would fail).
- **enigo paste idiom:** the official enigo crate-level paste example is
  `Key::Control` + `Key::Unicode('v')`, which contradicts AGENTS.md's stated
  reason for `Key::Other(0x56)` ("Unicode doesn't combine with held Ctrl").
  molvi's Windows `Key::Other(0x56)` stays defensible (Unicode-via-SendInput is
  rejected as Ctrl+V by some apps), but the **cross-platform re-key uses
  `Key::Unicode('v')`** per upstream docs. One-line AGENTS.md accuracy fix.
- **cpal Linux:** optional **`pipewire`** backend now exists (in addition to
  jack/pulseaudio). molvi's capture path is backend-agnostic (`default_host`).
- **arboard Wayland:** clipboard is **opt-in** (`wayland-data-control` feature,
  non-default) — must be enabled explicitly for Wayland clipboard.
- **ort ctx7 id drift:** AGENTS.md's `/pyke.io/ort` (dotted) now 404s; live id
  is **`/pykeio/ort`** (High reputation, 232 snippets). AGENTS.md fix.
- **enigo Wayland note (AGENTS.md):** "enigo does NOT work on Wayland" is
  **stale** — 0.6.1 ships opt-in `wayland` / `libei_smol` / `libei_tokio`
  features (experimental, stability-flagged).

## Sources

- `docs/platform-portability.md` (historical seed — Vosk-as-portability-engine,
  capability-filtered engine matrix)
- `docs/next-session-handoff.md` (Track A/B/C menu)
- `docs/mobile-strategy.md` (mobile = separate product)
- Platform-coupling inventory (this session, 5 files / 6 boundaries)
- Crate docs (Aug 2026): ctx7 `/pykeio/ort`, `/enigo-rs/enigo`, `/websites/
  v2_tauri_app`; docs.rs global-hotkey, cpal, arboard; crates.io transcribe-rs,
  parakeet-rs
- License comparison: choosealicense.com (Apache-2.0 §3 patent grant vs MIT)
- Market research (this session): Linux-desktop saturation (Handy 28.9k★),
  macOS landscape (Superwhisper $249, CoreML competitors), mobile (FUTO license
  wedge), headless/Pi (zero demand)
