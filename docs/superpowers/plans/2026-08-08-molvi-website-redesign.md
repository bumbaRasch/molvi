# molvi-website Redesign — Design-Driven «сайт-визитка» Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Spec: `docs/superpowers/specs/2026-08-08-molvi-website-redesign.md` (read the Design Direction section — it governs every visual choice). Checkbox (`- [ ]`) tracking.

**Goal:** Rebuild the molvi marketing site as a premium, minimal single-page landing («сайт-визитка») with the approved design direction «The sound wave that stays on your machine»: blue-ink dark, Instrument Serif + Inter, an ambient waveform signature, grain texture, asymmetric rhythm with a Privacy-thesis pause — cutting all documentation-flavored content.

**Architecture:** Design-driven restructure of an existing Astro 7 static site. Foundation task (fonts/palette/grain/brand) → content cuts → section redesigns applying the new visual language → verify. One worker; build + i18n parity green at every task.

**Tech Stack:** Astro 7 static (fonts API, astro:assets Sharp), vanilla modern CSS (container queries / `color-mix` / `:has()` allowed — **backward-compat OFF the table**), minimal TS. Test cycle = build + i18n parity (`check-i18n.ts` + `audit-i18n.ts`) + JSON-LD validity + playwright overflow measure.

## Global Constraints (bind every task)
- Repo: `C:\Users\bumbarasch\Desktop\2026_Projects\molvi-website`. App repo (READ-ONLY, source of product facts + brand icons): `C:\Users\bumbarasch\Desktop\2026_Projects\molvi`. Host: Windows PowerShell 5.1 (no `&&`; `; if ($?)`). Non-ASCII (Cyrillic) → `Get-Content -Raw -Encoding UTF8`.
- Design Direction (spec §"Design Direction") governs: ink-dark palette (`--bg #070B14`, `--bg-elev #0E1626`, brand gradient `#5EB3E6→#1E5A8E` as GLOW not fill, `--signal-soft #0E7C86` for trust semantics, text `#E6EDF6` / mute `#8A97A8`); Instrument Serif (display) + Inter (body) via Astro fonts API; ~3% grain overlay; glass scrolled-nav; ONE signature = the ambient waveform.
- EN canonical + RU reviewed ship; es/de/fr/zh = `{...en}` stubs, noindex, unlinked. Every removed i18n key removed from ALL 6 locales — `npx tsx scripts/check-i18n.ts` + `scripts/audit-i18n.ts` MUST stay green (key SETS identical; proper-noun + {token} parity).
- Proper nouns (GigaAM, Nemotron, Tauri, ONNX, Rust, GitHub, Windows, macOS, Linux) verbatim. New/edited RU copy → `// REVIEW: redesign copy` marker in ru.ts.
- License MIT OR Apache-2.0. Base path `/molvi-website/`. SEO unchanged (canonical/hreflang x-default+en+ru/noindex/sitemap en+ru). JSON-LD: keep SoftwareApplication + Organization; FAQPage must match rendered FAQ count.
- Brand icons source: `C:\Users\bumbarasch\Desktop\2026_Projects\molvi\src-tauri\icons\` (`icon.png` 512 master, `128x128@2x.png` 256, `icon.ico`). No SVG → process via astro:assets (Sharp → AVIF/WebP).
- Each task: `npm run build` exit 0 (0 warnings) + i18n gates green; commit per task (concise, repo style). Never commit `.playwright-cli/` or `dist/`. Backward-compat: ignore (latest CSS/fonts freely).
- VERIFY every API (Astro fonts, astro:assets) via ctx7 (`npx ctx7@latest docs /withastro/docs "…"`) or docs before coding — never model memory.

## File map
- **Astro config:** `astro.config.mjs` (+`fonts` array). **Styles:** `src/styles/global.css` (palette tokens, font vars, grain utility). **Layout:** `src/layouts/Base.astro` (<Font> preload).
- **New:** `src/components/Waveform.astro` (signature), `src/components/Brand.astro` maybe (nav/footer mark) — implementer's call.
- **Assets:** copy brand icons → `src/assets/`; regenerate favicon/apple-touch/og-default with the real mark (gen-assets.mjs or direct Sharp).
- **Delete:** Comparison/Performance/Limitations/OssCta/FeatureRow/Bento/TrustStrip `.astro` + `src/data/comparison.ts`.
- **Redesign:** Hero, HowItWorks, Privacy, Languages, Download, Faq (+`src/data/faq.ts`), Nav, Footer, Home.astro (section list), `public/llms.txt`.
- **i18n:** all 6 `src/i18n/locales/*.ts` (remove cut keys; add concise design-copy keys EN+RU).
- **Untouched:** deploy.yml, sitemap/robots config, theme/locale/os-detect scripts.

---

### Task D1: Design-system foundation (fonts + palette + grain + glass-nav + brand assets)
**Files:** `astro.config.mjs` (+fonts), `src/layouts/Base.astro` (<Font preload), `src/styles/global.css` (tokens + font vars + `.grain` utility + glass-nav), `src/assets/` (brand icons), favicon/apple-touch/og regen, `public/`.
**Verify API first (ctx7):** Astro 7 `fonts` config (`fontProviders.fontsource()`), `<Font cssVariable preload>` from `astro:assets`, astro:assets Sharp `format`/`widths`.
- [ ] ctx7-confirm Astro 7 fonts API + `<Font>` usage; confirm Instrument Serif + Inter available via fontsource provider; **verify Instrument Serif has a Cyrillic subset** — if not, choose a Cyrillic-capable display serif for RU (locale-aware font-family: `--font-display` per lang) and note it.
- [ ] `astro.config.mjs`: add `fonts: [{provider: fontsource(), name:'Instrument Serif', cssVariable:'--font-display'}, {provider: fontsource(), name:'Inter', cssVariable:'--font-ui', weights:[400,500,600,700]}]` (verify exact API/shape from ctx7).
- [ ] `Base.astro`: add `<Font cssVariable="--font-display" preload />` + `<Font cssVariable="--font-ui" preload />` in `<head>` (per ctx7).
- [ ] `global.css`: refine palette tokens to ink (`--bg:#070B14`, `--bg-elev:#0E1626`, `--bg-elev-2` deeper); keep brand gradient vars; add `--signal-soft:#0E7C86`; text `#E6EDF6`/mute `#8A97A8`; set `--font-ui`/`--font-display` stack; add `.grain` overlay utility (SVG feTurbulence data-uri or tiny PNG, fixed, full-viewport, ~3% opacity, pointer-events:none, z-index high but below modal/nav-chrome); ensure `:focus-visible`, contrast AA still holds on new bg (recompute accent/mute if needed).
- [ ] Brand assets: copy `icon.png` (512) + `128x128@2x.png` (256) + `icon.ico` from app `src-tauri/icons/` into `src/assets/`; regenerate `favicon.ico` (use icon.ico as-is), `apple-touch-icon.png` (180, Sharp from icon.png), `og-default.png` (1200×630 composed: ink bg + grain + brand mark + "molvi" wordmark in Instrument Serif — via gen-assets.mjs/node:sharp). Replace the placeholder favicon.svg "m" gradient with the real mark where used (nav/footer brand).
- [ ] Verify: `npm run build` exit 0, 0 warnings; fonts self-host (woff2 in dist/_astro/), preload tags present; grain renders; no FOUC (theme inline script untouched); old components still render OK on new tokens.
- [ ] Commit: `feat(design): ink-dark palette + Instrument Serif/Inter fonts + grain + real brand mark`.

### Task D2: Content cuts (remove 7 sections + i18n keys)
**Files:** delete Comparison/Performance/Limitations/OssCta/FeatureRow/Bento/TrustStrip `.astro` + `src/data/comparison.ts`; `Home.astro` (imports+usages + orphaned image imports/styles); 6 locale files (remove `cmp.*`,`performance.*`,`limitations.*`,`oss.*`,`features.*`,`bento.*`,`trust.*`).
- [ ] Remove the 7 components' imports + usages from Home.astro; remove now-orphaned `settingsShot`/`onboardingShot` imports + `.privacy-motif` style.
- [ ] Delete the 7 `.astro` files + `comparison.ts`.
- [ ] Remove the listed key blocks from all 6 locale files.
- [ ] Verify: build 0 warnings; `check-i18n.ts` + `audit-i18n.ts` PASS; rendered EN+RU pages have none of the cut sections (grep dist).
- [ ] Commit: `refactor(site): cut 7 doc-heavy sections (comparison/perf/limits/oss/features/bento/trust)`.

### Task D3: Waveform signature + Hero (serif thesis + 3-OS CTA)
**Files:** new `src/components/Waveform.astro`; redesign `Hero.astro`; locale keys (hero thesis/subhead/CTA/microcopy EN+RU, RU `// REVIEW`).
- [ ] `Waveform.astro`: an SVG ambient waveform — a smooth multi-peak path in the brand gradient (as a soft glow, via SVG gradient + blur filter or `filter: drop-shadow`). Animate via CSS keyframes (translate/scale of path or opacity shimmer) OR a tiny TS rAF loop; **`prefers-reduced-motion` → static rendered line**. Accessible (`aria-hidden="true"` — decorative). Locally generated, no audio.
- [ ] `Hero.astro`: serif thesis headline (Instrument Serif, e.g. EN "Speak freely. It stays here." / RU equivalent under `// REVIEW`) + 1-line subhead (the on-device/offline hook) + `<Waveform/>` prominent + **3 OS download buttons** (detectOs → visitor-OS `btn-primary`, others `btn-secondary`; reuse `src/scripts/os-detect.ts` + `OS_ASSETS`/`assetUrl` from `src/data/site.ts`; no-JS fallback = Windows primary) + microcopy "Free & open source · no account".
- [ ] Verify: build green; hero renders headline+waveform+3 buttons at 375/768/1024/1440 (0 overflow); reduced-motion → static waveform; RU headline uses Cyrillic-capable display face.
- [ ] Commit: `feat(hero): ambient waveform signature + serif thesis + 3-OS download CTA`.

### Task D4: HowItWorks (3 steps + feature row + waveform echo)
**Files:** redesign `HowItWorks.astro`; locale keys (`how.steps.*` + new `how.features.*` icon-row EN+RU, RU `// REVIEW`).
- [ ] 3 numbered steps (hold hotkey → speak → text pastes) — numbering EARNED (real sequence). Compact.
- [ ] One tight feature icon-row (5 items: command mode, snippets+dictionary, history, hotkey+overlay, auto-updater) — inline SVG icons (`--signal` color), small labels; responsive wrap. Reuse existing icon vocabulary.
- [ ] Waveform echo: a tiny static waveform-as-recording-dot near step 1 (the one allowed repeat, small).
- [ ] Verify: build+parity green; 3 steps + 1 feature row render clean at all breakpoints (0 overflow).
- [ ] Commit: `feat(how): 3 earned steps + compact feature row + waveform echo`.

### Task D5: Privacy thesis (full-bleed pause) + Languages (one line)
**Files:** redesign `Privacy.astro`, `Languages.astro`; locale keys (privacy thesis + bullets; languages one-liner; EN+RU, RU `// REVIEW`).
- [ ] `Privacy.astro`: **full-bleed near-empty section** — ONE emotional sentence (serif, large, e.g. "Dictation that never leaves your machine") + 3–4 tight provenance bullets (on-device / no account / no cloud / open source). Generous negative space; this is the "breath." Remove the verify-card/test-suite-link from body (link can go to footer).
- [ ] `Languages.astro`: ONE compact row — "40+ recognition languages · 36 UI languages" + an RU highlight chip. Remove the 3-card grid + chips list.
- [ ] Trim/replace locale keys; remove unused (`privacy.verify*`, extra `languages.*`) from all 6 locales.
- [ ] Verify: build+parity+audit green; Privacy is one statement + ≤4 bullets; Languages one line; 0 overflow.
- [ ] Commit: `refactor(site): Privacy thesis pause + one-line Languages`.

### Task D6: Download (clean) + Nav («GitHub»/glass) + Footer (changelog/trust) + FAQ (3–4)
**Files:** `Download.astro`, `Nav.astro`, `Footer.astro`, `Faq.astro`, `src/data/faq.ts`, `public/llms.txt`, locale keys.
- [ ] `Download.astro`: 3 OS cards (real brand icon per card optional; human labels; visitor-OS highlighted+primary via detectOs). **Remove** `.dl-asset` filename, `.dl-meta` size·version, `.dl-changelog` link, `.dl-note` bypass aside + their CSS. Keep id="download".
- [ ] `Nav.astro`: brand uses real mark; label `nav.github`→"GitHub" (all locales, proper noun verbatim); glass `backdrop-filter` on scrolled nav.
- [ ] `Footer.astro`: real brand mark + Changelog link (RELEASES_URL, _blank) + one-line trust/privacy note + license (MIT OR Apache-2.0).
- [ ] `faq.ts` + `Faq.astro`: keep 3–4 best objection-handlers (EN+RU); FAQPage JSON-LD must match rendered count exactly (no hardcoded count). Remove unused faq keys (6 locales).
- [ ] `public/llms.txt`: trim to match the 8-block page (remove cut-section refs).
- [ ] Verify: build+parity green; Download clean; Nav "GitHub"; Footer has Changelog; FAQ 3–4 + JSON-LD matches (`.playwright-cli/validate-jsonld.cjs` ALL VALID).
- [ ] Commit: `feat(site): clean Download + glass Nav «GitHub» + footer changelog + FAQ 3-4`.

### Task D7: Full re-verify + whole-branch redesign review
**Files:** none (verification + review).
- [ ] `npm run build` exit 0, 6 pages, 0 warnings. `check-i18n.ts` + `audit-i18n.ts` green.
- [ ] Overflow measure (`.playwright-cli/measure.js`) EN+RU × 375/768/1024/1440 × dark/light → 0 pageOverflow all 16. Tap targets ≥44px on kept interactive els (footer links remain dense/deferred).
- [ ] Contrast AA on new ink palette (re-run checker; note gradient-glow elements are decoration). Reduced-motion: waveform static.
- [ ] RU no-EN-leak (case-sensitive grep dist/ru/index.html); Cyrillic display face applied to RU headings.
- [ ] JSON-LD valid; canonical/hreflang/noindex correct; sitemap en+ru only; assets base-prefixed (0 stray `/_astro/`); fonts self-hosted + preloaded; no FOUC.
- [ ] Dispatch final whole-branch reviewer (most capable) over redesign BASE (pre-redesign `ad78538`)..HEAD; fix residuals ONE wave; re-verify.
- [ ] Capture post-redesign screenshots (dark+light × mobile+desktop) into `.playwright-cli/qa/`; report new RU keys for human review.

## Self-review (controller)
- 8-block target coverage: Nav(D6)+Hero(D3)+HowItWorks(D4)+Privacy(D5)+Languages(D5)+Download(D6)+FAQ(D6)+Footer(D6). ✓ Design Direction (fonts/palette/grain/waveform/glass) in D1; signature waveform D3; Privacy-thesis pause D5. ✓
- Parity green per task (each removes/changes its own keys). ✓ No placeholders: copy is direction+examples, implementer writes final strings (RU `// REVIEW`). ✓ APIs flagged for ctx7 verification (D1 fonts/assets). ✓
