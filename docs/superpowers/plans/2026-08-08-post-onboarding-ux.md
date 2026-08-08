# Post-Onboarding UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After onboarding completes (Skip or Done), surface the Settings window plus one context-aware toast so the user knows molvi lives in the tray and which hotkey to press — or that dictation unlocks shortly if the engine isn't ready.

**Architecture:** `complete_onboarding` (Rust IPC) gains a `ready: bool` param sourced from the onboarding frontend's `engineReady`. After its current finalization, it calls the existing `tray::show_settings` and emits a `post-onboarding-hint` event `{ ready, hotkey }`. The Settings window's `main.ts` (its webview loads at startup, listener already live) receives the event and calls the existing vanilla `toast()`: success + `onboarding.all_set` when ready, info + a new `onboarding.toast_preparing` key otherwise. No new module, no new `AppState` field, no new dependency, no new window.

**Tech Stack:** Rust (edition 2024, Tauri 2.11.5, `Emitter::emit`, `serde_json::json!`), TypeScript (vanilla, no framework), plain-object i18n (36 locales).

**Spec:** `docs/superpowers/specs/2026-08-08-post-onboarding-ux-design.md` (read it first).

## Global Constraints

- **Rust:** stable, edition 2024, MSRV `rustc 1.97.1`. `tauri 2.11.5`. `Emitter` + `Manager` already imported in `src-tauri/src/ipc.rs:10`.
- **Gates (must be green):** `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; `cargo test --manifest-path src-tauri/Cargo.toml --lib`; `npx tsc --noEmit`; `npm run build`.
- **Binary lock:** if a live `molvi.exe` (`cargo tauri dev`) holds the debug binary, `cargo build`/full `cargo test` fail at link — use `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` instead. Do NOT kill the human's running app.
- **ONE-commit invariant (the published state is a single commit):** per-task commits ARE expected as you go — the repo is currently one commit (`4801a8c molvi v0.1.0 …`); task commits stack on top of it. At the very end, on the human's explicit request, squash them all back into that one commit: `git reset --soft 4801a8c` (base = the pre-existing `molvi v0.1.0` commit; confirm with `git log --oneline`) → `git commit --amend --no-edit` → `git push --force-with-lease`. Do NOT push the per-task commits; only the final squashed single commit is published.
- **Privacy §10.1:** never log transcript/audio/dict/dictionary/snippet/history content. The `{ ready: bool, hotkey: String }` payload is metadata (hotkey = config key-combo); it crosses the IPC bus as an event and is NEVER passed to `tracing::`. No new log lines carry user content.
- **Blaze (performance NFR):** zero impact on the capture→engine→finalize→paste hot loop (RTF ≤ 0.03). This change touches only first-run one-shot finalization + UI events.
- **i18n:** en is canonical; every locale's key set must equal en's (set-equality). Tokens like `{hotkey}` are ASCII-verbatim in ALL locales including RTL (ar, he) and CJK (ja, ko, zh, vi, th).
- **No new tests required:** this is side-effect wiring (window show + event emit) + i18n content. There is no Tauri-command unit-test harness in this project and no JS test runner (frontend gate = `tsc` + `npm run build`). Verification = the 4 gates + the Task 4 manual smoke. Do NOT fabricate tests (over-engineering).
- **Doc verification (AGENTS.md anti-stale rule):** the spec already verified the load-bearing Tauri APIs via ctx7 `/websites/v2_tauri_app` — (1) window `create` config defaults `true`, so `visible:false` windows load webview + JS at startup (Settings listener is live before onboarding completes); (2) `Emitter::emit` global + frontend `listen<T>` typed payload; (3) `serde_json::json!` payload convention (`model_store.rs:161`). Do not re-litigate; code to these verified signatures.

## File Structure

- `src-tauri/src/ipc.rs` — `complete_onboarding`: add `ready: bool` param; call `crate::tray::show_settings(&app)` + `app.emit("post-onboarding-hint", serde_json::json!({ "ready": ready, "hotkey": hotkey }))`. Read `hotkey` from settings while the lock is held.
- `src/onboarding.ts` — `complete()`: pass `{ ready: engineReady }` to the `complete_onboarding` invoke.
- `src/settings/main.ts` — `import { toast } from "./ui"`; register a `listen("post-onboarding-hint")` next to the existing `navigate-history` listener; branch on `ready` for success vs info toast.
- `src/i18n/locales/*.ts` × 36 — add one key `onboarding.toast_preparing` (en canonical), set-equal.

## Cross-Task Contract ( Interfaces )

All tasks must agree on these exact names/shapes:

- **IPC command:** `complete_onboarding(ready: bool)` (Rust param, snake_case; `ready` is single-word so camelCase≡snake_case). Frontend invokes `invoke("complete_onboarding", { ready: engineReady })`.
- **Event name:** `"post-onboarding-hint"` (kebab-case, matches `navigate-history` / `ui-lang-changed` / `engine-ready` convention).
- **Event payload:** `{ ready: boolean, hotkey: string }` — Rust emits `serde_json::json!({ "ready": ready, "hotkey": hotkey })`; TS listens `listen<{ ready: boolean; hotkey: string }>`.
- **i18n keys:** reuse `onboarding.all_set` (en: "You're all set. Press {hotkey} anywhere to dictate."); new `onboarding.toast_preparing` (en: "molvi is still preparing. Press {hotkey} to dictate when it's ready." — humanizer-reviewed, no em dash).

---

### Task 1: Rust — `complete_onboarding` surfaces Settings + emits the hint

**Files:**
- Modify: `src-tauri/src/ipc.rs:489-512` (the `complete_onboarding` body).
- No new file; no new import (`Emitter` + `Manager` already at `ipc.rs:10`; `serde_json::json!` used fully-qualified).

**Interfaces:**
- Consumes: `crate::tray::show_settings(&app)` (`tray.rs:170`, `pub(crate)`); `app.emit` (`Emitter`, in scope); `coordinator::Command::Cancel`; `AppState` fields `settings`, `cmd_tx`, `onboarding_practice`, `mic_preview`.
- Produces: the IPC command `complete_onboarding(ready: bool)` and the `"post-onboarding-hint"` event `{ ready, hotkey }` for Task 3 to consume.

- [ ] **Step 1: Read the current function** to confirm it matches the plan's "before" block.

Read `src-tauri/src/ipc.rs:483-512`.

- [ ] **Step 2: Replace the function** with the version below.

Replace the entire `complete_onboarding` function (signature + body, `ipc.rs:489-512`) with:

```rust
/// Complete onboarding. Set `settings.onboarded = true`, persist, clean-exit any
/// live practice session (Cancel + reset flags), hide the onboarding window.
/// THEN surface the Settings window + emit a context-aware hint so the user
/// (especially a Skip-from-step-1 user who never saw the hotkey) knows what's
/// next. `ready` is the onboarding frontend's `engineReady` — race-free vs
/// Rust-side download-handle detection (Skip kicks off the download and calls
/// this back-to-back; the bg thread may not have stored the handle yet).
/// Privacy §10.1: the payload is a readiness bool + the hotkey config combo
/// (metadata); it crosses the IPC bus as an event, never `tracing::`.
#[tauri::command]
pub fn complete_onboarding(
    app: AppHandle,
    state: State<'_, AppState>,
    ready: bool,
) -> Result<(), MolviError> {
    let hotkey = {
        let mut g = state.settings.lock().unwrap();
        g.onboarded = true;
        if let Err(e) = g.save() {
            tracing::warn!("settings save failed: {e}");
            return Err(e);
        }
        g.hotkey.clone()
    };
    // Clean exit any in-flight practice session (Cancel covers Recording +
    // Processing; a no-op in Idle). Drops a live session so a stale finalize
    // can't emit a practice-result after the window hid.
    if let Some(tx) = state.cmd_tx.lock().unwrap().as_ref() {
        let _ = tx.send(coordinator::Command::Cancel);
    }
    state.onboarding_practice.store(false, Ordering::Relaxed);
    state.mic_preview.store(false, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.hide();
    }
    // Surface the home window + a hint toast in it (the Settings webview loads
    // at startup, so its listener is already registered). The global hotkey is
    // OS-level/focus-independent, so the user can dictate even with Settings open.
    crate::tray::show_settings(&app);
    let _ = app.emit(
        "post-onboarding-hint",
        serde_json::json!({ "ready": ready, "hotkey": hotkey }),
    );
    tracing::info!("onboarding complete");
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles + clippy is clean.**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
(If a live `molvi.exe` holds the binary, run `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` instead — do NOT kill the running app.)
Expected: clean, no warnings. Common issue: none expected (`Emitter`, `Manager` in scope; `serde_json` is a dep).

- [ ] **Step 4: Run the lib tests** (unchanged behavior; confirms no regression).

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`
Expected: all 189 model-free tests pass (or, under binary lock, `cargo check --all-targets` suffices for this task since no test here changed).

- [ ] **Step 5: Commit (transient working commit; squashed into the single commit at the end).**

```bash
git add src-tauri/src/ipc.rs
git commit -m "feat(onboarding): surface Settings + emit post-onboarding hint"
```

---

### Task 2: i18n — add `onboarding.toast_preparing` to all 36 locales

**Files:**
- Modify: every file in `src/i18n/locales/*.ts` (36 files: ar, bg, cs, da, de, el, en, es, et, fi, fr, he, hi, hr, hu, it, ja, ko, lt, lv, mt, nb, nl, nn, pl, pt, ro, ru, sk, sl, sv, th, tr, uk, vi, zh).

**Interfaces:**
- Produces: the `onboarding.toast_preparing` key consumed by Task 3's info-toast branch.
- Consumes: nothing (content-only task).

**Canonical (en) value — copy verbatim:**

```
"onboarding.toast_preparing": "molvi is still preparing. Press {hotkey} to dictate when it's ready."
```

Humanizer-reviewed (`humanizer-zh` skill): two short sentences, no em dash (AI-tell pattern #13); mirrors `onboarding.all_set`'s two-sentence form. Don't "restore" an em dash.

**Placement (every locale):** insert as the LAST key of the `onboarding.*` block — immediately AFTER the `"onboarding.model_choose_another": "..."` line, and BEFORE the closing `};` of the locale object. In `en.ts` that is after line 231 (`"onboarding.model_choose_another": "Choose another model",`) and before line 232 (`};`).

**Translation rules:**
- Translate the prose to each locale's language, matching the register of that locale's existing `onboarding.preparing` and `onboarding.all_set` strings (read them in-file to match tone).
- The `{hotkey}` token is **ASCII-verbatim** in every locale (including ar, he — RTL — and ja, ko, zh, vi, th — CJK). Do not localize, transliterate, or rename it.
- "molvi" is a proper noun — untranslated everywhere.
- Two short sentences joined by a period (no em dash) — mirror the structure of each locale's existing `onboarding.all_set` (same two-sentence form). Match that locale's register/punctuation.

**Reference translations (anchor quality; the implementer produces all 36):**

| Locale | Value |
|---|---|
| en | `molvi is still preparing. Press {hotkey} to dictate when it's ready.` |
| ru | `molvi ещё готовится. Нажмите {hotkey} для диктовки, когда всё будет готово.` |
| es | `molvi aún se está preparando. Pulsa {hotkey} para dictar cuando esté listo.` |
| de | `molvi wird noch vorbereitet. Drücken Sie {hotkey} zum Diktieren, sobald es bereit ist.` |

- [ ] **Step 1: Add the key to `en.ts`** at the placement above. Read `src/i18n/locales/en.ts:219-233` to confirm the anchor, then insert after the `model_choose_another` line.

- [ ] **Step 2: Add the key to the other 35 locales**, each translated per the rules. For each file, read its `onboarding.all_set` / `onboarding.preparing` lines to match register, then insert the translated `onboarding.toast_preparing` after its `onboarding.model_choose_another` line.

- [ ] **Step 3: Verify the frontend still type-checks + builds.**

Run: `npx tsc --noEmit`
Expected: no errors (a missing comma or unbalanced brace in any locale will surface here).

Run: `npm run build`
Expected: clean Vite build.

- [ ] **Step 4: Verify the key is present + set-equal across all 36.**

Run (ripgrep): `rg -c "onboarding\.toast_preparing" src/i18n/locales` and confirm each of the 36 files reports exactly `1`.
Then confirm set-equality: each locale file's total key count equals `en.ts`'s key count. A quick check — count quoted keys per file:
`rg -c '^\s*"[a-zA-Z]+\.' src/i18n/locales/en.ts` vs the same for a few spot-checked locales (ru, ar, zh) — they must be equal. (The project's invariant is full set-equality ×36; this spot-check plus the tsc/build gate covers the single-key addition.)

- [ ] **Step 5: Commit (transient working commit; squashed at the end).**

```bash
git add src/i18n/locales
git commit -m "i18n: add onboarding.toast_preparing (×36 locales)"
```

---

### Task 3: Frontend wiring — pass `ready`, listen + toast

**Files:**
- Modify: `src/onboarding.ts:317` (the `complete()` invoke line).
- Modify: `src/settings/main.ts` — add one import + one listener (inside the existing startup IIFE at `main.ts:123-148`, next to `navigate-history` at line 133).

**Interfaces:**
- Consumes (from Task 1): the `complete_onboarding(ready: bool)` IPC command + the `"post-onboarding-hint"` event `{ ready: boolean; hotkey: string }`.
- Consumes (from Task 2): `onboarding.toast_preparing` key; reuses `onboarding.all_set`.
- Produces: the visible toast the user sees after Skip/Done.

- [ ] **Step 1: Pass `ready` from onboarding.**

In `src/onboarding.ts`, locate the `complete()` function's invoke line (`onboarding.ts:317`):

```ts
await invoke("complete_onboarding").catch((e) => console.error("complete_onboarding", e));
```

Replace with:

```ts
await invoke("complete_onboarding", { ready: engineReady }).catch((e) => console.error("complete_onboarding", e));
```

`engineReady` is the module-level boolean (`onboarding.ts:24`), set true only by the `engine-ready` event handler (`onboarding.ts:125`). It is `false` on a Skip-from-step-1 (download just started, hotkey not registered) and `true` otherwise — exactly the "can the user dictate now" signal.

- [ ] **Step 2: Add the `toast` import to `settings/main.ts`.**

In `src/settings/main.ts`, add to the local-imports block (e.g., immediately after `import { ICONS } from "./icons";` at line 5):

```ts
import { toast } from "./ui";
```

(`t` is already imported from `../i18n` at line 4; `listen` from `@tauri-apps/api/event` at line 2.)

- [ ] **Step 3: Register the listener.**

Inside the startup IIFE, immediately after the `navigate-history` listener (`main.ts:133`):

```ts
  void listen("navigate-history", () => selectSection("history"));
  void listen<{ ready: boolean; hotkey: string }>("post-onboarding-hint", (e) => {
    const { ready, hotkey } = e.payload;
    if (ready) toast("success", t("onboarding.all_set").replace("{hotkey}", hotkey));
    else toast("info", t("onboarding.toast_preparing").replace("{hotkey}", hotkey));
  });
```

- [ ] **Step 4: Verify the frontend gate.**

Run: `npx tsc --noEmit`
Expected: no errors.

Run: `npm run build`
Expected: clean Vite build.

- [ ] **Step 5: Commit (transient working commit; squashed at the end).**

```bash
git add src/onboarding.ts src/settings/main.ts
git commit -m "feat(onboarding): pass ready flag + listen for hint toast"
```

---

### Task 4: Manual smoke + final gate sweep (+ single amend, human-only)

**Files:** none (verification + the one human-driven amend).

**Interfaces:** none.

- [ ] **Step 1: Run the full 4-gate sweep** (the project's merge gate).

```
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
npx tsc --noEmit
npm run build
```

(If a live `molvi.exe` locks the binary, substitute `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` for the first two and note it; do NOT kill the running app. The human can run the full sweep once the dev binary is free.)

Expected: all four green.

- [ ] **Step 2: Manual smoke (`cargo tauri dev`).**

Force first-run (rename/backup the settings file so `onboarded` is unset, or use a fresh `%APPDATA%\com.molvi.app`), then verify the three completion paths:

1. **Skip from step 1** → onboarding hides → **Settings window appears** → an **info** toast shows the `onboarding.toast_preparing` text with the hotkey. The model downloads; tray Status flips Warming→Ready when done. (Confirms the engine-not-ready branch.)
2. **Skip from step 2 or 3** → Settings appears → a **success** toast shows `onboarding.all_set` with the hotkey. (Engine already ready — step 1 gates on `engine-ready`.)
3. **Done from step 3** (complete the full flow) → Settings appears → **success** toast `onboarding.all_set`.

Also confirm: with Settings open after completion, pressing the hotkey still starts dictation (global hotkey is focus-independent) — Settings is non-blocking.

- [ ] **Step 3: Language spot-check.** Switch the UI language (onboarding bottom-left picker) to at least ru, ar (RTL), ja (CJK); complete onboarding; confirm the toast renders localized with `{hotkey}` substituted and `dir`/layout correct.

- [ ] **Step 4: Privacy spot-check.** Grep the logs from the smoke run for the hotkey string and the event name — confirm neither the hotkey value nor any transcript appears in `tracing` output (only `"onboarding complete"` metadata is expected). `rg "post-onboarding-hint" <log>` should find nothing (it's an event, not a log).

- [ ] **Step 5: Squash into the single commit (HUMAN ONLY, on explicit request).** Fold all task commits back into the repo's one commit, then force-push:

```bash
git log --oneline            # confirm the base commit (molvi v0.1.0 …, currently 4801a8c)
git reset --soft 4801a8c     # drop task commits onto the base; keep every change staged
git commit --amend --no-edit # fold into the single commit, keep its message
git push --force-with-lease
```

(Do NOT run this autonomously — only when the human explicitly says to ship. The per-task commits were transient; this is the only published state.)

---

## Self-Review (completed)

**1. Spec coverage:** Spec §"complete_onboarding changes" → Task 1. §"settings/main.ts listener" → Task 3. §"ready source / frontend passes ready" → Task 3 Step 1 + Task 1 param. §"Window lifecycle" (load-bearing) → verified in Global Constraints doc-verification note (no code task; it's a property relied on, not changed). §"i18n" (1 new key, reuse all_set) → Task 2. §"Privacy" → Global Constraints + Task 4 Step 4. §"Performance/blaze" → Global Constraints (no hot-loop touch). §"Files affected" maps 1:1 to the four Modify lists. No spec gap.

**2. Placeholder scan:** No "TBD"/"TODO"/"add error handling". The 36-locale translations are a content task with a verbatim en canonical value + 4 reference translations + explicit rules — not a placeholder (content generation, not a code template). All code blocks are complete.

**3. Type consistency:** `ready: bool` (Rust) ↔ `ready: boolean` (TS) ↔ `engineReady` (source) — consistent. Event name `"post-onboarding-hint"` identical in Task 1 emit and Task 3 listen. Payload keys `"ready"` / `"hotkey"` identical both sides. i18n key `onboarding.toast_preparing` identical in Task 2 (produced) and Task 3 (consumed); `onboarding.all_set` reused unchanged. No drift.
