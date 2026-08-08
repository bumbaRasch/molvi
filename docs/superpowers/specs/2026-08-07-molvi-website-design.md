# molvi marketing website — design spec

- **Date:** 2026-08-07
- **Status:** Draft → pending user review
- **Author:** brainstorming session
- **Sibling repo:** `../molvi-website` (new; not part of the app repo)
- **Source of truth for product copy:** [`README.md`](../../README.md) of the app repo. The website is a localized, visual extension of that README. Where this spec and the README disagree on product facts, the README wins.

---

## 1. Goals & non-goals

### Goals
1. A single, fast, localized landing page that converts privacy-conscious visitors into downloads.
2. Lead with molvi's defensible wedges: **on-device/private-by-design**, **Russian-native engine (GigaAM)**, **36-language UI**, **Windows·macOS·Linux parity**, **open source**.
3. Mirror the README's tone: honest, specific, no fake social proof, no unverifiable superlatives.
4. Ship in **6 locales** (EN canonical + RU reviewed + ES/DE/FR/ZH), structured for community PRs to add the remaining 30.
5. Excellent Core Web Vitals and SEO out of the box (LCP ≤ 2.5 s, INP ≤ 200 ms, CLS ≤ 0.1).

### Non-goals (YAGNI — explicitly out of scope at launch)
- No blog, no full docs site (Starlight), no CMS, no server-side analytics.
- No testimonials / customer logos / star-count flex (none exist yet; fake ones destroy the privacy-trust story).
- No enterprise/commercial/pricing pages — molvi is free/OSS.
- No cloud sync/account/history-sync story — pure-local is the product.
- No mobile/iOS/Android page — desktop only.
- No interactive web demo of the engine — models are ~2.6 GB; a silent GIF of the overlay→paste flow is the hero visual instead.

---

## 2. Stack & architecture

| Layer | Choice | Why |
|---|---|---|
| Framework | **Astro** (static output) | Same stack Tauri's own site uses. Zero JS by default; best-in-class i18n routing; SSG = tiny, fast, great SEO. For 6 locales this is less hand-rolled code than plain static HTML. |
| Styling | **Vanilla CSS** (no Tailwind, no UI lib) | Matches the app's no-framework philosophy; fewer deps; ponytail. CSS custom properties for theming. |
| Client JS | ~minimal vanilla TS | OS detection for the download button + theme toggle + mobile nav. Nothing else. Honors `prefers-reduced-motion`. |
| i18n | Astro i18n, **subdirectories** + `<link rel="alternate" hreflang>` | SEO consensus: subdirs inherit domain authority and outrank subdomains. |
| Deploy | **GitHub Pages** via GitHub Actions | Free, OSS-aligned, same GitHub org as the release pipeline. Default URL `https://bumbaRasch.github.io/molvi-website/`; custom domain (e.g. `molvi.app`) later by adding a `CNAME`. |
| Repo location | `../molvi-website` (sibling to the app repo) | Per request; keeps the app repo lean. |
| Skills used during build | `astro`, `seo` (Addy Osmani/web-quality), `ui-ux-pro-max` | Installed in the app repo's `.agents/skills/`; loaded into the building agent (not shipped in the site). Guide the build; not a runtime dependency. |

**Project layout (target):**
```
molvi-website/
├── astro.config.mjs           # i18n locales + site URL + base path
├── public/
│   ├── favicon.svg, og-default.png, robots.txt, CNAME (later)
│   └── img/                   # icon derivatives + screenshots (copied from the app repo's docs/img/, see §2.1)
├── src/
│   ├── components/            # Nav.astro, Hero.astro, FeatureRow.astro, Bento.astro, Privacy.astro, Faq.astro, Download.astro, Footer.astro, LocaleSwitcher.astro, ThemeToggle.astro
│   ├── layouts/Base.astro     # <html lang/dir>, <head> meta/OG/hreflang/JSON-LD
│   ├── styles/global.css      # tokens, dark/light/system theme
│   ├── scripts/               # os-detect.ts, theme.ts, nav.ts (tiny)
│   ├── i18n/                  # locales/{en,ru,es,de,fr,zh}.ts, ui.ts (t()), config.ts
│   └── pages/
│       ├── index.astro        # EN canonical
│       ├── ru/index.astro
│       ├── es/index.astro
│       ├── de/index.astro
│       ├── fr/index.astro
│       └── zh/index.astro
├── package.json
└── .github/workflows/deploy.yml   # build → gh-pages
```

**Routing rule:** every page exists in all 6 locales; EN at `/`, others at `/<code>/`. Each page emits hreflang alternates for all 6 (including `x-default` → EN).

### 2.1 Screenshot assets (available now)

