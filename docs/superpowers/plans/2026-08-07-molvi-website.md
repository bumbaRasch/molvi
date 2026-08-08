# molvi Marketing Website — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the bilingual+ marketing website for molvi as a static Astro site in a new sibling repo `../molvi-website`, deployed to GitHub Pages, leading with the privacy/on-device story.

**Architecture:** Single landing page per locale, Astro static output, zero JS by default (theme toggle + OS-detect + nav + accordion are the only client scripts). Six locale subdirectories (`/` EN canonical, `/ru/`, `/es/`, `/de/`, `/fr/`, `/zh/`) with `hreflang` alternates. Copy is the localized extension of the app repo's `README.md`.

**Tech Stack:** Astro 5 (static), `@astrojs/sitemap`, `astro:assets` (Sharp), vanilla CSS + ~minimal vanilla TS. Deploy: GitHub Pages via `withastro/action`. Build agent guided by skills `astro`, `seo`, `ui-ux-pro-max` (installed in the **app** repo's `.agents/skills/`).

## Testing approach (adapted — no unit framework)

This is a static marketing site; there is no JS unit-test runner (YAGNI — do not add one). Each task's "test cycle" is a concrete runnable check:
- **Build gate:** `npm run build` must exit 0 and emit `dist/`.
- **Assertion gate:** a `grep`/`Select-String` on `dist/**/*.html` (or a node one-liner) asserting the task's expected output is present (e.g. hreflang tags, JSON-LD, a translated string, an OG tag).
- **Where noted:** `npx lighthouse-cli` or a manual a11y checklist.
Every task ends with its build+assertion passing, then a commit. Run all commands from the website repo root (`../molvi-website`) unless noted.

## Global Constraints

Copied verbatim from the spec ([`docs/superpowers/specs/2026-08-07-molvi-website-design.md`](../specs/2026-08-07-molvi-website-design.md)):

- **Node:** v24.15.0+ (the `withastro/action` defaults to Node 24).
- **Locales (6):** `en` (canonical, at `/`) · `ru` (reviewed) · `es` · `de` · `fr` · `zh-CN`. Astro i18n `defaultLocale: "en"`, **no `prefixDefaultLocale`** (EN lives at `/`).
- **Site/base (GH Pages project URL):** `site: 'https://bumbaRasch.github.io'`, `base: '/molvi-website'`.
- **Styling:** vanilla CSS only — no Tailwind, no UI lib. CSS custom properties for theming.
- **Brand accent:** icon blue gradient `#5EB3E6` → `#1E5A8E`; white silhouette. Do NOT use the app's settings-UI teal `#0E7C86` on the site.
- **Proper nouns stay verbatim** in all locales: GigaAM, Nemotron, Tauri, ONNX, Rust, GitHub, Windows, macOS, Linux. ASCII tokens (`{size}`, `{n}`) verbatim.
- **No fake social proof.** No testimonials, no invented user counts, no stock-photo people. Star count is read live from GitHub API (or omitted if zero).
- **License line:** `MIT OR Apache-2.0`.
- **Source of truth for product facts:** the app repo `README.md`. If README and this plan disagree, README wins.
- **Skills to consult during build:** `astro` (framework patterns), `seo` (Addy Osmani/web-quality — meta/schema/CWV/hreflang), `ui-ux-pro-max` (palette/typography/layout/component styling). Load via the `skill` tool.
- **Anti-stale:** Astro APIs in this plan were verified 2026-08-07 via ctx7 (`/withastro/docs`): `glob` loader from `astro/loaders`, i18n `routing.prefixDefaultLocale`, `@astrojs/sitemap` `i18n` option, `withastro/action@v6` + `actions/deploy-pages@v5` + `actions/checkout@v7`.

---

## File Structure (the decomposition)

```
molvi-website/
├── astro.config.mjs              # site/base/i18n/sitemap/integrations
├── package.json
├── tsconfig.json
├── .gitignore
├── .nvmrc                         # "24"
├── public/
│   ├── favicon.svg, favicon.ico, apple-touch-icon.png, og-default.png
│   ├── robots.txt
│   └── img/                       # overlay/settings/onboarding (+ AVIF/WebP via astro:assets live in src/assets)
├── src/
│   ├── assets/                    # images imported through astro:assets (Sharp-optimized)
│   ├── content.config.ts          # not strictly needed (no MDX collections at launch) — omit if unused
│   ├── env.d.ts
│   ├── i18n/
│   │   ├── config.ts              # LOCALES, DEFAULT_LOCALE, LANG_ATTR (en-US, ru-RU, …)
│   │   ├── ui.ts                  # t(locale, key, vars?) + types
│   │   └── locales/{en,ru,es,de,fr,zh}.ts   # one Record<string,string> per locale
│   ├── data/
│   │   ├── faq.ts                 # FAQ entries (keyed) per locale
│   │   ├── comparison.ts          # the README comparison table as data
│   │   └── site.ts                # APP_REPO_URL, RELEASES_URL, VERSION, INSTALLER_SIZE
│   ├── layouts/
│   │   └── Base.astro             # <html lang/dir>, <head>, hreflang, JSON-LD, theme-init
│   ├── components/
│   │   ├── Nav.astro, LocaleSwitcher.astro, ThemeToggle.astro
│   │   ├── Hero.astro
│   │   ├── TrustStrip.astro
│   │   ├── HowItWorks.astro
│   │   ├── FeatureRow.astro       # generic alternating row (reused 3×)
│   │   ├── Bento.astro
│   │   ├── Privacy.astro
│   │   ├── Languages.astro
│   │   ├── Performance.astro
│   │   ├── Comparison.astro
│   │   ├── Limitations.astro
│   │   ├── Download.astro
│   │   ├── Faq.astro
│   │   ├── OssCta.astro
│   │   └── Footer.astro
│   ├── scripts/
│   │   ├── theme-init.ts          # inline, runs before paint (no FOUC)
│   │   ├── os-detect.ts           # download-button OS detection
│   │   ├── theme-toggle.ts        # light/dark/system persistence
│   │   ├── nav.ts                 # mobile hamburger
│   │   └── faq.ts                 # accordion
│   ├── styles/
│   │   ├── global.css             # tokens, reset, base, utilities
│   │   └── prism.css              # (only if code blocks needed — likely omit)
│   └── pages/
│       ├── index.astro            # EN
│       └── {ru,es,de,fr,zh}/index.astro   # one thin wrapper per locale
└── .github/workflows/deploy.yml
```

**Why per-locale page wrappers (not one dynamic `[locale]/index.astro`):** six fixed locales, one landing page each. Explicit wrappers are simpler to read, localize, and SEO-audit than a `getStaticPaths` dynamic route, and they make "this locale isn't ready → don't link it" a one-file deletion. Each wrapper is ~10 lines: set locale, import strings, render `<Base locale>`.

---

## Task 1: Scaffold repo + Astro + base config

**Files:**
- Create: `../molvi-website/` (new sibling repo), `package.json`, `astro.config.mjs`, `tsconfig.json`, `.gitignore`, `.nvmrc`, `src/env.d.ts`, `src/pages/index.astro`

**Interfaces:**
- Produces: a building Astro project; `astro.config.mjs` exports the canonical `site`/`base`/`i18n` config consumed by Tasks 3, 15.

- [ ] **Step 1: Create the sibling repo and scaffold Astro**

```powershell
# from the app repo root (C:\Users\bumbarasch\Desktop\2026_Projects\molvi)
cd ..
npm create astro@latest molvi-website -- --template minimal --no-install --no-git --skip-houston --typescript strict
cd molvi-website
git init
git branch -M main
```

- [ ] **Step 2: Pin Node + add integrations**

Create `.nvmrc`:
```
24
```

Install the sitemap integration (Sharp ships with Astro):
```powershell
npm install
npm install @astrojs/sitemap
```

- [ ] **Step 3: Write `astro.config.mjs` (verified API)**

```js
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

// GH Pages project URL → base path is the repo name.
export default defineConfig({
  site: 'https://bumbaRasch.github.io',
  base: '/molvi-website',
  trailingSlash: 'ignore',
  i18n: {
    locales: ['en', 'ru', 'es', 'de', 'fr', 'zh'],
    defaultLocale: 'en',
    // prefixDefaultLocale is intentionally unset → 'en' lives at '/', others at '/<code>/'
  },
  integrations: [
    sitemap({
      i18n: {
        defaultLocale: 'en',
        locales: {
          en: 'en-US', ru: 'ru-RU', es: 'es-ES', de: 'de-DE', fr: 'fr-FR', zh: 'zh-CN',
        },
      },
    }),
  ],
});
```

- [ ] **Step 4: Write `.gitignore`, `tsconfig.json` (Astro strict default is fine), and a placeholder `src/pages/index.astro`**

`src/pages/index.astro`:
```astro
---
---
<html lang="en"><head><meta charset="utf-8"/><title>molvi</title></head>
<body><h1>molvi</h1></body></html>
```

- [ ] **Step 5: Build gate**

Run: `npm run build`
Expected: exits 0; `dist/index.html` exists; `dist/sitemap-0.xml` exists (sitemap integration wired).

- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "chore: scaffold Astro site with sitemap + i18n config"
```

---

## Task 2: i18n foundation (`config.ts`, `ui.ts`, 6 locale files)

**Files:**
- Create: `src/i18n/config.ts`, `src/i18n/ui.ts`, `src/i18n/locales/{en,ru,es,de,fr,zh}.ts`
- Create: `scripts/check-i18n.mjs` (dev-only key-parity check)

**Interfaces:**
- Produces: `LOCALES`, `DEFAULT_LOCALE`, `LANG_ATTR` (locale → BCP47 for `<html lang>`), `t(locale, key, vars?)`. Consumed by every section component + `Base.astro`.

- [ ] **Step 1: Write `src/i18n/config.ts`**

```ts
export const LOCALES = ['en', 'ru', 'es', 'de', 'fr', 'zh'] as const;
export type Locale = (typeof LOCALES)[number];
export const DEFAULT_LOCALE: Locale = 'en';
// BCP47 for <html lang> + OG locale. zh → zh-CN.
export const LANG_ATTR: Record<Locale, string> = {
  en: 'en-US', ru: 'ru-RU', es: 'es-ES', de: 'de-DE', fr: 'fr-FR', zh: 'zh-CN',
};
```

- [ ] **Step 2: Write `src/i18n/ui.ts`**

```ts
import { en } from './locales/en';
import { ru } from './locales/ru';
import { es } from './locales/es';
import { de } from './locales/de';
import { fr } from './locales/fr';
import { zh } from './locales/zh';
import { DEFAULT_LOCALE, type Locale } from './config';

export const dictionaries = { en, ru, es, de, fr, zh } as const;
export type Dict = Record<string, string>;

/** Look up a key in the given locale, falling back to EN, then to the raw key. */
export function t(locale: Locale, key: string, vars?: Record<string, string | number>): string {
  const raw = dictionaries[locale]?.[key] ?? dictionaries[DEFAULT_LOCALE][key] ?? key;
  if (!vars) return raw;
  return Object.entries(vars).reduce((s, [k, v]) => s.replaceAll(`{${k}}`, String(v)), raw);
}
```

- [ ] **Step 3: Write `en.ts` with the full canonical string set**

This file is the single source for copy. Use the canonical EN copy from spec §5. (Headline example; the file contains the full ~80 keys used across all sections — author them all here first.)

```ts
import type { Dict } from '../ui';
export const en: Dict = {
  'nav.how': 'How it works',
  'nav.features': 'Features',
  'nav.privacy': 'Privacy',
  'nav.download': 'Download',
  'hero.h1': 'Speak. It types. Nothing leaves.',
  'hero.sub': 'molvi turns your voice into text in any app — transcribed on your device, in 36 UI languages, with a dedicated Russian engine. Free and open source, for Windows, macOS, and Linux.',
  'hero.cta.download': 'Download for {os}',
  'hero.cta.github': 'View on GitHub',
  'hero.micro': 'Free · Open source · No account · ~11 MB installer',
  'trust.zero': '0 bytes sent to the cloud · 0 accounts · 0 telemetry',
  'trust.builtwith': 'Built with Tauri · Rust · GigaAM · Nemotron ASR · ONNX Runtime',
  'privacy.lead': 'Private by design, not by promise.',
  'privacy.sub': 'Recognition runs 100% on your CPU. Your voice never leaves your computer — and that claim is enforced by a test suite, not a marketing sentence.',
  'privacy.verify': 'Block molvi in your firewall and dictate: recognition still works, proving nothing left the machine. Or watch your network monitor during a session — it stays silent. Or read the privacy test suite that fails the build if any transcript text reaches a log.',
  'russian.claim': 'No other dictation tool in this space runs a Russian-native engine — everyone else transcribes Russian through a multilingual model.',
  // …faq.*, features.*, comparison.*, footer.*, etc. — all keys authored here first.
};
```

- [ ] **Step 4: Create the 5 non-EN locale files as EN copies (to be translated)**

Each of `ru.ts`, `es.ts`, `de.ts`, `fr.ts`, `zh.ts` initially re-exports the EN object (so the build is green and key sets are equal), with a top-of-file marker:
```ts
// TODO(i18n): translate to <lang>. Currently EN fallback. Do NOT link this locale in the locale switcher until reviewed (Global Constraints: no unreviewed locales ship).
import { en } from './en';
export const ru: typeof en = { ...en };
```
> RU is translated for real in Task 19. ES/DE/FR/ZH are translated/linked only when reviewed (Task 20).

- [ ] **Step 5: Write the key-parity check `scripts/check-i18n.mjs`**

```js
// node scripts/check-i18n.mjs — exits non-zero if any locale's key set !== en's.
import { dictionaries } from '../src/i18n/ui.ts';
const enKeys = Object.keys(dictionaries.en).sort();
for (const [loc, d] of Object.entries(dictionaries)) {
  const k = Object.keys(d).sort();
  if (JSON.stringify(k) !== JSON.stringify(enKeys)) {
    console.error(`i18n key mismatch in ${loc}`); process.exit(1);
  }
}
console.log('i18n keys OK across', Object.keys(dictionaries).length, 'locales');
```
(If ESM `.ts` import from node is awkward, import the compiled output or run via `npx tsx scripts/check-i18n.mjs`. The check must run.)

- [ ] **Step 6: Run the check**

Run: `npx tsx scripts/check-i18n.mjs`
Expected: `i18n keys OK across 6 locales`.

- [ ] **Step 7: Build gate**

Run: `npm run build`
Expected: exits 0.

- [ ] **Step 8: Commit**

```powershell
git add -A
git commit -m "feat(i18n): locale config, t() with EN fallback, 6 locale files, key-parity check"
```

---

## Task 3: `Base.astro` layout — `<head>`, hreflang, JSON-LD, theme-init, favicon

**Files:**
- Create: `src/layouts/Base.astro`, `src/scripts/theme-init.ts`, `src/data/site.ts`

**Interfaces:**
- Consumes: `LANG_ATTR`, `LOCALES` (Task 2).
- Produces: `<Base locale title description>` layout used by all 6 page wrappers; emits hreflang alternates + `SoftwareApplication`/`Organization` JSON-LD + the no-FOUC theme init.

- [ ] **Step 1: Write `src/data/site.ts`**

```ts
export const APP_REPO_URL = 'https://github.com/bumbaRasch/molvi';
export const RELEASES_URL = `${APP_REPO_URL}/releases`;
export const VERSION = '0.1.0';                 // bump on release
export const INSTALLER_SIZE = '~11 MB';
// Release asset URL template (GitHub releases download):
export const assetUrl = (osAsset: string) => `${RELEASES_URL}/download/v${VERSION}/${osAsset}`;
export const OS_ASSETS = {
  windows: 'molvi_0.1.0_x64-setup.exe',
  mac: 'molvi_0.1.0_aarch64.dmg',
  linux: 'molvi_0.1.0_amd64.AppImage',
};
```
> NOTE (spec §4.12): v0.1.0 is built+signed but **not yet published**. If the asset 404s, the Download component falls back to linking `RELEASES_URL` (Task 12).

- [ ] **Step 2: Write the no-FOUC theme init `src/scripts/theme-init.ts`**

This must be inlined into `<head>` so it runs before first paint.
```ts
// Inlined in Base.astro <head>. Sets data-theme on <html> before paint.
(function () {
  const stored = localStorage.getItem('molvi-theme'); // 'light' | 'dark' | 'system' | null
  const sys = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  const resolved = stored === 'system' || !stored ? sys : stored;
  document.documentElement.setAttribute('data-theme', resolved);
})();
```

- [ ] **Step 3: Write `src/layouts/Base.astro`**

```astro
---
import { LOCALES, LANG_ATTR } from '../i18n/config';
import { t } from '../i18n/ui';
import { APP_REPO_URL } from '../data/site';

interface Props { locale: typeof LOCALES[number]; title: string; description: string; }
const { locale, title, description } = Astro.props;
const lang = LANG_ATTR[locale];
const canonicalPath = locale === 'en' ? '/' : `/${locale}/`;
const siteBase = Astro.site;            // https://bumbaRasch.github.io
const origin = siteBase?.toString().replace(/\/$/, '') ?? '';
// hreflang alternates for all 6 locales + x-default → EN
const alternates = [
  { hreflang: 'x-default', href: `${origin}${Astro.base}/` },
  ...LOCALES.map((l) => ({
    hreflang: LANG_ATTR[l],
    href: `${origin}${Astro.base}${l === 'en' ? '/' : `/${l}/`}`,
  })),
];
const jsonLd = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: 'molvi',
  applicationCategory: 'MultimediaApplication',
  operatingSystem: 'Windows, macOS, Linux',
  url: `${origin}${Astro.base}/`,
  downloadUrl: `${origin}${Astro.base}/#download`,
  offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
  license: 'https://spdx.org/licenses/MIT-Apache-2.0.html',
};
const orgLd = { '@context': 'https://schema.org', '@type': 'Organization', name: 'molvi', url: APP_REPO_URL };
---
<!doctype html>
<html lang={lang} dir="ltr">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{title}</title>
  <meta name="description" content={description} />
  <link rel="canonical" href={`${origin}${Astro.base}${canonicalPath}`} />
  {alternates.map((a) => <link rel="alternate" hreflang={a.hreflang} href={a.href} />)}
  <!-- OG / Twitter -->
  <meta property="og:type" content="website" />
  <meta property="og:locale" content={lang.replace('-', '_')} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta property="og:url" content={`${origin}${Astro.base}${canonicalPath}`} />
  <meta property="og:image" content={`${origin}${Astro.base}/og-default.png`} />
  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:image" content={`${origin}${Astro.base}/og-default.png`} />
  <link rel="icon" href={`${Astro.base}/favicon.svg`} type="image/svg+xml" />
  <link rel="icon" href={`${Astro.base}/favicon.ico`} sizes="32x32 64x64" />
  <link rel="apple-touch-icon" href={`${Astro.base}/apple-touch-icon.png`} />
  <!-- no-FOUC theme init (inlined) -->
  <script is:inline set:html={`(function(){var s=localStorage.getItem('molvi-theme');var m=window.matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light';document.documentElement.setAttribute('data-theme',s==='system'||!s?m:s);})();`} />
  <link rel="stylesheet" href={`${Astro.base}/src/styles/global.css`} />
