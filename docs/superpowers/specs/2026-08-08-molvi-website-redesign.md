# molvi-website redesign — «сайт-визитка» (trim documentation → landing)

> Date: 2026-08-08. Supersedes the content scope of `2026-08-07-molvi-website-design.md` (architecture/brand/tech stack unchanged). Source: user verdict — "you made documentation, not a website"; approved 8-block structure.

## Goal
Trim the over-documented marketing site into a polished, minimal **single-page product landing (сайт-визитка)**: one clear job (download), one CTA repeated, benefit-led copy, technical detail demoted to the footer. Keep EN canonical + RU reviewed parity.

## Approved final structure (8 blocks, top → bottom)

1. **Nav** — logo + **«GitHub»** (was «View on GitHub» → `nav.github` key value change) + Download anchor.
2. **Hero** — headline ≤6 words + 1-line subhead (offline/private hook) + **3 OS download buttons** (OS-detected primary CTA, reusing `detectOs`). TrustStrip's "zero telemetry / built with" line folds into the hero subhead area or is dropped.
3. **HowItWorks** — 3 steps (hold hotkey → speak → text pastes). The feature breadth (command mode, snippets+dictionary, history, hotkey, overlay, updater) is told here as **one tight icon/inline row**, NOT six Bento cards.
4. **Privacy** — ONE emotional line ("Dictation that never leaves your machine") + 3–4 tight bullets (on-device, no account, no cloud, open source). Drop the separate "verify" card / test-suite link from the body (move link to footer if kept).
5. **Languages** — ONE compact line: "40+ recognition languages · 36 UI languages" + RU highlight chip. Was 3 stat-cards → collapse to a single row.
6. **Download** — 3 clean OS cards (Windows/macOS/Linux; human labels; visitor-OS highlighted + primary). **REMOVE** the raw asset filename (`molvi_0.1.0_x64-setup.exe`), the `~11 MB installer · Version 0.1.0` line, and the Changelog link from the cards. (Changelog link lives in the footer.)
7. **FAQ** — trim from 6 → **3–4** objection-handlers only (keep `<details>`, keep FAQPage JSON-LD in sync).
8. **Footer** — GitHub · Changelog (releases URL) · License (MIT OR Apache-2.0) · a one-line privacy note. The "escape valve" for every useful-but-non-converting artifact.

## CUT entirely (remove components + their i18n keys from all 6 locales)
- `Comparison.astro` (+ `src/data/comparison.ts` + all `cmp.*` keys × 6 locales).
- `Performance.astro` (+ `performance.*` keys × 6).
- `Limitations.astro` (+ `limitations.*` keys × 6).
- `OssCta.astro` (+ `oss.*` keys × 6; its star-count fetch + node:https code goes too).
- The 3 `FeatureRow` usages in Home.astro AND the `FeatureRow.astro` component itself (its only consumers are those 3 usages; Privacy/Languages are separate components) + `features.*` keys.
- `Bento.astro` (folded into HowItWorks) + `bento.*` keys — unless HowItWorks reuses the icon row; if so, keep a trimmed key set.
- `TrustStrip.astro` + `trust.*` keys (fold the one-line "zero telemetry" sentiment into the hero subhead; drop the rest).

## EDIT (in place)
- Nav: `nav.github` value → "GitHub" (EN) / "GitHub" (RU — proper noun, verbatim). `nav.githubAria` stays descriptive.
- Download: remove the `.dl-asset code` (filename), `.dl-meta` (size·version), `.dl-changelog` link, and the `.dl-note` SmartScreen/Gatekeeper bypass aside (move trust line to footer; the bypass warnings are docs — drop or footer). Keep 3 OS cards + `.dl-btn` + OS-detect upgrade.
- Privacy: collapse to one statement + 3–4 bullets (remove verify-card markup / `privacy.verify*` keys if the section is restructured).
- Languages: collapse 3 stat-cards → one row.
- HowItWorks: add the compact feature icon-row inline (reuse existing feature copy trimmed).
- FAQ (`faq.ts` + `Faq.astro`): keep best 3–4 entries; FAQPage JSON-LD must match.
- Footer: add Changelog link + releases URL; keep license line.

## Copy direction
- Benefit-led, punchy. Hero headline ≤6 words. Subhead 1 sentence.
- Minimal numbers on the page: "40+ / 36" languages earns its place (credibility + breadth); RTF/latency numbers do NOT (cut).
- Tone: "Dictation that never leaves your machine" — emotional privacy hook once, then move on.

## i18n / parity rules (binding)
- EN canonical + RU reviewed ship. es/de/fr/zh stay EN-fallback stubs, noindex, unlinked.
- Every removed key is removed from ALL 6 locale files (parity gate: `npx tsx scripts/check-i18n.ts` + `scripts/audit-i18n.ts` must stay green — key count drops from 155 to whatever remains, but en===ru===es===de===fr===zh key SETS identical).
- Proper nouns + ASCII `{tokens}` verbatim in all locales (audit-i18n enforces proper-noun parity).
- RU translations for any NEW/edited copy need human review (mark with `// REVIEW`).

