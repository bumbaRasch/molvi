# molvi updater release pipeline — design spec

> **Status:** design (brainstorming → spec). **Date:** 2026-08-07.
> **Scope:** unblock releases by wiring the Tauri 2 updater end-to-end:
> ed25519 signing + GitHub Releases endpoint + a `tauri-action` release
> workflow. **Updater-only** (OS code-signing = separate later effort).

## Goal

molvi ships v0.1.0 on Windows only. The multi-platform port (PR #1) is
code-complete + all-3-OS CI-green, but **no release pipeline exists** and the
updater's `pubkey`/`endpoint` in `tauri.conf.json` are placeholders. Without
this work: no auto-updates on any platform, no installable Mac/Linux artifacts,
no versioned releases. This spec delivers the minimal, free, blaze-safe
pipeline that makes a release *possible* (cutting one is a manual step).

## Scope (decision: updater-only, free, now)

**In scope:**
- ed25519 keypair generation (free, self-generated).
- Wire the real public key + GitHub Releases endpoint into `tauri.conf.json`.
- Store the private key as a GitHub Actions secret.
- A `release.yml` workflow (`tauri-action@v1`) that builds all 3 platforms,
  signs the updater artifacts, and creates a **draft** GitHub Release with
  `latest.json` + per-platform installers + `.sig` files.

**Out of scope (documented paths, separate effort):**
- **OS code-signing** (macOS Developer ID notarization $99/yr; Windows Azure
  Trusted Signing ~$10/mo or Authenticode cert). Without it: macOS Gatekeeper
  blocks unsigned apps (right-click → Open workaround); Windows SmartScreen
  warns (but runs). Verified via `cjpais/Handy` — they pay for BOTH (Apple
  Developer ID cert + Azure Trusted Signing). There is no free Gatekeeper/
  SmartScreen bypass. The updater mechanism works regardless of OS signing.
- **Cutting an actual release** — this spec delivers the *capability*; the
  first release is a manual `workflow_dispatch` + draft-publish, done when the
  human is ready (after the Mac/Linux runtime smokes, or accepting the
  unsigned risk).

## Verified facts (ctx7 `/websites/v2_tauri_app` + `tauri-apps/tauri-action`, 2026-08-07)

1. **`tauri-apps/tauri-action@v1`** is the current stable (`gh api
   repos/tauri-apps/tauri-action/tags` → `v1.0.0`, `v1`). The official docs
   use `@v1`; `cjpais/Handy` pins `@v0` (stale). molvi uses `@v1`.
2. **`latest.json` auto-generation:** tauri-action generates `latest.json`
   (`includeUpdaterJson` defaults `true`) with platform-specific entries +
   signature refs, uploaded as a release asset. **CRITICAL:** if no `.sig`
   files are found, the action **skips the updater-JSON upload entirely**.
   → ed25519 signing is *required*, not optional. molvi signs → OK.
3. **`tauri signer generate`:** `npx tauri signer generate -w ~/.tauri/molvi.key`
   (or `npm run tauri signer generate -- -w …`). Writes the private key to the
   file; prints the public key to stdout (also writes `<path>.pub`).
   Options: `-p <password>` (key password), `-w <path>` (private key file),
   `-f` (overwrite), `--ci` (no prompts). The public-key string goes verbatim
   into `tauri.conf.json` `plugins.updater.pubkey`.
4. **Build signing env:** `TAURI_SIGNING_PRIVATE_KEY` (private-key content OR
   path) + optional `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. A standard `cargo
   tauri build` / tauri-action build with these set produces the updater
   artifacts + their `.sig` signatures. `.env` files are NOT supported — env
   vars only.
5. **Endpoint:** `https://github.com/bumbaRasch/molvi/releases/latest/download/
   latest.json` — the standard GitHub Releases pattern. The updater queries it,
   compares versions, downloads the per-platform signed artifact, verifies the
   ed25519 signature against the embedded pubkey, installs, restarts.
6. **macOS target:** `macos-14` IS Apple Silicon — its default Rust host triple
   is `aarch64-apple-darwin`, so **no `--target` arg** is needed (unlike the
   generic docs example that uses `macos-latest` + cross-targets). Matches
   molvi's CI.
7. **Ubuntu deps:** molvi uses `libayatana-appindicator3-dev` (maintained
   fork), NOT the generic `libappindicator3-dev` from the tauri-action
   example. The release workflow mirrors molvi's proven CI dep set exactly
   (ci.yml:47) — including `libasound2-dev` (cpal ALSA).

## Architecture (4 components, zero new app code)

```
[1] Key generation (one-time, local)   → private key (secret) + public key
[2] tauri.conf.json (2 value swaps)    → real pubkey + GitHub endpoint
[3] GitHub secret (repo settings)      → TAURI_SIGNING_PRIVATE_KEY = private key
[4] .github/workflows/release.yml (NEW)→ tauri-action@v1: build + sign + draft release
```

The existing app code is **untouched**: `updater.rs` (`check`/`apply` via
`tauri_plugin_updater::UpdaterExt`), the IPC commands (`check_update`/
`apply_update`), the frontend Updates section, and the `check_on_startup`
setting all read `pubkey`/`endpoints` from `tauri.conf.json` at runtime — they
work the moment the placeholders are replaced. No Rust/TS edit.

## Component detail

### [1] Key generation (one-time, local)

Run once on the dev machine (NOT in CI):
```
npx tauri signer generate -w ~/.tauri/molvi.key
```
- Writes the private key to `~/.tauri/molvi.key`; prints the public key (also
  writes `~/.tauri/molvi.key.pub`).
- **No password** (`-p` omitted) for v1 simplicity — the private key is itself
  a high-entropy secret; GitHub's encrypted secret store is the protection
  layer. (A password can be added later: regenerate + add
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; not v1-blocking.)
- **Backup the private-key file** (password manager / offline). Losing it =
  installed copies can no longer update (the public key baked into them won't
  verify a different signing key). It is the update-trust root.
- **Never commit** the private key (add `~/.tauri/` is outside the repo; the
  file never enters git).

### [2] tauri.conf.json — 2 value swaps (in `plugins.updater`)

```jsonc
"plugins": {
  "updater": {
    "pubkey": "<PASTE THE PUBLIC KEY PRINTED BY `signer generate`>",
    "endpoints": [
      "https://github.com/bumbaRasch/molvi/releases/latest/download/latest.json"
    ],
    "windows": {
      "installMode": "passive"
    }
  }
}
```
`bundle.createUpdaterArtifacts: true` is already set (tauri.conf.json:64) —
unchanged. `windows.installMode: "passive"` is already correct — unchanged.

### [3] GitHub secret (repo Settings → Secrets and variables → Actions)

- **Name:** `TAURI_SIGNING_PRIVATE_KEY`
- **Value:** the contents of `~/.tauri/molvi.key` (the private key).

(No password secret for v1 — see [1].)

### [4] `.github/workflows/release.yml` (NEW)

```yaml
name: Release

on:
  workflow_dispatch:

jobs:
  release:
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: windows-latest
          - os: macos-14        # Apple Silicon (aarch64-apple-darwin default host)
          - os: ubuntu-latest   # x86_64-unknown-linux-gnu
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7

      - name: Install Rust (stable)
        uses: dtolnay/rust-toolchain@stable

      - name: Rust cache
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - name: Setup Node
        uses: actions/setup-node@v7
        with:
          node-version: '24'
          cache: 'npm'

      - name: Install JS deps
        run: npm ci

      # Mirrors molvi's CI dep set EXACTLY (ci.yml:47) — webkit2gtk-4.1 (Tauri 2),
      # libayatana-appindicator3-dev (maintained fork, NOT generic libappindicator3),
      # libasound2-dev (cpal ALSA), + build tooling. Proven to compile molvi.
      - name: Install Linux system deps (Tauri 2 + ALSA)
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev pkg-config

      - uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
        with:
          tagName: v__VERSION__           # __VERSION__ ← tauri.conf.json version
          releaseName: 'molvi v__VERSION__'
          releaseBody: 'See the assets to download and install.'
          releaseDraft: true              # NOT public until manually published
          prerelease: false
```

Design notes:
- **Trigger `workflow_dispatch`** (manual button), not tag-push — avoids
  tag/version mismatch (the version comes from `tauri.conf.json`, bumped in a
  commit before clicking Run). Matches `cjpais/Handy`'s release trigger.
- **No `--target` args** — `macos-14`'s default host is `aarch64-apple-darwin`
  (Apple Silicon), matching molvi's Intel-unsupported stance (D3). ubuntu/
  windows defaults are correct.
- **`releaseDraft: true`** — the release is created as a draft. `latest.json`
  is NOT publicly downloadable while drafted → the updater finds nothing until
  the draft is manually published. This is the safety gate: unverified Mac/Linux
  builds never auto-publish.
- **`TAURI_SIGNING_PRIVATE_KEY` in `env`** — its presence makes the build emit
  `.sig` files, which tauri-action then references in `latest.json`. Without
  it, tauri-action skips the `latest.json` upload (verified fact #2).
- **Action versions** mirror molvi's CI (`checkout@v7`, `rust-toolchain@stable`,
  `rust-cache@v2`, `setup-node@v7`) for consistency.

## Release flow (how a release happens)

1. Bump `version` in `tauri.conf.json` (e.g. `0.1.0` → `0.2.0`); commit + push.
2. GitHub → Actions → **"Release"** workflow → **Run workflow** (manual button).
3. The matrix builds all 3 platforms in parallel, signs artifacts, creates ONE
   **draft** release ( tagName `v0.2.0`) with `latest.json` + NSIS/MSI +
   `.dmg`/`.app` + `.deb`/`.rpm`/`.AppImage` + their `.sig` files.
4. Open the draft release; verify `latest.json` + `.sig` files + per-platform
   installers are present.
5. **Publish** the draft → `latest.json` becomes public → the running app's
   "Check for updates" (Updates settings section) finds `v0.2.0`, downloads
   the signed platform artifact, verifies the ed25519 signature, installs,
   restarts.

The draft can sit indefinitely — publish after the Mac/Linux runtime smokes
pass, or whenever the unsigned-build risk is accepted.

## Safety / blaze / privacy

- **Blaze:** zero app-code changes. The dictation hot loop (capture→engine→
  finalize→paste) is byte-untouched; RTF ≤ 0.03 unaffected. The release
  workflow is offline from the runtime; builds are opt-in (`workflow_dispatch`).
- **Privacy §10.1:** the private key is a GitHub Actions secret (masked in
  logs). Version strings + endpoint URLs are metadata. No transcript/audio/
  settings content touches the release pipeline.
- **Update-trust root:** the ed25519 private key is the sole updater trust
  root. Compromise = a malicious "update" could be signed. Mitigation: GitHub
  secret (encrypted at rest, masked in logs), no password for v1 simplicity
  (the key is the secret). Backup offline; rotate if compromised (regenerate +
  bump pubkey → installed copies update to the new-key version, then trust the
  new key for subsequent updates — the Tauri updater supports key rotation via
  the pubkey in the new release's config).
- **Draft gate:** unverified Mac/Linux builds are never auto-published.

## Verification gates

- **Key + config:** after `[1]`+`[2]`+`[3]`, `npx tsc --noEmit && npm run build`
  pass (config-only change); the app's Updates section "Check now" hits the
  real endpoint (returns "up to date" or a 404 until the first release — both
  handled gracefully by `updater.rs::check`).
- **Workflow dry-run:** the first `workflow_dispatch` produces a draft release
  with `latest.json` + `.sig` + per-platform installers. Inspect the draft
  assets (do NOT publish yet).
- **End-to-end:** publish the draft; on a machine running an OLDER molvi,
  "Check for updates" → finds the new version → "Apply" → downloads, verifies
  signature, installs, restarts into the new version.
- **CI unchanged:** the existing `.github/workflows/ci.yml` gates (fmt +
  clippy + test --lib + tsc + build, all 3 OSes) remain the per-push gate;
  `release.yml` is an additional, opt-in workflow.

## Out of scope — OS code-signing (documented for a later effort)

When molvi is ready for polished distribution (no Gatekeeper/SmartScreen
warnings), the path — verified via `cjpais/Handy` (`build.yml`):
- **macOS:** Apple Developer ID Application certificate ($99/yr Apple
  Developer Program) + notarization. Secrets: `APPLE_CERTIFICATE` (base64
  .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD`,
  `APPLE_TEAM_ID`. Import into a temp keyourcing in the workflow; pass
  `APPLE_SIGNING_IDENTITY` to tauri-action's env.
- **Windows:** Azure Trusted Signing (~$10/mo, cheaper than a traditional
  Authenticode cert). Secrets: `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`,
  `AZURE_CLIENT_SECRET`. Sign via `cargo install trusted-signing-cli` +
  Tauri's `windows` signing hook.
- Both are additive to this spec (more env secrets on the same tauri-action
  step; no structural workflow change). The updater ed25519 signing from this
  spec stays as-is regardless.
