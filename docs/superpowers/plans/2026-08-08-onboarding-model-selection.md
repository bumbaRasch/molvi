# Onboarding Model Selection — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace onboarding's silent auto-download of Nemotron (2.4 GB) with an explicit, informed model choice (smart default by UI language); only the chosen model downloads.

**Architecture:** The cold-start bg thread stops auto-downloading and instead loops on a choice channel (`mpsc::Sender<(model_id, language)>` in `AppState`, mirroring the existing `cmd_tx`). Returning users auto-feed the channel from `settings`; first-run users feed it via a new `onboarding_select_model` IPC after picking a card. The download runs as a `tauri::async_runtime::spawn` task whose `JoinHandle` is stored in the **existing** `AppState.model_download`, so the existing `cancel_model_download` works unchanged. Onboarding step 1 shows two model cards (pre-selected by UI language) → real progress → engine-ready.

**Tech Stack:** Rust (edition 2024, tauri 2.11.5), vanilla TypeScript 7 + Vite 8, plain-object i18n (36 locales). Verified APIs: `model_store::ensure_model(id, make_progress)` (model_store.rs:288), `ModelProgressEmitter` (model_store.rs:115), `AppState.model_download` (lib.rs:175), `cancel_model_download` (ipc.rs:613), Tauri 2 `#[tauri::command]` + `State<T>` + `app.emit` + frontend `invoke`/`listen`.

## Global Constraints

