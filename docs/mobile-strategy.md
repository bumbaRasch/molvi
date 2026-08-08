# molvi mobile strategy (future, separate product)

> Status: **documented strategic option (Phase-5+), NOT a current build target.**
> Records the mobile findings so the decision isn't re-researched later. molvi's
> active track is the **desktop family** (Windows → macOS → Linux); see
> `platform-portability.md` and `next-session-handoff.md`.

## Decision

**Mobile is a SEPARATE PRODUCT, not a port of molvi.** If/when greenlit, the
realistic product is **"molvi Keyboard for Android"** — a native Kotlin
`InputMethodService` (system keyboard) with Vosk small models — NOT a Tauri
port of the desktop app. Revisit **only after the desktop family proves a real
privacy-OS user base.** Skip iOS until Android ships.

## Why mobile ≠ a molvi port (technical facts — do not re-litigate)

These are architectural, not maturity gaps. They do not change with more Tauri
versions:

1. **Tauri 2 mobile cannot be an Android IME.** An Android keyboard is an
   `InputMethodService` (a `Service` with its own view hierarchy, totally
   different lifecycle from an Activity). Tauri mobile hosts an Activity with a
   WebView. There is no plugin path that turns a Tauri app into the system
   keyboard. This is structural.
2. **molvi's engines don't fit.** GigaAM and Nemotron are ~2.6 GB each (ort/ONNX)
   — non-starters on phones. The mobile-viable engine is **Vosk small (~50 MB,
   ~300 MB runtime RAM)**, a *different* engine than molvi's current two.
3. **No global hotkey on mobile.** Desktop PTT (Win32 `RegisterHotKey` /
   global-shortcut plugin) has no phone analog. PTT becomes either (a) a mic key
   *inside the IME* (the good path), or (b) a floating overlay button
   (`SYSTEM_ALERT_WINDOW` — janky, Android 14+ restricted). The IME path is the
   real product shape.
4. **Reuse is ~the text layer only.** molvi's pure-Rust post-processing /
   dictionary / snippet transforms could be shared (via JNI). The ASR engines,
   the Tauri shell, the global-hotkey PTT, the overlay/focus model — none of that
   ports.

## The wedge (why it could matter — OSS lens)

molvi is **open source**. The mobile local-dictation landscape's incumbent is:

- **FUTO Keyboard / FUTO Voice Input** (`futo-org`) — the established "open-ish"
  answer, **but its license is the "FUTO Source-First License" — source-available,
  NOT OSI-open** (restrictions on commercial use). Rossmann-promoted.
- **Gboard / Samsung Keyboard** — system defaults, **ship voice to Google/Samsung
  servers on every non-Pixel/non-Samsung device** (verified via teardown + the
  "airplane-mode test"). This is the concrete privacy leak that drives demand.
- **Yaps** — polished but **closed + paid**.

**The clean whitespace: a genuinely OSI-free (MIT/Apache), Vosk-powered, RU-first,
system-wide Android keyboard.** FUTO structurally cannot occupy this position
because of its license. That is molvi's strongest single OSS differentiation in
the whole mobile landscape — *if* the desktop family first proves the
privacy-OS user base that makes a second product worth building.

## Demand (evidence)

- **Privacy gap is real and verified:** Gboard leaks voice to Google servers on
  all non-Pixel phones. Samsung does the same.
- **Paying/committed verticals:** healthcare (HIPAA — cloud voice is a
  dealbreaker), legal (privilege), journalists (confidential sources), regulated
  industries. Plus functional demand: travel, offline, no-signal, accessibility
  (ADHD/dyslexia/RSI).
- **Counter-signal:** mainstream users tolerate free cloud ("good enough"). The
  motivated audience is the privacy/compliance niche, not the general public.

## Cautionary precedent

**`alphacep/vosk-android-service`** — Alpha Cephei's own Vosk Android service —
is **dead, last commit 2023-03.** Someone saw this exact idea and couldn't
sustain it. molvi-on-mobile must clear a higher bar than "Vosk on Android
exists" (it does, as a demo) — it has to be a *finished keyboard*.

## iOS — lower priority

- **Apple's built-in dictation** (iOS 26) runs **on-device** on Apple Silicon and
  is **free + improving** — shrinking the privacy-gap argument.
- **Custom keyboard extensions** are sandboxed and **Apple-hostile to mic
  access** (`requestsOpenAccess` friction, review gates).
- No IME-routing mechanism like Android's `RecognitionService`.

→ Skip iOS until Android ships AND the FUTO-license wedge is validated in the
wild.

## Gating + product shape (if greenlit)

**Gate:** revisit mobile ONLY after the desktop family (Win/Mac/Linux) proves a
real privacy-OS user base. Without that, you'd be building a free Android
keyboard to fight FUTO for non-paying users.

**Product if gated open:**
1. **Android-native first** (10× the demand + the IME mechanism makes it
   system-wide), Vosk-only, share molvi's post-proc/dictionary/snippet text layer
   via JNI.
2. Lead with **RU + EN + top locales** (molvi's RU-native spine is the differentiator).
3. Position explicitly as **the OSI-free alternative to FUTO** (license purity is
   the wedge).
4. Skip iOS until Android ships.

## Sources

- `futo-org/voice-input` + `futo-org/keyboard` (source-available, not OSI)
- `alphacep/vosk-android-service` (dead 2023), `alphacep/vosk-android-demo` (lib)
- Gboard cloud-leak: FUTO teardown + Louis Rossmann coverage + r/fossdroid threads
- Mobile landscape research (internal, 2026-08): ~30 offline-dictation repos,
  almost all desktop; mobile local-dictation OSS products = FUTO + a dead Vosk
  service.
