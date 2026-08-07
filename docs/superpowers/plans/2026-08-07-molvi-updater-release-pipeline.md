# molvi updater release pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Tauri 2 updater end-to-end (ed25519 signing + GitHub Releases
endpoint + a `tauri-action@v1` release workflow) so a versioned release is
*possible* on all 3 platforms (Windows / macOS-Apple-Silicon / Linux) — unblocking
the RELEASE BLOCKER flagged in the multi-platform port.

**Architecture:** Updater-only (no OS code-signing). 4 components: (1) a local
ed25519 keypair (free, self-generated); (2) two value swaps in `tauri.conf.json`
(real pubkey + GitHub endpoint); (3) one GitHub Actions secret
(`TAURI_SIGNING_PRIVATE_KEY`); (4) one new workflow `release.yml`
(`tauri-action@v1`, 3-OS matrix, draft releases). **Zero new app code** — the
existing `updater.rs`/IPC/frontend read `pubkey`/`endpoints` from config at
runtime and work the moment the placeholders are replaced.

**Tech Stack:** Tauri 2.11 + `tauri-plugin-updater` 2.10.1 (already in the app);
`tauri-apps/tauri-action@v1` (GitHub Actions, current stable); ed25519 signing
(`tauri signer generate`); GitHub Releases (free, auto-generated `latest.json`).

## Global Constraints

Copied verbatim from the spec + project bible (AGENTS.md). Every task implicitly
includes these.

- **Blaze (HARD — performance):** the dictation hot loop (capture→engine→
  finalize→paste) MUST stay byte-untouched + RTF ≤ 0.03. This plan changes ZERO
  Rust/TS application code — only `tauri.conf.json` values + a new workflow YAML.
  The release build uses molvi's existing `[profile.release]` (`opt-level = 3`,
  `codegen-units = 1`, `lto = "thin"`, `strip = true`, `panic = "unwind"` —
  Cargo.toml:77-82) which is already maximally optimized. `tauri-action` runs
  `cargo tauri build` (release profile) → the shipped binary's inference speed
  is identical to a local `cargo tauri build`. The release workflow is OFFLINE
  from the app runtime; build *compile* time is slow (~15-25 min/platform for
  the ort/transcribe-rs/parakeet-rs graph) but the produced *binary* is blaze.
- **Privacy §10.1 (HARD):** the ed25519 private key is a GitHub Actions secret
  (masked in logs, encrypted at rest). Version strings + endpoint URLs are
  metadata. No transcript/audio/settings/dict content touches the pipeline.
- **Updater-only scope:** NO OS code-signing in this plan. macOS builds are
  unsigned (Gatekeeper: right-click → Open); Windows unsigned (SmartScreen warn,
  runs); Linux AppImage unsigned (norm). Documented in the spec; OS-signing is a
  separate later effort (Apple Developer ID $99/yr + Azure Trusted Signing).
- **`[patch.crates-io]` transcribe-rs override** (Cargo.toml:92-93) is
  load-bearing — do NOT touch it.
- **`macos-14` = Apple Silicon** (default host `aarch64-apple-darwin`) — the
  release matrix uses `macos-14` with NO `--target` arg (matches molvi's CI;
  molvi is Intel-unsupported per D3). Do NOT add Intel Mac (`x86_64-apple-darwin`).