Three real captures already live in the **app repo** at `docs/img/` — copy them into the website's `public/img/` at build time and produce pre-resized AVIF/WebP variants (the PNGs are high-res; `analyze_image` timed out on them, so they need compression for web LCP):

| File | Shows | Used in |
|---|---|---|
| `overlay.png` | The dictation overlay mid-session (bubble + live caption) | **Hero** static fallback (and a reference frame for the GIF); also the "How it works" step 2 |
| `settings.png` | The settings window (sidebar + a section) | **Feature deep-dives / capabilities** — the "more than dictation" row + bento |
| `onboarding.png` | The first-run dialog (hotkey + model download) | **How it works** step 1 / "Quick start" |

The only missing visual is the **~3 s silent overlay→paste GIF** for the hero — see open follow-ups.

### 2.2 Brand / icon assets

All brand/icon assets are sourced from the **app repo** at `src-tauri/icons/` — **`icon.png` is the master** (the blue-gradient "shushing" silhouette; see §6). The website repo's `public/` consumes it as follows (generate the derived sizes from the master at build time — do not hand-edit):

| Website asset | Source | Notes |
|---|---|---|
| `favicon.ico` + `favicon.svg` | `src-tauri/icons/icon.ico` / derive svg | Both formats for broad browser support. |
| `apple-touch-icon.png` | `src-tauri/icons/icon.png` | 180×180, no transparency (iOS rounds it). |
| `og-default.png` | `src-tauri/icons/icon.png` | 1200×630 social card = icon on the blue gradient + the EN tagline. Used by §7 OG/Twitter tags. |
| Nav logo | `src-tauri/icons/icon.png` | Inline SVG derivation preferred (crisp at all DPRs). |
| Favicon variants (32/64/128) | `src-tauri/icons/32x32.png` etc. | Pre-sized sources already exist in the repo — reuse rather than re-resize. |

The repo already ships ready sizes (`32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`, `Square*Logo.png`, `StoreLogo.png`) — reuse those where exact, regenerate only the OG/social 1200×630.

---

## 3. Locales & i18n

- **Launch set (6):** `en` (canonical) · `ru` (fully reviewed — ties to the GigaAM/Russian story) · `es` · `de` · `fr` · `zh-CN`.
- **Translation pipeline:** EN authored first as the single source → RU reviewed by a native (highest quality bar) → ES/DE/FR/ZH translated + reviewed → remaining 30 app locales added later via community PRs (a `CONTRIBUTING` note + per-locale TODO markers).
- **UI strings only** are localized. Proper nouns (GigaAM, Nemotron, Tauri, ONNX, Rust, GitHub, Windows/macOS/Linux) stay verbatim in all locales. ASCII tokens (`{size}`, `{n}`) are kept verbatim, mirroring the app's i18n rules.
- **Locale switcher:** globe icon + current code in nav; persisted in `localStorage`; links to the same section anchor on the sibling locale page where possible.
- **RTL:** not required for the launch set (no ar/he). The CSS will use logical properties (`inset-inline-*`, `text-align: start`) from day one so adding ar/he later is a flip, not a rewrite — same convention as the app.
- **Honest floor:** no locale ships machine-translated and unreviewed. If a locale isn't ready at launch, it isn't linked — broken copy hurts the privacy-trust story more than a missing locale.

---

## 4. Landing page sections (in order)

1. **Sticky nav** — logo · anchor links (How it works · Features · Privacy · Download) · **GitHub ★ button** (links to app repo) · **locale switcher** · **theme toggle**. Becomes a hamburger on mobile.
2. **Hero** — outcome headline + privacy-led subhead + **OS-detected "Download for [OS]"** primary CTA + **"View on GitHub ★"** secondary + silent looping GIF (~3 s) of overlay→paste (**static fallback = `overlay.png`** under reduced-motion / before the GIF exists) + microcopy `Free · Open source · No account · ~11 MB installer`.
3. **Trust strip** — `Built with Tauri · Rust · GigaAM · Nemotron ASR · ONNX Runtime` badges + the zero-trick: `0 bytes sent to the cloud · 0 accounts · 0 telemetry`.
4. **How it works** (3 sequential steps — **not** bento) — Hold the hotkey → Speak → Text lands in any app. Mirrors README "How it works".
5. **Feature deep-dives** (2–3 alternating image/text rows):
   - Row A: **Private by design, not by promise** (the lead) — on-device, no network call, privacy test suite.
   - Row B: **Speaks your language** — 36-language UI (RTL), 40+ recognition locales; **GigaAM Russian-native** callout ("no other dictation tool runs a Russian-native engine").
   - Row C: **More than dictation** — command mode, per-app profiles, dictionary, snippets, Smart/Polished post-processing.