</head>
<body>
  <slot />
  <script type="application/ld+json" set:html={JSON.stringify(jsonLd)} />
  <script type="application/ld+json" set:html={JSON.stringify(orgLd)} />
</body>
</html>
```
> NOTE: Astro requires CSS to be imported as a module, not linked by URL. Step 4 corrects the stylesheet line — replace the `<link rel="stylesheet" href=...>` line with `import '../styles/global.css';` in the frontmatter, and remove that `<link>` tag. (Task 4 creates `global.css`.)

- [ ] **Step 4: Create an empty `src/styles/global.css`** so the import resolves (`:root{}` placeholder).

- [ ] **Step 5: Build + assertion gate**

Run: `npm run build`
Then assert hreflang + JSON-LD landed in the EN output:
```powershell
Select-String -Path dist/index.html -Pattern 'rel="alternate"','SoftwareApplication','og:image' | Select-Object -ExpandProperty Line | Select-Object -First 6
```
Expected: lines containing `hreflang="x-default"`, `hreflang="ru-RU"`, `SoftwareApplication`, and `og:image`.

- [ ] **Step 6: Commit**

```powershell
git add -A
git commit -m "feat(layout): Base.astro with hreflang alternates, JSON-LD, OG tags, no-FOUC theme init"
```

---

## Task 4: Global CSS + theme tokens (design via `ui-ux-pro-max`)

**Files:**
- Modify: `src/styles/global.css`

**Interfaces:**
- Produces: CSS custom properties (`--accent`, `--accent-from`, `--accent-to`, `--bg`, `--text`, `--muted`, radii, spacing scale) consumed by every component. `[data-theme="light|dark"]` selectors.

- [ ] **Step 1: Consult `ui-ux-pro-max` for the visual pass**

Load the `ui-ux-pro-max` skill (via the `skill` tool) and query for: product type "developer/utility desktop tool", a style direction matching the brand (calm, honest, premium-minimal — Ollama/LocalSend/Vercel reference), and a font pairing. Adopt its recommended scale/radii/spacing within the brand constraints (Global Constraints: accent = icon blue gradient). Document the chosen tokens as CSS variables.

- [ ] **Step 2: Write `global.css` tokens**

```css
:root {
  --accent-from: #5EB3E6;   /* icon sky */
  --accent-to:   #1E5A8E;   /* icon navy */
  --accent:      #2C83C2;   /* mid blue, 4.5:1 on white for text */
  --grad: linear-gradient(135deg, var(--accent-from), var(--accent-to));
  --radius: 14px;
  --radius-sm: 8px;
  --maxw: 1120px;
  --space: clamp(1rem, 2vw, 1.5rem);
  --font-ui: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, "Cascadia Code", "SF Mono", Menlo, Consolas, monospace;
  color-scheme: dark light;
}
[data-theme="light"] {
  --bg: #ffffff; --bg-elev: #f7f9fc; --text: #0f172a; --muted: #4B5563; --border: #e5e7eb;
}
[data-theme="dark"] {
  --bg: #0a1220; --bg-elev: #0f1a2e; --text: #e6edf6; --muted: #93a4bd; --border: #1f2c44;
}
* { box-sizing: border-box; }
html { scroll-behavior: smooth; }
body { margin: 0; background: var(--bg); color: var(--text); font-family: var(--font-ui); line-height: 1.6; }
@media (prefers-reduced-motion: reduce) { html { scroll-behavior: auto; } *, *::before, *::after { animation-duration: .001ms !important; transition-duration: .001ms !important; } }
h1, h2, h3 { line-height: 1.15; }
a { color: var(--accent); }
img { max-width: 100%; height: auto; }
.container { max-width: var(--maxw); margin: 0 auto; padding: 0 var(--space); }
```
Add further base/utilities as `ui-ux-pro-max` recommends, but keep it under one file.

- [ ] **Step 3: Build gate**

Run: `npm run build`
Expected: exits 0; CSS bundled.

- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "feat(css): design tokens + light/dark themes + reduced-motion (ui-ux-pro-max)"
```