- **Ubuntu deps mirror molvi's CI EXACTLY** (ci.yml:47): `libwebkit2gtk-4.1-dev
  build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev
  librsvg2-dev libasound2-dev pkg-config`. Use `libayatana-appindicator3-dev` (the
  maintained fork), NOT the generic `libappindicator3-dev` from tauri-action docs.
- **Verify against live docs, never memory:** `tauri-action@v1` is current stable
  (`gh api repos/tauri-apps/tauri-action/tags` → v1.0.0). The public key is a
  single-line base64 minisign-format string (NOT PEM) — verified empirically;
  goes verbatim into `tauri.conf.json` as a JSON string with no escaping.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `~/.tauri/molvi.key` (NEW, local, never committed) | ed25519 private key (update-trust root) | 1 |
| `~/.tauri/molvi.key.pub` (NEW, local) | ed25519 public key (→ tauri.conf.json) | 1 |
| `src-tauri/tauri.conf.json` (MODIFY) | swap placeholder `pubkey` + `endpoints` → real values | 2 |
| GitHub repo secret `TAURI_SIGNING_PRIVATE_KEY` (NEW) | private key content for the release build | 1 |
| `.github/workflows/release.yml` (NEW) | `tauri-action@v1`: build + sign + draft release, 3-OS matrix | 3 |

No Rust/TS source files are touched. The existing `src-tauri/src/updater.rs`,
`src-tauri/src/ipc.rs` (`check_update`/`apply_update`), `src/settings/sections/
updates.ts`, and `src-tauri/src/settings.rs` (`UpdaterSettings.check_on_startup`)
are all **unchanged** — they read the config this plan edits.

---

## Task 1: Generate the ed25519 keypair + set the GitHub secret

> **This is a HUMAN RUNBOOK task** (local keygen + GitHub UI). It produces no
> commit + no reviewable code. It MUST complete before Task 2 (which needs the
> public key) and before Task 3's dry-run (which needs the secret). A subagent
> cannot do this — the human does.

**Produces:**
- `~/.tauri/molvi.key` — the private key file (NEVER committed; BACKUP offline).
- `~/.tauri/molvi.key.pub` — the public key (its content goes into Task 2).
- GitHub secret `TAURI_SIGNING_PRIVATE_KEY` — the private key content (for the
  release workflow's `tauri-action` step to sign artifacts).

- [ ] **Step 1: Generate the keypair (local, no password)**

Run on the dev machine (NOT in CI; the key never leaves your machine except as
the GitHub secret):
```bash
npx tauri signer generate --ci -w ~/.tauri/molvi.key
```
> `--ci` skips the interactive prompt (no password). Expected output:
> ```
>         Warn Generating new private key without password. For security reasons, we recommend setting a password instead.
> Your keypair was generated successfully:
> Private: ~/.tauri/molvi.key (Keep it secret!)
> Public: ~/.tauri/molvi.key.pub
> ```
> The `Warn` is expected + accepted (no-password for v1 simplicity — the private
> key is itself a high-entropy secret; GitHub's encrypted secret store is the
> protection layer).
>
> **VERIFY** the two files exist + are non-empty:
> ```bash
> ls -la ~/.tauri/molvi.key ~/.tauri/molvi.key.pub
> # Windows PowerShell: Get-Item ~/.tauri/molvi.key, ~/.tauri/molvi.key.pub | Select Name,Length
> ```
> Expected: both files exist; private ~348 bytes, public ~152 bytes.

- [ ] **Step 2: BACKUP the private key offline**

Copy `~/.tauri/molvi.key` to a secure offline location (password manager
attachment, encrypted USB, etc.). **Rationale:** losing this key = installed
copies can no longer update (the public key baked into them won't verify a
different signing key). It is the update-trust root.

- [ ] **Step 3: Set the GitHub secret `TAURI_SIGNING_PRIVATE_KEY`**

GitHub repo → **Settings → Secrets and variables → Actions → New repository
secret**:
- **Name:** `TAURI_SIGNING_PRIVATE_KEY`
- **Value:** the ENTIRE contents of `~/.tauri/molvi.key` (the single-line
  base64 private-key string, e.g. `dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5...`).
  Copy-paste the full file content — do NOT truncate.
- Click **Add secret**.

> Do NOT set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the key has no password).
> Do NOT set `TAURI_SIGNING_PRIVATE_KEY_PATH` (the action uses the content form
> via the secret).

- [ ] **Step 4: Capture the public key for Task 2**

Read `~/.tauri/molvi.key.pub` — its full single-line content is the public key
that Task 2 pastes into `tauri.conf.json`. Copy it to a scratch note (it is NOT
secret — public keys are safe to share, but keep it handy for Task 2).

**Deliverable:** keypair generated + backed up; `TAURI_SIGNING_PRIVATE_KEY`
secret set; public key captured. Nothing committed yet (the key never enters
git). Proceed to Task 2.

---

## Task 2: Wire the real pubkey + endpoint into `tauri.conf.json`

> **CODE task** (one file, two value swaps). Reviewable by an independent
> subagent. Blaze-safe (config-only; no app code).

**Files:**
- Modify: `src-tauri/tauri.conf.json` (the `plugins.updater` block, ~lines 76-85)

**Interfaces:**
- Consumes: the public key from Task 1 Step 4 (`~/.tauri/molvi.key.pub` content).
- Produces: a `tauri.conf.json` whose `plugins.updater.pubkey` is the real ed25519
  public key + whose `plugins.updater.endpoints[0]` is the GitHub Releases
  `latest.json` URL. The existing `updater.rs`/IPC/frontend read these at runtime.

- [ ] **Step 1: Read the current `plugins.updater` block**

Read `src-tauri/tauri.conf.json`. The current block (verified) is:
```jsonc
  "plugins": {
    "updater": {
      "pubkey": "PLACEHOLDER_PASTE_REAL_ED25519_PUBKEY_BEFORE_RELEASE",
      "endpoints": [
        "https://github.com/PLACEHOLDER_OWNER/molvi/releases/latest/download/latest.json"
      ],
      "windows": {
        "installMode": "passive"
      }
    }
  }