## SEO (keep correct; adjust for cuts)
- canonical / hreflang (x-default+en+ru) / per-locale noindex — UNCHANGED.
- JSON-LD: keep SoftwareApplication + Organization. FAQPage must reflect the trimmed 3–4 FAQ entries. (Comparison/Limitations/OssCta had no JSON-LD — nothing to remove there.)
- sitemap (en+ru only), robots.txt, llms.txt — UNCHANGED (llms.txt content may be lightly trimmed to match the shorter page if it references cut sections; verify).
- Base path `/molvi-website/`, dark=primary, brand gradient #5EB3E6→#1E5A8E, license MIT OR Apache-2.0 — UNCHANGED.

## Out of scope (do NOT touch)
- astro.config.mjs (EXCEPT adding the `fonts` config — see Design Direction), deploy.yml, theme toggle, locale switcher, OS-detect script, sitemap/robots.
- The app repo (read-only).

## Design Direction (approved 2026-08-08 — «The sound wave that stays on your machine»)
Grounded in: ui-ux-pro-max `--design-system` (Modern Dark / Cinema, dark-primary, glassmorphism) + competitor research (competitor-design-research.md). Backward-compat is OFF the table — use the latest CSS (container queries, `color-mix`, `:has()`, view-transitions) and the Astro 7 `fonts` API freely.

**Visual language:**
- Deep **blue-ink dark** — refine `--bg` from `#0A1320` → `#070B14` (near-black with blue undertone; NEVER pure `#000`/neutral zinc). `--bg-elev` → `#0E1626` (ink-raised). Light theme stays secondary.
- **Palette (5 working tokens):** ink `#070B14` · ink-raised `#0E1626` · brand signal gradient `#5EB3E6→#1E5A8E` **demoted to a controlled glow** (waveform, button glow, focus rings — NOT large fills) · signal-soft `#0E7C86` (the app teal — for "verified"/trust semantics only) · text `#E6EDF6` / mute `#8A97A8`.
- **Texture:** ~3% film-grain overlay (SVG `feTurbulence` or tiny tiled PNG, fixed full-viewport, pointer-events:none) — anti-flat-dark. Glass `backdrop-filter: blur()` on the scrolled nav only (already partially present).
- **Type (Astro 7 `fonts` API, self-host, preload, `font-display: swap`):** **Instrument Serif** for display (hero + section titles) + **Inter** for body/UI + `ui-monospace` for the hotkey glyph. Verify Instrument Serif **Cyrillic** coverage (RU is shipped) — if absent, substitute a Cyrillic-capable display serif (e.g. a serif with Cyrillic subset) for RU headings via locale-aware font-family; never let RU headings fall back to a mismatched face. cssVariables: `--font-display`, `--font-ui`.

**Signature (one loud element; everything else disciplined-quiet — Chanel rule):**
- **The ambient waveform** — a locally-generated, animated SVG waveform in the hero, rendered in the brand gradient glow. It IS the privacy proof (lives on the page, nothing transmitted) AND the voice signature (competitors show keyboard OUTPUT; molvi is voice INPUT). Echoed ONCE at tiny scale in HowItWorks (the recording dot). Never repeated full-size. `prefers-reduced-motion` → static line.

**Section rhythm (asymmetric; the 8 blocks from above, with Privacy as a pause):**
- Hero → HowItWorks (3 steps, numbering EARNED by a real sequence) → **Privacy thesis** (full-bleed, near-empty, ONE emotional sentence — the breath of negative space) → Languages (one line + RU chip) → Download (3 clean OS cards, repeats hero CTA verbatim) → FAQ (3–4) → Footer.

**Motion:** ambient waveform oscillation (the single decorative motion) + minimal scroll-reveal (IntersectionObserver, subtle) + button hover-lift/glow. Full `prefers-reduced-motion` everywhere. No animation of `width`/`height`.

**3 principles to enforce:** (1) one signature, everything else quiet; (2) near-black with a hue + grain, never pure black/neutral; (3) privacy stated once emotionally + proven via provenance/absence, never a spec checklist.

**Frontend-design copy discipline:** hero headline is a thesis (serif, ≤6 words). Write from the user's side, plain verbs, sentence case, no filler. Microcopy carries trust ("Free & open source · no account"). Errors/emptiness never vague.

## Verification (each task + final)
- `npm run build` exit 0, 6 pages, 0 warnings.
- `npx tsx scripts/check-i18n.ts` + `npx tsx scripts/audit-i18n.ts` green (parity + proper-noun).
- JSON-LD valid (FAQPage count matches rendered FAQ).
- 0 horizontal overflow at 375/768/1024/1440 × dark/light × EN+RU (re-run `.playwright-cli/measure.js`).
- No stray English on RU page; no broken internal links; no 404 assets.