6. **Capabilities bento** (one section, parallel/same-register cells) — Snippets · Dictionary · Local history search · Custom hotkey · Overlay · Auto-updater. Anchor cell = the headline feature.
7. **Privacy section** (deep) — "Your audio never leaves your machine" + README's verifiable claims + **"Private by design, not by promise"** + **verify-it-yourself** (firewall test, network monitor, link to `tests/log_privacy.rs`).
8. **Languages** — coverage map: 36 UI · 40+ recognition; Russian-specialist highlighted.
9. **Performance** — RTF table + cold-start + installer size, lifted from README (with the same "indicative, varies" honesty note).
10. **Comparison** — the README's table (molvi vs Handy / Scribe / VoiceInk / Vocalinux / superwhisper). Handy noted as "kin, not a rival."
11. **Known limitations** — lifted verbatim-in-spirit from README (Nemotron commas-only, unsigned installers, Apple Silicon only, Wayland caveat, desktop only, BYO-LLM polish). Honesty = trust.
12. **Download / platforms** — 3 OS cards (Win 11 / macOS Apple Silicon / Linux), visitor's OS highlighted, version + size + changelog link. NOTE: v0.1.0 is built+signed but **not yet published** — the page links to the Releases page and is honest about that.
13. **FAQ** (6–7, accordion + JSON-LD `FAQPage` schema) — see copy below.
14. **Open-source CTA** — "Star on GitHub" · Contribute · **MIT OR Apache-2.0** · Roadmap · Acknowledgements (GigaAM/Nemotron/transcribe-rs/parakeet-rs/ort/Tauri).
15. **Footer** (minimal, only filled columns) — Product (Download, Changelog, Roadmap) · Resources (Repo, Privacy, Security) · Community (GitHub, Contributing) · `Made by @bumbaRasch` · © 2026 · license.

---

## 5. Copy (canonical EN)

> RU translations of all hero/privacy/FAQ copy are part of this spec's deliverable but are authored in the `ru` locale file at build time. RU is the only non-EN locale given a full reviewed pass at launch.

**Hero**
- H1: **"Speak. It types. Nothing leaves."**
- Subhead: *"molvi turns your voice into text in any app — transcribed on your device, in 36 UI languages, with a dedicated Russian engine. Free and open source, for Windows, macOS, and Linux."*
- Primary CTA: `Download for [detected OS]` · Secondary: `View on GitHub ★`
- Microcopy: `Free · Open source · No account · ~11 MB installer`

**Zero-trust strip:** `0 bytes sent to the cloud · 0 accounts · 0 telemetry`

**Privacy line (lead):** **"Private by design, not by promise."** + sub: *"Recognition runs 100% on your CPU. Your voice never leaves your computer — and that claim is enforced by a test suite, not a marketing sentence."*

**Verify-it-yourself (privacy section):** *"Block molvi in your firewall and dictate: recognition still works, proving nothing left the machine. Or watch your network monitor during a session — it stays silent. Or read the privacy test suite that fails the build if any transcript text reaches a log."*

**Russian claim:** *"No other dictation tool in this space runs a Russian-native engine — everyone else transcribes Russian through a multilingual model."*