```
Note: `bundle.createUpdaterArtifacts: true` (tauri.conf.json:64) is ALREADY set —
leave it. `windows.installMode: "passive"` is correct — leave it.

- [ ] **Step 2: Replace the `pubkey` placeholder**

Replace the string `"PLACEHOLDER_PASTE_REAL_ED25519_PUBKEY_BEFORE_RELEASE"` with
the **entire content of `~/.tauri/molvi.key.pub`** (the single-line base64 public
key from Task 1 Step 4). The result is a JSON string value — the public key is a
single line (no newlines, no PEM `-----BEGIN-----` wrappers), so it pastes verbatim
with NO escaping:
```jsonc
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU5RTJCMDIwQzIzOEVFNjEKUldS...fullkey...",
```
> The pubkey starts with `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6`
> (= base64 of "untrusted comment: minisign public key:"). That prefix is correct
> + expected — paste the FULL string including it.

- [ ] **Step 3: Replace the `endpoints` placeholder**

Replace the URL `"https://github.com/PLACEHOLDER_OWNER/molvi/releases/latest/download/latest.json"`
with:
```jsonc
      "endpoints": [
        "https://github.com/bumbaRasch/molvi/releases/latest/download/latest.json"
      ],
```
> The owner is `bumbaRasch` (verified: `git remote -v` →
> `https://github.com/bumbaRasch/molvi.git`; PR #1 is at that owner). Do NOT
> change the path — `releases/latest/download/latest.json` is the canonical
> GitHub Releases updater endpoint (auto-generated by `tauri-action`, verified).

- [ ] **Step 4: Verify the JSON is valid**

Run (PowerShell):
```powershell
Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json | Out-Null; Write-Output "JSON valid"
```
Expected: prints `JSON valid`. If it errors → the pubkey paste broke the JSON
(likely a stray newline or quote) — fix the paste, re-verify.

- [ ] **Step 5: Verify the values are non-placeholder**

Run:
```powershell
$j = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json
$pub = $j.plugins.updater.pubkey
$ep  = $j.plugins.updater.endpoints[0]
Write-Output "pubkey starts: $($pub.Substring(0,[Math]::Min(40,$pub.Length)))"
Write-Output "pubkey len: $($pub.Length)"
Write-Output "endpoint: $ep"
```
Expected: `pubkey starts:` shows `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWdu...`; `pubkey
len` is ~150-160; `endpoint:` is
`https://github.com/bumbaRasch/molvi/releases/latest/download/latest.json`. NONE
of the values contain the word `PLACEHOLDER`.

- [ ] **Step 6: Verify the frontend + build gates pass (blaze + no-regression)**