---

## Task 5: Brand/icon assets (favicon, OG, apple-touch from `src-tauri/icons`)

**Files:**
- Create: `public/favicon.svg`, `public/favicon.ico`, `public/apple-touch-icon.png`, `public/og-default.png`
- Source: app repo `src-tauri/icons/icon.png` (master), plus `32x32.png`, `128x128.png`, `128x128@2x.png`.

**Interfaces:**
- Produces: the OG/favicon assets referenced by `Base.astro` (Task 3).

- [ ] **Step 1: Copy the master icon into the website repo**

```powershell
# from ../molvi-website
Copy-Item "..\molvi\src-tauri\icons\icon.png" "public\icon-master.png"
Copy-Item "..\molvi\src-tauri\icons\128x128.png" "public\apple-touch-icon.png"   # ~128 is fine; 180 ideal — resize if needed
```

- [ ] **Step 2: Generate `favicon.ico`, `favicon.svg`, and the 1200×630 `og-default.png`**

Use Sharp (already an Astro dep) via a one-off script `scripts/gen-assets.mjs`:
```js
import sharp from 'sharp';
const src = 'public/icon-master.png';
await sharp(src).resize(32, 32).toFile('public/favicon-32.png');
await sharp(src).resize(180, 180).toFile('public/apple-touch-icon.png');
// OG card: 1200x630, icon centered on the blue gradient with the tagline burned in (or just gradient + icon for v1).
await sharp({ create: { width: 1200, height: 630, channels: 4, background: { r: 30, g: 90, b: 142 } } })
  .composite([{ input: src, gravity: 'center' }]).png().toFile('public/og-default.png');
```
Run: `node scripts/gen-assets.mjs`. (A real `favicon.ico` requires an ico encoder; if Sharp's ico output is unavailable, ship `favicon-32.png` + a hand-authored `favicon.svg`, and reference both — modern browsers accept PNG/SVG favicons.) Author `public/favicon.svg` as a simple gradient rounded-square with the white silhouette (trace or reuse if an SVG master exists; otherwise a gradient square + "m" glyph as a v1).

- [ ] **Step 3: Build + assertion gate**

Run: `npm run build`
```powershell
Test-Path dist/og-default.png, dist/apple-touch-icon.png, dist/favicon.svg
```
Expected: all `True`.

- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "feat(assets): favicon, apple-touch, OG card derived from src-tauri/icons/icon.png"
```

---

## Task 6: Nav + LocaleSwitcher + ThemeToggle + GitHub button

**Files:**
- Create: `src/components/Nav.astro`, `LocaleSwitcher.astro`, `ThemeToggle.astro`, `src/scripts/theme-toggle.ts`, `src/scripts/nav.ts`

**Interfaces:**
- Consumes: `LOCALES`, `LANG_ATTR`, `APP_REPO_URL`.
- Produces: `<Nav locale>` rendered at top of every page.

- [ ] **Step 1: `ThemeToggle.astro` + `theme-toggle.ts`**

`ThemeToggle.astro` — a 3-way button (system/light/dark). `theme-toggle.ts` cycles the value, persists `molvi-theme` in `localStorage`, sets `data-theme` on `<html>` (resolving "system" against `matchMedia`).

- [ ] **Step 2: `LocaleSwitcher.astro`**

A `<select>` (or popover) listing all 6 locales, linking to the same section on the sibling locale page. Persist last choice in `localStorage`. Use `LANG_ATTR` for display. **Only link locales that are "ready"** — gate by a `READY_LOCALES` set in `config.ts` (initially `['en','ru']`; expand in Task 20).

- [ ] **Step 3: `Nav.astro`**

Sticky, blurred on scroll. Left: logo (icon). Center/right: anchor links (`#how`, `#features`, `#privacy`, `#download`) from `t(locale,'nav.*')`. Right: GitHub button (→ `APP_REPO_URL`, optional live star count via the GitHub API at build time — see Task 14), `<LocaleSwitcher>`, `<ThemeToggle>`. Mobile: hamburger → `nav.ts` toggles a disclosure.

- [ ] **Step 4: Build + assertion gate**

Run: `npm run build`
```powershell
Select-String -Path dist/index.html -Pattern 'id="nav"','aria-label','ThemeToggle|data-theme-toggle'
```
Expected: nav present; only ready locales linked.

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(nav): sticky nav + locale switcher + 3-way theme toggle + GitHub button"
```

---

## Task 7: Hero (OS-detected download CTA)

**Files:**
- Create: `src/components/Hero.astro`, `src/scripts/os-detect.ts`
- Modify: each page wrapper to render `<Hero locale>`.

**Interfaces:**
- Consumes: `t(locale,'hero.*')`, `OS_ASSETS`, `assetUrl`, `RELEASES_URL`, `INSTALLER_SIZE`.

- [ ] **Step 1: `os-detect.ts`**

```ts
// Detects OS from navigator.userAgent and sets the primary download button.
type OS = 'windows' | 'mac' | 'linux' | 'other';
export function detectOs(ua: string): OS {
  if (/Win/.test(ua)) return 'windows';
  if (/Mac/.test(ua)) return 'mac';
  if (/Linux/.test(ua) && !/Android/.test(ua)) return 'linux';
  return 'other';
}
```
On `DOMContentLoaded`, read `detectOs(navigator.userAgent)`, set the primary CTA's `href` to `assetUrl(OS_ASSETS[os])` and its label to `t(...) 'Download for {os}'` (Windows/macOS/Linux; "other" → label "Download", href → `RELEASES_URL`).

- [ ] **Step 2: `Hero.astro`**

Markup: `<h1>{t(locale,'hero.h1')}</h1>`, `<p>{t(locale,'hero.sub')}</p>`, primary `<a id="download-primary">` (enhanced by os-detect), secondary `<a href={APP_REPO_URL}>View on GitHub ★</a>`, microcopy `<p>{t(...,'hero.micro')}</p>`. Hero visual: `<img>` of the overlay (Task 16; until then, the imported `overlay.png`). Set `width`/`height` (no CLS). Provide a `<noscript>` fallback link to `RELEASES_URL` so the CTA works without JS.

- [ ] **Step 3: Build + assertion gate**

Run: `npm run build`
```powershell
Select-String -Path dist/index.html -Pattern 'id="download-primary"','os-detect','<noscript>'
```
Expected: all present.

- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "feat(hero): OS-detected download CTA + GitHub CTA + hero image"
```

---

## Task 8: TrustStrip + HowItWorks (3 sequential steps)

**Files:**
- Create: `src/components/TrustStrip.astro`, `HowItWorks.astro`

- [ ] **Step 1: `TrustStrip.astro`** — `t('trust.zero')` + `t('trust.builtwith')` badges (render the tech names verbatim; they are NOT localized). One row, centered.

- [ ] **Step 2: `HowItWorks.astro`** — `id="how"`. Three numbered steps (Hold hotkey → Speak → Text lands), from `t('how.step1'…'step3')`. **Not** a bento — a vertical/ordered list. Reference `onboarding.png` (step 1) and `overlay.png` (step 2) once Task 16 imports them.

- [ ] **Step 3: Build gate + commit**

```powershell
npm run build
git add -A
git commit -m "feat(sections): trust strip + 3-step how-it-works"
```

---

## Task 9: Three FeatureRow deep-dives (alternating image/text)

**Files:**
- Create: `src/components/FeatureRow.astro` (generic)

**Interfaces:** `<FeatureRow locale flip? image title body bullets cta? />` — reused 3×.

- [ ] **Step 1: Write the generic `FeatureRow.astro`** — two-column grid; `flip` swaps image/text order (use `direction: rtl` flip OR CSS `grid-template-columns` order — keep logical properties). Image on one side, `<h2>` + `<p>` + `<ul>` on the other.

- [ ] **Step 2: Render the three rows on the page**
  - **Row A — Private by design** (`id="privacy-intro"`): title `t('privacy.lead')`, body `t('privacy.sub')`, bullets from README Privacy section, image = a privacy motif (gradient card or the icon). CTA → `#privacy`.
  - **Row B — Speaks your language**: title `t('features.lang.title')`, body `t('features.lang.body')`, callout `<strong>{t('russian.claim')}</strong>`, image = `settings.png` (recognition section). CTA → `#languages`.
  - **Row C — More than dictation**: title `t('features.more.title')`, bullets (command mode, per-app profiles, dictionary, snippets, Smart/Polished), image = `settings.png`. CTA → `#features`.

- [ ] **Step 3: Build gate + commit**

```powershell
npm run build
git add -A
git commit -m "feat(features): three alternating feature rows (privacy / languages / more)"
```

---

## Task 10: Capabilities Bento (one section)

**Files:**
- Create: `src/components/Bento.astro`

- [ ] **Step 1: `Bento.astro`** — `id="features"`. CSS grid bento; **one anchor cell** (the headline capability, e.g. "Works in any app") ≥1.5× the others. Cells: Snippets, Dictionary, Local history, Custom hotkey, Overlay, Auto-updater. Each cell = icon + `t('bento.*.title')` + one-line `t('bento.*.desc')`. Gutter ≈ half inner padding. Keep it to ONE bento section (anti-pattern: multiple decorative bento walls).

- [ ] **Step 2: Build gate + commit**

```powershell
npm run build
git add -A
git commit -m "feat(bento): single capabilities bento with anchor cell"
```

---

## Task 11: Privacy + Languages + Performance + Comparison + Limitations

**Files:**
- Create: `Privacy.astro`, `Languages.astro`, `Performance.astro`, `Comparison.astro`, `Limitations.astro`, `src/data/comparison.ts`

- [ ] **Step 1: `Privacy.astro`** (`id="privacy"`) — `t('privacy.lead')`, `t('privacy.sub')`, `t('privacy.verify')`, a checklist of README's verifiable claims, and a link to the app repo's `tests/log_privacy.rs` (`${APP_REPO_URL}/blob/main/src-tauri/tests/log_privacy.rs`). Add a small note that the only outbound call is the optional update check.

- [ ] **Step 2: `Languages.astro`** (`id="languages"`) — coverage: 36 UI (incl. ar/he RTL) · 40+ recognition locales. Render recognition locales as a compact grid (the 40 from `recognition.ts` — copy the list as data). Highlight Russian as "dedicated engine (GigaAM-v3)".

- [ ] **Step 3: `Performance.astro`** — RTF table + cold-start + installer size, lifted from README, with the "indicative, varies with utterance length" note verbatim.

- [ ] **Step 4: `Comparison.astro`** — `src/data/comparison.ts` holds the README comparison table as data (products, rows, cells). Render as a responsive table; Handy row captioned "kin, not a rival" per README.

- [ ] **Step 5: `Limitations.astro`** — the README's "Known limitations" list, verbatim in spirit (Nemotron commas-only, unsigned installers, Apple Silicon only, Wayland caveat, desktop only, BYO-LLM polish).

- [ ] **Step 6: Build gate + commit**

```powershell
npm run build
git add -A
git commit -m "feat(content): privacy, languages, performance, comparison, limitations sections"
```

---

## Task 12: Download / platforms section

**Files:**
- Create: `src/components/Download.astro`

**Interfaces:** Consumes `OS_ASSETS`, `assetUrl`, `RELEASES_URL`, `VERSION`, `INSTALLER_SIZE`.

- [ ] **Step 1: `Download.astro`** (`id="download"`) — three OS cards (Windows 11 / macOS Apple Silicon / Linux). The visitor's OS card gets `data-active` (set by `os-detect.ts`). Each card: OS name, asset filename, `INSTALLER_SIZE`, version `VERSION`, "Download" button → `assetUrl(...)`, "changelog" link → `RELEASES_URL`. **Honest note** (visible): "v0.1.0 is built and signed for updates but not yet published — grab it from the Releases page" → link `RELEASES_URL`. Add the SmartScreen/Gatekeeper bypass one-liners from README.

- [ ] **Step 2: Resilience** — `os-detect.ts` (Task 7) also sets `data-active` on the matching card. If a release asset 404s (not published), a `<noscript>` + server-rendered fallback keeps every card linking to `RELEASES_URL`.

- [ ] **Step 3: Build gate + commit**

```powershell
npm run build
git add -A
git commit -m "feat(download): 3 OS cards + active-OS highlight + honest 'not yet published' note"
```

---

## Task 13: FAQ accordion + FAQPage JSON-LD

**Files:**
- Create: `src/data/faq.ts`, `src/components/Faq.astro`, `src/scripts/faq.ts`

- [ ] **Step 1: `faq.ts`** — six entries keyed per locale (use the 6 FAQ Q&A from spec §5). `type Faq = { q: string; a: string }`.

- [ ] **Step 2: `Faq.astro`** — accordion (`<details>/<summary>` for no-JS baseline, enhanced by `faq.ts`). Inject `FAQPage` JSON-LD built from the locale's entries:
```astro
<script type="application/ld+json" set:html={JSON.stringify({ '@context':'https://schema.org','@type':'FAQPage', mainEntity: faqs.map(f => ({ '@type':'Question', name:f.q, acceptedAnswer:{ '@type':'Answer', text:f.a } })) })} />
```

- [ ] **Step 3: Build + assertion gate**

```powershell
npm run build
Select-String -Path dist/index.html -Pattern 'FAQPage','acceptedAnswer'
```
Expected: matches.

- [ ] **Step 4: Commit**

```powershell
git add -A
git commit -m "feat(faq): accordion + FAQPage JSON-LD"
```

---

## Task 14: OSS CTA + Footer (+ optional build-time star count)

**Files:**
- Create: `src/components/OssCta.astro`, `Footer.astro`

- [ ] **Step 1: `OssCta.astro`** — "Star on GitHub" (→ `APP_REPO_URL`), "Contribute" (→ `CONTRIBUTING.md`), license `MIT OR Apache-2.0`, Roadmap (→ README roadmap anchor), Acknowledgements (GigaAM/Nemotron/transcribe-rs/parakeet-rs/ort/Tauri + Handy "kin" note).

- [ ] **Step 2: Optional GitHub star count** — fetch at build time (SSG) inside `OssCta.astro`'s frontmatter:
```ts
let stars: number | null = null;
try { const r = await fetch('https://api.github.com/repos/bumbaRasch/molvi'); if (r.ok) stars = (await r.json()).stargazers_count; } catch {}
```
Render `★ {stars}` only if `stars !== null && stars > 0`. **Never** invent a number (Global Constraints).

- [ ] **Step 3: `Footer.astro`** — three columns (Product / Resources / Community), each filled (no empty columns). `Made by @bumbaRasch` · © 2026 · `MIT OR Apache-2.0`. Repeat `<LocaleSwitcher>` + `<ThemeToggle>` for mobile.

- [ ] **Step 4: Build gate + commit**

```powershell
npm run build
git add -A
git commit -m "feat(oss/footer): star CTA, acknowledgements, filled footer columns"
```

---

## Task 15: Sitemap + robots + locale wrappers + per-locale pages

**Files:**
- Create: `public/robots.txt`, `src/pages/{ru,es,de,fr,zh}/index.astro`, modify `src/pages/index.astro`

- [ ] **Step 1: `public/robots.txt`**
```
User-agent: *
Allow: /
Sitemap: https://bumbaRasch.github.io/molvi-website/sitemap-index.xml
```

- [ ] **Step 2: Author the EN page `src/pages/index.astro`** — import all components, render `<Base locale="en" ...>` with `<Nav>`, `<Hero>`, `<TrustStrip>`, `<HowItWorks>`, three `<FeatureRow>`s, `<Bento>`, `<Privacy>`, `<Languages>`, `<Performance>`, `<Comparison>`, `<Limitations>`, `<Download>`, `<Faq>`, `<OssCta>`, `<Footer>`. Pass `locale="en"` to each.

- [ ] **Step 3: Author the 5 locale wrappers** — `src/pages/ru/index.astro` etc. Each is ~10 lines: same component sequence, `locale="ru"` (etc.). EN copy renders until Task 19/20 translate them.

- [ ] **Step 4: Build + assertion gate**

```powershell
npm run build
# 6 locale HTML files + sitemap with hreflang
Test-Path dist/index.html, dist/ru/index.html, dist/es/index.html, dist/de/index.html, dist/fr/index.html, dist/zh/index.html
Select-String -Path dist/sitemap-0.xml -Pattern 'hreflang','ru-RU','zh-CN'
```
Expected: all 6 pages exist; sitemap contains hreflang alternates for all locales.

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat(pages): EN canonical + 5 locale wrappers; sitemap + robots with hreflang"
```

---

## Task 16: Screenshot asset pipeline (`astro:assets`, AVIF/WebP)

**Files:**
- Create: `src/assets/overlay.png`, `settings.png`, `onboarding.png` (copied from app repo `docs/img/`)
- Modify: Hero, HowItWorks, FeatureRow to import via `astro:assets`.

- [ ] **Step 1: Copy screenshots**
```powershell
Copy-Item "..\molvi\docs\img\overlay.png" "src\assets\overlay.png"
Copy-Item "..\molvi\docs\img\settings.png" "src\assets\settings.png"
Copy-Item "..\molvi\docs\img\onboarding.png" "src\assets\onboarding.png"
```

- [ ] **Step 2: Import through `astro:assets`** in components:
```astro
---
import { Image } from 'astro:assets';
import overlay from '../assets/overlay.png';
const overlayImg = await getImage({ src: overlay, widths: [480, 960, 1440], formats: ['avif','webp'] });
---
<Image src={overlay} alt={t(locale,'hero.img.alt')} width={960} height={540} loading="eager" />
```
This emits optimized AVIF/WebP with reserved dimensions (no CLS). Below-the-fold images: `loading="lazy"`. Always include descriptive `alt`.

- [ ] **Step 3: Build + assertion gate**
```powershell
npm run build
Get-ChildItem -Recurse dist\_astro | Where-Object { $_.Extension -in '.avif','.webp' } | Measure-Object | Select-Object -ExpandProperty Count
```
Expected: > 0 optimized variants.

- [ ] **Step 4: Commit**
```powershell
git add -A
git commit -m "feat(assets): screenshots via astro:assets (AVIF/WebP, no-CLS, lazy)"
```

---

## Task 17: Performance / a11y / reduced-motion pass (with `seo` skill)

**Files:**
- Modify: components/global.css as flagged by the audit.

- [ ] **Step 1: Consult the `seo` skill** (Addy Osmani/web-quality) — load it and run its on-page audit guidance against the built site.

- [ ] **Step 2: Run Lighthouse mobile** (CI-friendly):
```powershell
npx --yes lighthouse https://bumbaRasch.github.io/molvi-website/ --preset=desktop --quiet --output=json --output-path=lh.json
# and a mobile run without --preset=desktop
```
(If the site isn't deployed yet, `npm run preview` and Lighthouse `http://localhost:4321/molvi-website/`.)
Targets: Performance ≥ 95, Accessibility ≥ 95, Best Practices = 100, SEO ≥ 95.

- [ ] **Step 3: Fix the common offenders**: add missing `alt`, ensure every interactive element has a visible focus ring (`:focus-visible`), tap targets ≥ 44×44, ensure the hero GIF (when added) is replaced by a static image under `prefers-reduced-motion` (the global.css reduce block + a `<noscript><img>` pair), preload the hero image with `<link rel="preload" as="image" ...>` in `Base.astro`.

- [ ] **Step 4: Commit**
```powershell
git add -A
git commit -m "perf(a11y): lighthouse pass — alt text, focus rings, tap targets, reduced-motion, preload"
```

---

## Task 18: GitHub Pages deploy workflow

**Files:**
- Create: `.github/workflows/deploy.yml`

- [ ] **Step 1: Write the workflow (verified `withastro/action@v6`, `actions/checkout@v7`, `actions/deploy-pages@v5`)**

```yaml
name: Deploy to GitHub Pages
on:
  push:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: read
  pages: write
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: withastro/action@v6
  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v5
```

- [ ] **Step 2: Validate YAML**
```powershell
npx --yes --package=yaml -- yaml-cli lint .github/workflows/deploy.yml 2>$null
# fallback: just ensure node can parse it
```
(Any reliable YAML parse is fine.)

- [ ] **Step 3: Commit**
```powershell
git add -A
git commit -m "ci: deploy to GitHub Pages via withastro/action"
```
> After the first push to `main`, enable Pages (Settings → Pages → Source: GitHub Actions). The site appears at `https://bumbaRasch.github.io/molvi-website/`.

---

## Task 19: RU full reviewed translation (the one non-EN native pass)

**Files:**
- Modify: `src/i18n/locales/ru.ts`, `src/data/faq.ts` (ru entries)

- [ ] **Step 1: Translate every EN key into RU**, reviewed by a native speaker. The hero/privacy/FAQ Russian copy from the spec is the seed:
  - `hero.h1`: «Говорите. Текст появляется. Ничего не уходит.»
  - `hero.sub`: «molvi превращает ваш голос в текст в любом приложении — распознавание прямо на вашем устройстве, 36 языков интерфейса, отдельный движок для русского. Бесплатно и с открытым исходным кодом для Windows, macOS и Linux.»
  - `privacy.lead`: «Приватность по конструкции, а не по обещанию.»
  - `privacy.sub`, `privacy.verify`, `russian.claim`, all `faq.*` — translate fully.
  - Keep proper nouns verbatim: GigaAM, Nemotron, Tauri, ONNX, Rust, GitHub, Windows/macOS/Linux. Keep `{os}`, `{n}` tokens ASCII.

- [ ] **Step 2: Key-parity + build**
```powershell
npx tsx scripts/check-i18n.mjs
npm run build
```
Expected: parity OK; build green.

- [ ] **Step 3: Mark RU ready**
In `config.ts` set `READY_LOCALES = ['en','ru']` (LocaleSwitcher now links RU).

- [ ] **Step 4: Commit**
```powershell
git add -A
git commit -m "feat(i18n): full reviewed RU translation; RU linked in switcher"
```

---

## Task 20: ES / DE / FR / ZH — translate-or-defer

**Files:**
- Modify: `src/i18n/locales/{es,de,fr,zh}.ts`, `src/data/faq.ts`

- [ ] **Step 1: For each of es/de/fr/zh** — either (a) translate + review all keys (then add to `READY_LOCALES`), or (b) leave as EN-fallback with the TODO marker and **do not add to `READY_LOCALES`**. The Global Constraint is absolute: no unreviewed locale is linked in the switcher.

- [ ] **Step 2: For each locale that IS translated** — update `READY_LOCALES` and rebuild:
```powershell
npx tsx scripts/check-i18n.mjs
npm run build
```

- [ ] **Step 3: Commit**
```powershell
git add -A
git commit -m "feat(i18n): add reviewed <locales> to switcher (deferred ones remain EN-fallback, unlinked)"
```

---

## Self-Review (against spec)

**Spec coverage** (spec section → task):
- §1 Goals/non-goals → constraints header + no testimonials anywhere ✓
- §2 Stack & architecture → Tasks 1, 3, 4 ✓
- §2.1 Screenshots → Task 16 ✓
- §2.2 Brand/icon assets → Task 5 ✓
- §3 Locales & i18n → Tasks 2, 19, 20 ✓
- §4 §1–§15 sections → Tasks 6–14, 15 ✓
- §5 Copy (canonical EN) → Task 2 (`en.ts` authored from §5) ✓
- §6 Visual/brand (`ui-ux-pro-max`) → Task 4 ✓
- §7 SEO (hreflang/OG/JSON-LD/sitemap/robots/canonical) → Tasks 3, 13, 15 ✓
- §8 Perf & a11y (`seo`) → Task 17 ✓
- §9 Deployment → Task 18 ✓
- §10 Acceptance criteria → covered by per-task gates + Task 17 ✓
- §11 Open follow-ups → GIF is deferred (Hero uses static `overlay.png` until recorded); RU done in Task 19; domain deferred ✓

**Placeholder scan:** the only "…"/"etc." are inside `en.ts`/`faq.ts` illustrating where the full key set is authored — these are explicit author-this-here instructions, not TBDs. `os-detect.ts`, `theme-init.ts`, `astro.config.mjs`, `deploy.yml`, the JSON-LD, and the sitemap config all carry real, verified code. No "implement later"/"handle edge cases" stubs.

**Type/identifier consistency:** `t(locale, key, vars?)` signature is constant across Tasks 2/3/6/7/9/13. `LOCALES`, `LANG_ATTR`, `DEFAULT_LOCALE`, `READY_LOCALES` defined in Task 2/config. `OS_ASSETS`/`assetUrl`/`RELEASES_URL`/`VERSION`/`INSTALLER_SIZE` defined in Task 3 (`site.ts`) and consumed in Tasks 7/12. `detectOs()` defined in Task 7, reused in Task 12. `<Base locale title description>` props constant across wrappers (Task 15).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-07-molvi-website.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using executing-plans, batch with checkpoints.

Which approach?