**How it works (3 steps):**
1. Hold your hotkey (default `Alt + ` `) — the overlay appears.
2. Speak — molvi transcribes on your CPU; words stream live to the overlay.
3. Release — polished text pastes wherever your cursor is.

**FAQ (6):**
- **Is it really free?** — Yes. MIT OR Apache-2.0, forever. No account, no subscription, no feature gate.
- **Does it need the internet?** — Only once, to download the model (~214 MB Russian / ~2.4 GB multilingual). After that, fully offline. The only other outbound call is the optional update check, which you can turn off.
- **Does my audio get sent anywhere?** — No. Recognition runs on your CPU; the engine makes zero network requests while dictating. Audio is never written to disk.
- **Which languages?** — 36-language UI (incl. Arabic & Hebrew RTL) + 40+ recognition locales. Russian has a dedicated fast, natively-punctuated engine (GigaAM-v3).
- **How big is the download?** — Installer ~11 MB. Models download on first run and are cached; they're not bundled, to keep the installer tiny.
- **Which OSes?** — Windows 11, macOS (Apple Silicon), Linux. macOS Intel is not supported (upstream runtime limitation); Linux X11 is full, Wayland has partial profile support.

**Tone rules:** concrete numbers beat adjectives · user vocabulary (dictation/voice typing/push-to-talk), not internals (RTF/VAD/ort-pin) · one primary CTA per section · never fake testimonials · state real limits plainly.

---

## 6. Visual / brand

> **Implementation note:** the visual design pass (palette refinement, typography pairing, layout/composition, component styling, spacing scale, dark/light tokens) is driven by the **`ui-ux-pro-max`** skill — query its local database for the chosen product type (developer/utility tool), a style direction, and a font pairing; apply recommendations through `ui-ux-pro-max`, then enforce the brand constraints below. `seo` governs §7–§8.

- **Accent = the icon's blue gradient** (`#5EB3E6` sky → `#1E5A8E` navy), white silhouette. The app's internal settings-UI teal (`#0E7C86`) is **not** carried to the site — the public brand mark is the icon.
- **The "shushing" motif** is reused subtly (favicon, the privacy section marker, the loading state) — it reads as "voice + discretion."
- **Theme:** system-default with a visible **light/dark/system** toggle; **dark is the polished primary** skin (deep navy/near-black bg + gradient accent), light is clean white + same accent. No FOUC: theme resolved from `localStorage`/`prefers-color-scheme` before first paint.
- **Motion:** minimal — only subtle scroll reveals and the hero GIF. All gated behind `prefers-reduced-motion` (reduced → static hero image instead of GIF, no reveals).
- **Contrast:** WCAG AA 4.5:1 on all text. No gray-on-gray.
- **Type:** system font stack (no web-font download on first paint) for LCP; an optional display weight loaded with `font-display: swap` if a distinctive headline face is wanted later.

---

## 7. SEO

(Implemented with the `seo` skill — Addy Osmani/web-quality.)
- **Per-locale `<title>`/`<meta name="description">`**, EN-first keyword set around: *offline dictation, private voice-to-text, Russian speech to text, open source dictation, push-to-talk, local ASR*.
- **OG + Twitter card** tags per page; `og-default.png` = the icon on the blue gradient with the tagline.
- **JSON-LD schema:** `SoftwareApplication` (name, OS, license, free) + `FAQPage` (the FAQ) + `Organization`. `applicationCategory: "MultimediaApplication"`, `operatingSystem: "Windows, macOS, Linux"`, `offers.price: 0`.
- **hreflang** alternates for all 6 locales + `x-default` → EN.
- **`sitemap.xml`** + **`robots.txt`** (allow all, point to sitemap).
- Canonical URLs per locale to prevent dup-content.
- Semantic headings (one `<h1>`, ordered `<h2>`/`<h3>`), descriptive `alt` on every image, Lighthouse SEO ≥ 95.

---

## 8. Performance & accessibility

- **CWV targets:** LCP ≤ 2.5 s · INP ≤ 200 ms · CLS ≤ 0.1.
- Hero GIF/image: AVIF/WebP, `width`/`height` reserved (no CLS), lazy-load below the fold, preload the hero asset.
- Zero render-blocking JS; Astro ships HTML+CSS only by default.
- **a11y:** semantic landmarks, keyboard-navigable nav + accordion, visible focus rings, 44 px tap targets, `prefers-reduced-motion` honored, `lang`/`dir` correct per locale.
- Lighthouse Performance/Accessibility/Best-Practices ≥ 95 on mobile.

---

## 9. Deployment

- **GitHub Actions** workflow (`.github/workflows/deploy.yml`): on push to `main` → `npm ci && npm run build` → deploy `dist/` to the `gh-pages` branch (peaceiris/actions-gh-pages or official `actions/deploy-pages`).
- **Base path:** Astro `site` + `base` configured for `https://bumbaRasch.github.io/molvi-website/` (relative asset paths so a custom domain swap is a config change, not a refactor).
- A draft PR preview (optional, later) via GitHub Pages artifact previews.

---

## 10. Acceptance criteria (definition of done)

1. All 6 locale pages render, localized, with correct `lang`/`dir` and hreflang alternates.
2. Download button correctly detects Windows / macOS / Linux and links to the matching release asset; falls back gracefully if the v0.1.0 release isn't published (links to Releases page).
3. Theme toggle works (light/dark/system), no FOUC, persisted.
4. Lighthouse mobile: Performance ≥ 95, Accessibility ≥ 95, SEO ≥ 95, Best Practices = 100.
5. JSON-LD validates (SoftwareApplication + FAQPage); hreflang validated in Google Search Console (later).
6. No fake social proof; all product facts match the app README.
7. `prefers-reduced-motion` tested — hero GIF becomes a static image.
8. Acknowledgements, license (MIT OR Apache-2.0), and "kin" note for Handy present.

---

## 11. Open follow-ups (post-spec)

- **Screenshots: DONE** — `overlay.png`, `settings.png`, `onboarding.png` exist in the app repo `docs/img/` (mapped to sections in §2.1). Still TODO: produce pre-resized AVIF/WebP variants for web.
- Record the ~3 s silent overlay→paste GIF for the hero (use `overlay.png` as static fallback until then).
- RU full reviewed translation of all EN copy (the one non-EN locale with a native pass).
- Decide whether/when to add a custom domain (`molvi.app`) and a `CNAME`.