The config edit must not break the build. Run:
```
npx tsc --noEmit
npm run build
cargo check --all-targets --manifest-path src-tauri/Cargo.toml
```
Expected: all PASS (config-only change; no code touched). `cargo check` (not
`build`) avoids the long release compile while still type-checking the Rust side.
If a `cargo tauri dev` holds the binary, `cargo check --all-targets` is the safe
gate. Do NOT kill a running dev app.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "updater: wire real ed25519 pubkey + GitHub Releases endpoint"
```

---

## Task 3: Create the release workflow (`.github/workflows/release.yml`)

> **CODE task** (one new file). Reviewable by an independent subagent.
> Blaze-safe (a workflow YAML; offline from the app runtime).

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: the GitHub secret `TAURI_SIGNING_PRIVATE_KEY` (set in Task 1).
- Produces: a GitHub Actions workflow `Release` (manual `workflow_dispatch`)
  that builds all 3 platforms in parallel, signs the updater artifacts (ed25519),
  and creates ONE **draft** GitHub Release with `latest.json` + per-platform
  installers + `.sig` files.

- [ ] **Step 1: Verify `release.yml` does not already exist**

Run: `Test-Path .github/workflows/release.yml` (PowerShell) or check the
`.github/workflows/` directory. Expected: does NOT exist (only `ci.yml` is there,
verified). If it exists, STOP — reconcile before overwriting.

- [ ] **Step 2: Write the workflow**

Create `.github/workflows/release.yml` with EXACTLY this content:

```yaml
name: Release

# Manual trigger only (GitHub → Actions → Release → Run workflow). Avoids
# tag/version mismatch: the version comes from tauri.conf.json (bumped in a
# commit before clicking Run). Mirrors cjpais/Handy's release trigger.
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
          - os: macos-14        # Apple Silicon (aarch64-apple-darwin default host) — D3, Intel unsupported
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

      # Mirrors molvi's CI dep set EXACTLY (ci.yml:47): webkit2gtk-4.1 (Tauri 2),
      # libayatana-appindicator3-dev (maintained fork, NOT generic libappindicator3),
      # libasound2-dev (cpal ALSA), + build tooling. Proven to compile molvi.
      - name: Install Linux system deps (Tauri 2 + ALSA)
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev pkg-config

      # tauri-action@v1 = current stable (gh api repos/tauri-apps/tauri-action/tags
      # → v1.0.0; Handy pins stale @v0). It builds all configured bundle types per
      # OS, signs updater artifacts (TAURI_SIGNING_PRIVATE_KEY present → emits .sig),
      # and creates ONE draft release with auto-generated latest.json. If no .sig
      # files are produced, tauri-action SKIPS the latest.json upload — so signing
      # is load-bearing (the secret must be set).
      - uses: tauri-apps/tauri-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
        with:
          tagName: v__VERSION__           # __VERSION__ ← tauri.conf.json version (auto-replaced)
          releaseName: 'molvi v__VERSION__'
          releaseBody: 'See the assets to download and install. Unsigned build — macOS: right-click → Open; Windows: SmartScreen → More info → Run anyway.'
          releaseDraft: true              # NOT public until manually published (safety gate)
          prerelease: false