- **One-commit invariant:** this repo has exactly ONE commit. Do NOT commit per-task. Stage all changes through Task 7, then a single `git commit --amend --no-edit` + `git push --force-with-lease`. (User instruction; overrides the skill's per-task-commit default.)
- **Backward compatibility is not required** — freely change cold-start logic, onboarding flow, settings write paths.
- **Privacy §10.1:** never log transcript/dict/snippet/ практики text. Model download logs carry only `model_id` + byte counts + error strings (metadata). The new IPC + bg-thread code touches no inference output.
- **Engine no-hot-reload:** engine + language apply at startup only (unchanged).
- **Gates (must stay green):** `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; `cargo test --manifest-path src-tauri/Cargo.toml --lib` (189 tests); `npx tsc --noEmit`; `npm run build`.
- **i18n set-equality:** every locale's key set === `en`. New keys added to all 36 locale files.
- **RTL:** new CSS uses logical properties (`inset-inline-*`, `border-inline-start`, `margin-block-*`); no physical `left/right` in layout.
- **Doc verification rule:** verify every Tauri/hf-hub API call against docs.rs / ctx7 (`/websites/v2_tauri_app`) / pinned source in `~/.cargo/registry` before coding. Do not trust memory.

**Spec:** `docs/superpowers/specs/2026-08-08-onboarding-model-selection-design.md` (read first).

---

## File Structure

**Backend (Rust):**
- `src-tauri/src/lib.rs` — add `model_selection_tx` to `AppState`; create channel in `run()`; refactor bg thread to loop on the choice channel + download via spawn/JoinHandle; returning-user auto-send in `setup`; register the new command.
- `src-tauri/src/ipc.rs` — new `onboarding_select_model(model_id, language)` command + a pure `validate_model_id` helper (unit-tested).

**Frontend (TS/CSS/HTML):**
- `src/i18n/locales/en.ts` — 6 new canonical keys.
- `src/i18n/locales/*.ts` (×35 others) — same 6 keys (English baseline; `ru` properly translated) for set-equality.
- `onboarding.html` — step-1 markup (choice cards + progress + error elements).
- `src/onboarding.ts` — step-1 logic (fetch sizes, smart default, card select, invoke, progress/error listeners, cancel, skip).
- `src/onboarding.css` — card / selected / recommended-badge / progress / error styles.

**No new files; no schema change** (`Settings.model` / `Settings.language` already exist; `ModelStatus` already mirrors Rust).

---

## Task 1: Backend — deferred model-choice channel + bg-thread download loop

**Files:**
- Modify: `src-tauri/src/lib.rs` (`AppState` ~L134-176; `run()` channel creation + `.manage`; bg thread ~L573-669; `setup` returning-user auto-send)

**Interfaces:**
- Produces: `AppState.model_selection_tx: Mutex<Option<std::sync::mpsc::Sender<(String, String)>>>` (consumed by Task 2's command). The bg thread owns the matching `Receiver`.

- [ ] **Step 1: Add the field to `AppState`**

In `lib.rs`, add to the `AppState` struct (next to `cmd_tx`, ~L136):

```rust
/// Model-choice signal for the bg thread (onboarding model selection).
/// Mirrors `cmd_tx`: `Some` after setup wires the channel; the
/// `onboarding_select_model` IPC sends `(model_id, language)` here. The bg
/// thread loops on the receiver — first-run waits for an onboarding pick,
/// returning-run is auto-fed from `settings` in `setup`. Privacy §10.1:
/// carries only a model id + a locale code, no user content.
pub model_selection_tx: std::sync::Mutex<Option<std::sync::mpsc::Sender<(String, String)>>>,
```

- [ ] **Step 2: Create the channel in `run()` and wire `tx` into `AppState`**

In `run()`, before the `tauri::Builder::default()` chain, create the channel; capture `model_rx` for the bg thread (it's moved into the `setup` closure which spawns the bg thread). Put `model_tx` into the `AppState` literal inside `.manage(...)`:

```rust
let (model_tx, model_rx) = std::sync::mpsc::channel::<(String, String)>();
```

In the `.manage(AppState { ... })` literal, add:

```rust
model_selection_tx: std::sync::Mutex::new(Some(model_tx)),
```

`model_rx` must reach the bg-thread spawn inside `setup`. The setup closure already captures many locals (`settings`, `app_handle`, `capture`, `consumer`, `native_rate`); add `model_rx` to the `move` closure.

- [ ] **Step 3: Refactor the bg thread to loop on the choice channel**

Replace the current `ensure_model` call block (lib.rs ~L581-593) with a loop that (a) waits for a choice, (b) downloads via a spawn task storing its JoinHandle in `model_download` (so `cancel_model_download` works), (c) loops on cancel/error. The rest of the bg thread (engine spawn → coordinator → hotkey → tray → `engine-ready`) is unchanged but now runs with the `model_dir` produced by the loop.

Inside the `spawn(move || { ... })` body, replace the `ensure_model` block with:

```rust
let model_dir = loop {
    let (model_id, _lang) = match model_rx.recv() {
        Ok(v) => v,
        Err(_) => {
            tracing::error!("model-choice channel closed, PTT disabled");
            let _ = app_handle.emit("engine-error", ());
            crate::tray::show_settings(&app_handle);
            return;
        }
    };
    tracing::info!("ensuring model {} is present", model_id);

    // Disk-space pre-check (fail fast instead of running 2.4GB into ENOSPC).
    let total = model_store::grand_total(&model_id);
    if !model_store::has_disk_space(total).unwrap_or(true) {
        tracing::warn!("insufficient disk space for {model_id}");
        let _ = app_handle.emit("model-download-failed", &model_id);
        continue; // loop back: onboarding shows error → retry / choose another
    }

    // Download as an abortable async task; store its JoinHandle in the
    // existing AppState.model_download so cancel_model_download works.
    let (res_tx, res_rx) = std::sync::mpsc::channel();
    let app_for_task = app_handle.clone();
    let id_for_task = model_id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let total = model_store::grand_total(&id_for_task);
        let result = model_store::ensure_model(&id_for_task, |offset| {
            Some(hf_hub::progress::Progress::new(
                model_store::ModelProgressEmitter::new(app_for_task.clone(), &id_for_task, total, offset),
            ))
        })
        .await;
        // Emit completion so onboarding can show "Preparing engine…"; the bg
        // thread separately emits engine-ready after spawn. Privacy: id only.
        match &result {
            Ok(_) => { let _ = app_for_task.emit("model-download-complete", &id_for_task); }
            Err(e) => { tracing::warn!("model download failed: {e}"); let _ = app_for_task.emit("model-download-failed", &id_for_task); }
        }
        let _ = res_tx.send(result);
    });
    *app_handle.state::<AppState>().model_download.lock().unwrap() = Some(handle);

    match res_rx.recv() {
        Ok(Ok(d)) => break d,            // success → proceed to engine spawn
        Ok(Err(_)) => continue,          // failed (event already emitted) → retry
        Err(_) => continue,              // cancelled (task aborted) → loop back
    }
};
tracing::info!("model dir: {}", crate::paths::redact_appdata(&model_dir));
```

The code that follows (coordinator channel, `EngineHandle::spawn`, coordinator thread, `cmd_tx` expose, hotkey, tray, `engine-ready` emit) is unchanged — it uses `model_dir`.

- [ ] **Step 4: Returning-user auto-proceed in `setup`**

In `setup`, after `let settings = ...clone();` (where `onboarded` is read for the launch gate), if the user is already onboarded, feed the channel so the bg thread proceeds without waiting for onboarding:

```rust
if settings.onboarded {
    let _ = app
        .state::<AppState>()
        .model_selection_tx
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|tx| tx.send((settings.model.clone(), settings.language.clone())).ok());
}
```

mpsc buffers, so this is order-independent with the bg thread's first `recv()`. For a cached returning-user model, `ensure_model`'s byte-exact fast path (model_store.rs:301) is a no-op → engine spawns immediately, as today.

- [ ] **Step 5: Verify it compiles + tests stay green**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean (no warnings).

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`
Expected: 189 passed, 0 failed.

- [ ] **Step 6: Do NOT commit** (one-commit invariant; amend happens in Task 7).

---

## Task 2: Backend — `onboarding_select_model` IPC command

**Files:**
- Modify: `src-tauri/src/ipc.rs` (new command + `validate_model_id` helper near the model-picker block ~L545)
- Modify: `src-tauri/src/lib.rs` (`generate_handler!` registration ~L398-436)

**Interfaces:**
- Consumes: `AppState.model_selection_tx` (from Task 1), `model_store::source` (existing, returns `Option`).
- Produces: `ipc::onboarding_select_model` (registered; invoked by onboarding frontend in Task 5).

- [ ] **Step 1: Write the failing test for `validate_model_id`**

In `ipc.rs` `#[cfg(test)] mod tests` (~L634), add:

```rust
#[test]
fn validate_model_id_accepts_known_rejects_unknown() {
    assert!(validate_model_id(model_store::MODEL_GIGAAM_V3_E2E_CTC));
    assert!(validate_model_id(model_store::MODEL_NEMOTRON_0_6B));
    assert!(!validate_model_id("nemotron-9.9-fake"));
    assert!(!validate_model_id(""));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib validate_model_id`
Expected: FAIL (`validate_model_id` not found).

- [ ] **Step 3: Add `validate_model_id` + the command**

Near the model-picker block in `ipc.rs` (~L545), add the pure helper and the command:

```rust
/// Pure model-id validation (unit-tested). Reuses model_store's source-of-truth
/// so the accepted set can never drift from what model_store can download.
fn validate_model_id(id: &str) -> bool {
    model_store::source_is_known(id)
}

/// Onboarding model choice (first-run). Validates, persists model + language,
/// and signals the bg thread to download + spawn the engine. Returning users
/// never call this (setup auto-feeds the channel). Privacy §10.1: id + locale
/// code only, no user content.
#[tauri::command]
pub fn onboarding_select_model(
    model_id: String,
    language: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), MolviError> {
    if !validate_model_id(&model_id) {
        return Err(MolviError::ModelStore(format!("unknown model id: {model_id}")));
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.model = model_id.clone();
        s.language = language;
    }
    crate::settings::save(&state.settings.lock().unwrap().clone())?;
    let _ = state
        .model_selection_tx
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|tx| tx.send((model_id.clone(), language)).ok())
        .ok_or_else(|| MolviError::ModelStore("model-choice channel closed".into()))?;
    Ok(())
}
```

- [ ] **Step 4: Expose `source_is_known` in model_store**

`model_store::source` is private. Add a tiny public wrapper in `model_store.rs` (next to `source`, ~L67):

```rust
/// Whether `model_id` is a known, downloadable model (thin pub wrapper over
/// `source`, which stays private). Used by IPC validation.
pub fn source_is_known(model_id: &str) -> bool {
    source(model_id).is_some()
}
```

- [ ] **Step 5: Register the command**

In `lib.rs` `generate_handler!` (~L398-436), add `crate::ipc::onboarding_select_model,` next to the other `crate::ipc::` entries.

- [ ] **Step 6: Verify `settings::save` signature**

Run: `grep -n "pub fn save" src-tauri/src/settings.rs` (use rg). Confirm the function exists and takes `&Settings`. If it is named differently (e.g. `save_to` / `persist`), use the real name. (Earlier analysis: `save` exists; verify — do not assume.)

- [ ] **Step 7: Run the test + clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib validate_model_id`
Expected: PASS.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Do NOT commit** (Task 7 amend).

---

## Task 3: i18n — 6 new keys across 36 locales

**Files:**
- Modify: `src/i18n/locales/en.ts` (canonical, ~L226 block) + the 35 other `locales/*.ts`.

**Interfaces:** none (pure data).

- [ ] **Step 1: Add the canonical keys to `en.ts`**

Add (in the `onboarding.*` section):

```ts
"onboarding.model_choose_title": "Choose your speech model",
"onboarding.model_choose_subtitle": "You can change this later in Settings.",
"onboarding.model_recommended": "Recommended",
"onboarding.model_nemotron_desc": "40 languages (multilingual). Commas, no periods.",
"onboarding.model_retry": "Retry",
"onboarding.model_choose_another": "Choose another model",
```

- [ ] **Step 2: Add the same 6 keys to all 36 locale files (set-equality)**

For each of the 36 locales: add the 6 keys. `ru.ts` gets proper Russian translations:

```ts
"onboarding.model_choose_title": "Выберите модель распознавания",
"onboarding.model_choose_subtitle": "Можно сменить позже в Настройках.",
"onboarding.model_recommended": "Рекомендуется",
"onboarding.model_nemotron_desc": "40 языков (мультиязычный). Запятые, без точек.",
"onboarding.model_retry": "Повторить",
"onboarding.model_choose_another": "Выбрать другую модель",
```

The other 34 locales get the English baseline values (maintains set-equality; `t()` returns them as-is, English is acceptable for untranslated strings; refine later).

- [ ] **Step 3: Verify set-equality + tsc**

Run: `npx tsc --noEmit`
Expected: clean.

- [ ] **Step 4: Do NOT commit.**

---

## Task 4: onboarding.html — step-1 markup

**Files:**
- Modify: `onboarding.html` (the `section.step[data-step="1"]` block, ~L11-40)

**Interfaces:** the element IDs here are consumed by Task 5's `onboarding.ts`.

- [ ] **Step 1: Replace step-1's download-only body with choice + progress + error elements**

Inside `section.step[data-step="1"]`, keep the welcome title + privacy lead, then replace the indeterminate-download block with three sub-blocks toggled by `onboarding.ts`:

```html
<!-- Choice -->
<div id="model-choice">
  <h2 id="welcome-title"></h2>
  <p id="privacy-lead" class="muted"></p>
  <p id="model-choose-subtitle" class="muted small"></p>
  <div class="model-cards">
    <button type="button" class="model-card" data-model="gigaam-v3-e2e-ctc" id="card-gigaam">
      <span class="model-badge" id="badge-gigaam" hidden></span>
      <span class="model-name">GigaAM</span>
      <span class="model-desc" id="gigaam-desc"></span>
      <span class="model-size" id="gigaam-size"></span>
    </button>
    <button type="button" class="model-card" data-model="nemotron-3.5-asr-streaming-0.6b" id="card-nemotron">
      <span class="model-badge" id="badge-nemotron" hidden></span>
      <span class="model-name">Nemotron</span>
      <span class="model-desc" id="nemotron-desc"></span>
      <span class="model-size" id="nemotron-size"></span>
    </button>
  </div>
  <button type="button" class="btn primary" id="confirm-model"></button>
</div>

<!-- Download progress -->
<div id="model-progress" hidden>
  <p id="downloading"></p>
  <div class="progress-bar"><div class="progress-fill" id="progress-fill"></div></div>
  <p id="progress-text" class="muted small"></p>
  <button type="button" class="btn" id="cancel-download"></button>
</div>

<!-- Error -->
<div id="model-error" hidden>
  <p id="model-error-text" class="alert error"></p>
  <button type="button" class="btn primary" id="retry-download"></button>
  <button type="button" class="btn" id="choose-another"></button>
</div>
```

Remove the old `continue-1`/`open-settings-error` engine-error elements from step 1 if they are now redundant (engine-error is handled inside `#model-error`). Keep `#engine-error` text via i18n key `onboarding.engine_error` (reuse) inside `#model-error-text`.

- [ ] **Step 2: Do NOT commit.**

---

## Task 5: onboarding.ts — step-1 logic

**Files:**
- Modify: `src/onboarding.ts` (step-1 section ~L39-99; init ~L198-256)
- Reference: `src/settings/sections/recognition.ts` for the `NEMOTRON_LANGS` map (reuse for ui_lang → locale).

**Interfaces:**
- Consumes: `invoke("onboarding_select_model", { modelId, language })` (Task 2), `invoke("model_status")`, `invoke("cancel_model_download")`, events `model-download-progress` / `model-download-complete` / `model-download-failed` / `engine-ready` / `engine-error`.

- [ ] **Step 1: Add a ui_lang → Nemotron-locale resolver**

Atop `onboarding.ts`, import the locale map and add a pure resolver (Nemotron default language from the UI language; GigaAM ignores it):

```ts
import { NEMOTRON_BY_VALUE, NEMOTRON_LANGS } from "./settings/sections/recognition";
import { fmtBytes } from "./settings/sections/recognition"; // if exported; else inline

const GIGAAM_ID = "gigaam-v3-e2e-ctc";
const NEMOTRON_ID = "nemotron-3.5-asr-streaming-0.6b";

/** Nemotron recognition language from ui_lang: match a locale whose prefix
 *  equals ui_lang, else "auto". GigaAM is RU-hardcoded (returns "auto"). */
function langForModel(modelId: string, uiLang: string): string {
  if (modelId === GIGAAM_ID) return "auto";
  const hit = NEMOTRON_LANGS.find((l) => l.value.toLowerCase().startsWith(uiLang.toLowerCase() + "-"));
  return hit ? hit.value : "auto";
}
```

(If `NEMOTRON_LANGS`/`fmtBytes` are not exported from `recognition.ts`, export them there first — minimal `export` additions. Verify with rg before assuming.)

- [ ] **Step 2: Smart default + render the choice cards in `init`**

After `settings = await invoke("get_settings")`, fetch sizes and pre-select:

```ts
const statuses = await invoke<{ model_id: string; cached: boolean; size_bytes: number }[]>("model_status");
const sizeOf = (id: string) => fmtBytes(statuses.find((s) => s.model_id === id)?.size_bytes ?? 0);
document.getElementById("gigaam-size")!.textContent = sizeOf(GIGAAM_ID);
document.getElementById("nemotron-size")!.textContent = sizeOf(NEMOTRON_ID);
document.getElementById("gigaam-desc")!.textContent = t("models.gigaam_desc");
document.getElementById("nemotron-desc")!.textContent = t("onboarding.model_nemotron_desc");

// Smart default by UI language.
const recommended = asLang(settings.ui_lang) === "ru" ? GIGAAM_ID : NEMOTRON_ID;
let selectedModel = recommended;
document.getElementById(`badge-${recommended === GIGAAM_ID ? "gigaam" : "nemotron"}`)!.hidden = false;
document.getElementById(`card-${recommended === GIGAAM_ID ? "gigaam" : "nemotron"}`)!.classList.add("selected");
updateConfirmButton();
```

- [ ] **Step 3: Card selection + confirm handler**

```ts
function updateConfirmButton(): void {
  document.getElementById("confirm-model")!.textContent =
    t("models.download").replace("{size}", sizeOf(selectedModel));
}
for (const id of [GIGAAM_ID, NEMOTRON_ID]) {
  const which = id === GIGAAM_ID ? "gigaam" : "nemotron";
  document.getElementById(`card-${which}`)!.addEventListener("click", () => {
    selectedModel = id;
    document.querySelectorAll(".model-card").forEach((c) => c.classList.remove("selected"));
    document.getElementById(`card-${which}`)!.classList.add("selected");
    updateConfirmButton();
  });
}
document.getElementById("confirm-model")!.addEventListener("click", () => void startDownload());
```

- [ ] **Step 4: startDownload — invoke the command + wire progress/error listeners**

```ts
let progressUnlisten: UnlistenFn | null = null;
let failedUnlisten: UnlistenFn | null = null;

async function startDownload(): Promise<void> {
  document.getElementById("model-choice")!.hidden = true;
  document.getElementById("model-error")!.hidden = true;
  document.getElementById("model-progress")!.hidden = false;
  document.getElementById("downloading")!.textContent = t("onboarding.downloading");
  document.getElementById("cancel-download")!.textContent = t("models.cancel");
  progressUnlisten = await listen<{ bytes: number; total: number; pct: number }>("model-download-progress", (e) => {
    const { bytes, total, pct } = e.payload;
    document.getElementById<HTMLElement>("progress-fill")!.style.width = `${pct}%`;
    document.getElementById("progress-text")!.textContent =
      t("models.downloading").replace("{bytes}", fmtBytes(bytes)).replace("{total}", fmtBytes(total)).replace("{pct}", String(pct));
  });
  failedUnlisten = await listen("model-download-failed", () => showError());
  await invoke("onboarding_select_model", { modelId: selectedModel, language: langForModel(selectedModel, asLang(settings!.ui_lang)) });
}
```

- [ ] **Step 5: cancel / error / engine-ready handlers**

```ts
document.getElementById("cancel-download")!.addEventListener("click", async () => {
  await invoke("cancel_model_download").catch((e) => console.error(e));
  backToChoice();
});
document.getElementById("retry-download")!.textContent = t("onboarding.model_retry"); // set in applyTranslations
document.getElementById("retry-download")!.addEventListener("click", () => void startDownload());
document.getElementById("choose-another")!.textContent = t("onboarding.model_choose_another");
document.getElementById("choose-another")!.addEventListener("click", () => backToChoice());

function backToChoice(): void {
  if (progressUnlisten) { progressUnlisten(); progressUnlisten = null; }
  if (failedUnlisten) { failedUnlisten(); failedUnlisten = null; }
  document.getElementById("model-progress")!.hidden = true;
  document.getElementById("model-error")!.hidden = true;
  document.getElementById("model-choice")!.hidden = false;
}
function showError(): void {
  if (progressUnlisten) { progressUnlisten(); progressUnlisten = null; }
  if (failedUnlisten) { failedUnlisten(); failedUnlisten = null; }
  document.getElementById("model-progress")!.hidden = true;
  document.getElementById("model-error-text")!.textContent = t("models.download_failed");
  document.getElementById("model-error")!.hidden = false;
}
```

`onEngineReady` (existing) already advances: keep `if (currentStep === 1) showStep(2)`. The existing `engine-ready`/`engine-error` listeners (init ~L204-205) stay. On `engine-error` during step 1, route to `showError()` instead of the old open-settings swap (update `onEngineError`).

- [ ] **Step 6: Skip = accept the recommended model**

In the Skip handler (init ~L243), before `complete()`, send the smart-default choice so the app gets an engine:

```ts
document.getElementById("skip")!.addEventListener("click", async () => {
  await invoke("onboarding_select_model", { modelId: recommended, language: langForModel(recommended, asLang(settings!.ui_lang)) }).catch(() => undefined);
  void complete();
});
```

(`recommended` must be in scope — hoist it to module level alongside `selectedModel`.)

- [ ] **Step 7: tsc + build**

Run: `npx tsc --noEmit` → clean. Run: `npm run build` → exit 0.

- [ ] **Step 8: Do NOT commit.**

---

## Task 6: onboarding.css — card / progress / error styles

**Files:**
- Modify: `src/onboarding.css` (append; use logical CSS properties for RTL)

**Interfaces:** none.

- [ ] **Step 1: Add styles**

```css
.model-cards { display: flex; flex-direction: column; gap: 12px; margin-block-start: 16px; }
.model-card {
  display: flex; flex-direction: column; gap: 4px; text-align: start;
  padding: 14px 16px; border: 2px solid var(--border, #d1d5db); border-radius: 10px;
  background: var(--card, #fff); cursor: pointer; position: relative;
}
.model-card.selected { border-color: var(--accent, #0E7C86); }
.model-badge {
  position: absolute; inset-block-start: -10px; inset-inline-end: 12px;
  background: var(--accent, #0E7C86); color: #fff; font-size: 11px;
  padding: 2px 8px; border-radius: 999px;
}
.model-name { font-weight: 600; }
.model-desc { color: var(--muted, #4B5563); font-size: 13px; }
.model-size { color: var(--muted, #4B5563); font-size: 12px; }
.progress-bar { height: 8px; background: #e5e7eb; border-radius: 999px; overflow: hidden; margin-block-start: 12px; }
.progress-fill { height: 100%; background: var(--accent, #0E7C86); width: 0%; transition: width 0.2s ease; }
.small { font-size: 13px; }
```

- [ ] **Step 2: Verify build**

Run: `npm run build` → exit 0.

- [ ] **Step 3: Do NOT commit.**

---

## Task 7: Full verification + single amend + force-push

**Files:** none (verification + git).

- [ ] **Step 1: Run all gates**

```
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
npx tsc --noEmit
npm run build
```
Expected: fmt clean; clippy clean; 189+1 = 190 tests pass (the new `validate_model_id`); tsc clean; build exit 0.

- [ ] **Step 2: Smoke-test first-run vs returning (manual)**

`cargo tauri dev` with empty `%APPDATA%\com.molvi.app` → onboarding shows choice cards, RU UI pre-selects GigaAM; pick Nemotron → progress bar → engine-ready → step 2. Then `complete_onboarding` (finish), relaunch → app proceeds straight to tray (returning-user auto-proceed, no onboarding). Cancel mid-download → back to cards. Pull network mid-Nemotron → error UI → Retry.

- [ ] **Step 3: Single amend + force-push (one-commit invariant)**

```bash
git add -A   # all tasks' changes (lib.rs, ipc.rs, model_store.rs, onboarding.html, onboarding.ts, onboarding.css, i18n locales, spec, this plan)
git status   # confirm only intended files staged; no stray tooling
git commit --amend --no-edit
git push --force-with-lease origin main
```

- [ ] **Step 4: Rebuild + ship the release**

```bash
gh release delete v0.1.0 --yes             # old draft from prior commit
gh run list --limit 5                       # delete stale runs if desired
gh workflow run release.yml --ref main      # builds from the new amended commit
```

Verify `latest.json` has all 3 platforms after the run; download `molvi_0.1.0_x64-setup.exe`; install on a clean machine (appdata wiped by the NSIS hook) → onboarding model choice appears.

---

## Self-Review (completed)

- **Spec coverage:** smart default (Task 5.2) ✓; deferred cold-start + returning-user fix (Task 1) ✓; `onboarding_select_model` (Task 2) ✓; cards/progress/error UI (Tasks 4-5) ✓; recognition-language default (Task 5.1 `langForModel`) ✓; skip = recommended (Task 5.6) ✓; cancel via existing `model_download` (Task 1.3) ✓; i18n 6 keys ×36 (Task 3) ✓.
- **Placeholders:** none — every step has concrete code or an exact command.
- **Type consistency:** `model_selection_tx: Mutex<Option<Sender<(String,String)>>>` used identically in Task 1 (struct + send) and Task 2 (send). `onboarding_select_model(model_id, language)` matches the frontend `invoke("onboarding_select_model", { modelId, language })` (Tauri snake_cases param names → camelCase on the JS side). `selectedModel`/`recommended` hoisted to module scope in Task 5.
- **Open verification points (called out in steps, not assumed):** `settings::save` exact name (Task 2.6); `NEMOTRON_LANGS`/`fmtBytes` export status (Task 5.1).