```

**Design notes (the invariant — do NOT deviate without re-verifying):**
- **`on: workflow_dispatch`** (manual), NOT tag-push — avoids tag/version mismatch.
- **Matrix: `[windows-latest, macos-14, ubuntu-latest]`** — NO `--target` args.
  `macos-14`'s default host IS `aarch64-apple-darwin` (Apple Silicon); molvi is
  Intel-unsupported (D3). Do NOT add `macos-latest` + `--target x86_64-apple-darwin`
  (that would build Intel, which molvi's ort-sys dist.tsv has no row for → fails).
- **`releaseDraft: true`** — the release is a draft. `latest.json` is NOT
  publicly downloadable while drafted → the updater finds nothing until the draft
  is manually published. This is the safety gate (unverified Mac/Linux builds
  never auto-publish).
- **`TAURI_SIGNING_PRIVATE_KEY` in `env`** — its presence makes `cargo tauri
  build` emit `.sig` files; tauri-action references them in `latest.json`.
  Without it, tauri-action SKIPS the `latest.json` upload (verified).
- **Action versions** mirror molvi's CI (`checkout@v7`, `rust-toolchain@stable`,
  `rust-cache@v2`, `setup-node@v7` node 24) — consistency.
- **`releaseBody`** documents the unsigned-build workaround honestly (no false
  trust claims).
- **NO `updaterJsonPreferUnsigned`** — we SIGN, so it's unnecessary.

- [ ] **Step 3: Verify the YAML is valid**

Run (PowerShell — parse the YAML):
```powershell
# YAML isn't in stdlib; use a quick node check (node 24 is installed):
node -e "const y=require('fs').readFileSync('.github/workflows/release.yml','utf8'); const {parse}=require('yaml'); try{parse(y); console.log('YAML valid')}catch(e){console.error('YAML INVALID:',e.message); process.exit(1)}"
```
> If the `yaml` npm package isn't installed: `npm ls yaml` — if missing, skip this
> step (the workflow_dispatch dry-run in Task 4 is the authoritative YAML check;
> GitHub validates the YAML when the workflow loads). Alternatively a visual
> indent check (2-space, consistent).

- [ ] **Step 4: Commit + push**

```bash
git add .github/workflows/release.yml
git commit -m "ci: release workflow (tauri-action@v1, 3-OS, ed2551 draft releases)"
git push
```
> Push so the workflow appears in the GitHub Actions tab (workflows are loaded
> from the branch they're on).

- [ ] **Step 5: Verify the workflow appears in the Actions tab**

```bash
gh workflow list
```
Expected: lists `Release` (the new workflow) alongside `CI`. If it doesn't
appear, the YAML has a structural error GitHub rejected — check the Actions tab
for a workflow-syntax error annotation, fix, re-push.

---

## Task 4: Dry-run release + verify the artifacts (HUMAN + CI)

> **VERIFICATION task.** Produces the FIRST draft release (NOT published). This
> is the end-to-end proof that the pipeline works. Human triggers + inspects; CI
> does the build. Do NOT publish the draft in this task (publishing is a separate
> release decision, made when the Mac/Linux runtime smokes pass or the unsigned
> risk is accepted).

**Prerequisite:** Tasks 1-3 complete (secret set, config wired, workflow pushed).

- [ ] **Step 1: Bump the version in `tauri.conf.json`**

Edit `src-tauri/tauri.conf.json` line 4: `"version": "0.1.0"` → `"version": "0.2.0"`
(the first release using this pipeline; v0.1.0 was the Windows-only pre-port
release). Verify:
```powershell
$j = Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json; Write-Output "version: $($j.version)"
```
Expected: `version: 0.2.0`. Commit + push:
```bash
git add src-tauri/tauri.conf.json
git commit -m "release: bump version to 0.2.0"
git push
```

- [ ] **Step 2: Trigger the Release workflow**

GitHub → **Actions → Release → Run workflow** (use the branch with the workflow,
e.g. `main` or `multiplatform-port` — wherever Tasks 2-3 landed). OR via CLI:
```bash
gh workflow run release.yml
```
Then watch the run:
```bash
gh run watch
```
Expected: 3 jobs (windows-latest, macos-14, ubuntu-latest) run in parallel. Each
takes ~15-25 min (full optimized ort/transcribe-rs/parakeet-rs compile). They
should all go green (compile is already CI-verified; this is the same code +
deps, just a release-profile build + bundling + signing).

- [ ] **Step 3: Verify the draft release contents**

Once all 3 jobs are green, find the draft release:
```bash
gh release list --limit 5
```
Expected: a draft release `molvi v0.2.0` (tag `v0.2.0`). Inspect its assets:
```bash
gh release view v0.2.0 --json assets --jq '.assets[].name'
```
Expected assets (the exact names vary by platform/bundler, but these CATEGORIES
must all be present):
- **`latest.json`** — the updater manifest (CRITICAL — its presence proves
  signing worked; if MISSING, `TAURI_SIGNING_PRIVATE_KEY` secret wasn't read →
  tauri-action skipped it).
- **Windows:** `molvi_0.2.0_x64-setup.exe` (NSIS) + `molvi_0.2.0_x64_en-US.msi`
  (MSI) + their `.sig` files.
- **macOS:** `molvi_0.2.0_aarch64.dmg` + `.app.tar.gz` + their `.sig` files.
- **Linux:** `molvi_0.2.0_amd64.deb` + `.AppImage` + `.rpm` + their `.sig` files.

Inspect `latest.json` content (it must reference the signed per-platform URLs):
```bash
gh release download v0.2.0 latest.json --output - | head -c 500
```
Expected: JSON with `"version": "0.2.0"`, `"pub_date"`, and per-platform entries
(`platforms` object with `windows-x86_64` / `darwin-aarch64` / `linux-x86_64`)
each pointing at a signed URL + signature.

- [ ] **Step 4: Do NOT publish yet**

Leave the release as a **draft**. Publishing makes `latest.json` public → every
running molvi (v0.1.0) finds the update on next "Check for updates". Publish only
when ready (after the Mac/Linux runtime smokes, or accepting the unsigned-build
risk). The draft can sit indefinitely.

**Deliverable:** a verified draft release `v0.2.0` with `latest.json` + signed
per-platform installers. The updater pipeline is PROVEN end-to-end. Publishing =
one click when ready.

---

## Self-Review

**1. Spec coverage** — checked each spec section against the tasks:
- Key generation ([1]) → Task 1. ✓
- tauri.conf.json pubkey + endpoint ([2]) → Task 2 Steps 2-3. ✓
- GitHub secret ([3]) → Task 1 Step 3. ✓
- `release.yml` workflow ([4]) → Task 3 Step 2. ✓
- Release flow (bump → trigger → draft → verify → publish-later) → Task 4. ✓
- Blaze (zero app code, [profile.release] preserved) → Global Constraints + Task 2 Step 6. ✓
- Privacy (secret masked, metadata only) → Global Constraints. ✓
- Updater-only scope (no OS code-signing) → Global Constraints. ✓
- Verified facts (tauri-action@v1, latest.json requires .sig, macos-14 no-target,
  ayatana-ubuntu-deps, pubkey single-line base64) → Global Constraints + Task 3
  design notes + Task 2 Step 2 note. ✓

**2. Placeholder scan** — the plan contains real values everywhere: the exact
endpoint URL (`https://github.com/bumbaRasch/molvi/...`), the exact ubuntu dep
list, the exact workflow YAML, the exact version bump (0.1.0→0.2.0). The ONLY
placeholder is the public key VALUE itself (`dW50cnVzdGVk...fullkey...`) — which
is honest (it can only be generated at Task 1 execution, not in the plan). No
"TBD"/"TODO"/"add error handling" anywhere.

**3. Type consistency** — the secret name `TAURI_SIGNING_PRIVATE_KEY` is
consistent across Task 1 (set it), Task 3 (read it in `env`). The endpoint URL is
identical in Task 2 (config) and the spec. `tauri-action@v1` consistent
throughout. The version `0.2.0` consistent in Task 4 Steps 1-3.

**4. Blaze re-check** — confirmed: Tasks 2-3 change only `tauri.conf.json` (2
values) + add 1 workflow file. ZERO lines of `src-tauri/src/*.rs` or `src/*.ts`
are touched. The release build uses the existing optimized `[profile.release]`.
The dictation hot loop is byte-identical. RTF ≤ 0.03 is preserved by compilation,
not affected by the config/workflow.

---

## Execution notes

- **Task 1 is a human runbook** — no subagent, no commit. Do it FIRST (it gates
  Task 2's pubkey + Task 4's secret).
- **Tasks 2 + 3 are code/edit tasks** — reviewable by independent subagents
  (the config swap + the workflow YAML). Each has explicit verification steps.
- **Task 4 is end-to-end verification** — human-triggered, CI-executed. The draft
  release is the proof. Do NOT publish in Task 4.
- **Gates:** Task 2 → `npx tsc --noEmit && npm run build && cargo check
  --all-targets`. Task 3 → `gh workflow list`. Task 4 → `gh release view v0.2.0`
  assets (latest.json + .sig present). The existing `ci.yml` remains the per-push
  compile gate; `release.yml` is opt-in.
