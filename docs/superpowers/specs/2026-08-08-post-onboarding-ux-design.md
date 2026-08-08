# Post-onboarding UX — design

**Date:** 2026-08-08
**Status:** Approved (brainstormed), pending spec review
**Backward compatibility:** Not required. Free to change `complete_onboarding`'s signature
and the post-completion surface.
**Sibling:** `2026-08-08-onboarding-model-selection-design.md` (step-1 model choice — the
context for *why* a Skip-from-step-1 user lands engine-less and needs orientation).

## Problem

`complete_onboarding` (`src-tauri/src/ipc.rs:489`) — invoked by both **Skip** and **Done** —
sets `onboarded=true`, sends `Command::Cancel`, resets `onboarding_practice`/`mic_preview`,
hides the onboarding window, and **nothing else**. The app is left tray-only.

A user who **Skips from step 1** (the realistic fast path): never saw the hotkey, never tested
the mic, never said a first word, and the chosen model is still downloading in the background
(Nemotron ≈2.4 GB; the hotkey is not even registered yet — it registers only after engine spawn
in the bg thread, `lib.rs:750`). They face a silent tray icon and no notion of "what next." Even
a **Done** user, though oriented by step 3's `all_set` copy, has the window vanish with no
confirmation that the app is alive and where it lives.

There is no OS-notification plugin (spec §10.1 / AGENTS.md: no toast dep). The only rich surfaces
are the **Settings window** (which already mounts the vanilla `toast()` primitive,
`src/settings/ui.ts:219`) and the **tray tooltip** (requires hover, nearly invisible).

## Goal

After `complete_onboarding`, always surface a visible "home" surface (Settings) plus one
context-aware toast that tells the user (a) molvi lives in the tray, and (b) the hotkey to
dictate — or, if the engine is not ready yet, that dictation unlocks shortly. The toast must
cover the Skip-from-step-1 edge case so the user does not press an unregistered hotkey and
conclude "it's broken."

## Decision

**B (always surface Settings) + one smart toast.** Both Skip and Done show the Settings window;
the toast text adapts to engine readiness. Single code path, consistent with the returning-user
manual-launch behavior (`lib.rs:516` — `onboarded && !autostarted` → Settings). The Settings
window is a non-blocking reference surface: the global hotkey is OS-level (RegisterHotKey /
GlobalShortcut), focus-independent, so the user can dictate immediately even with Settings open.

Rejected alternatives:
- **Skip-only orientation (differ per path):** better-targeted but adds a branch for marginal
  gain; Done users also benefit from a visible home + confirmation toast.
- **New in-onboarding "all set" final screen (option C):** best polish but most scope (new DOM
  section + logic + i18n keys) for a one-time moment that a toast already covers.

## Flow

```
Skip / Done click
   │  onboarding.ts complete()
   ▼
invoke("complete_onboarding", { ready: engineReady })      ← frontend knows engineReady
   │
   ▼  complete_onboarding (ipc.rs)
   ├── onboarded = true, persist
   ├── Command::Cancel, reset onboarding_practice / mic_preview
   ├── hide onboarding window
   ├── tray::show_settings(&app)                           ← surface the home window
   └── app.emit("post-onboarding-hint", { ready, hotkey }) ← json! payload (metadata-only)
            │
            ▼  settings/main.ts (listener registered at startup)
            └── toast( ready ? success: all_set  :  info: toast_preparing )
```

## Architecture

### `ready` source: the frontend's `engineReady`

`onboarding.ts` already tracks `engineReady` (`onboarding.ts:24`), set `true` only by the
`engine-ready` event handler `onEngineReady` (`onboarding.ts:125`). That event fires once from
the bg thread after download + engine spawn + hotkey register (`lib.rs:774`). So `engineReady`
maps exactly to "can the user dictate right now":

| Path at completion | `engineReady` | Toast |
|---|---|---|
| Skip from step 1 (download just started) | `false` | info — `onboarding.toast_preparing` |
| Skip from step 2/3 (step 1 gates on engine-ready) | `true` | success — `onboarding.all_set` |
| Done (step 3 reached ⇒ engine-ready fired) | `true` | success — `onboarding.all_set` |

The frontend passes it: `invoke("complete_onboarding", { ready: engineReady })`.

### Why the flag comes from the frontend (not Rust-side detection)

Skip calls `onboarding_select_model` (sends the choice on the channel, returns) and then
`complete_onboarding` **back-to-back** (`onboarding.ts:413`). The bg thread receives the channel
message and spawns the abortable download task, storing its `JoinHandle` in
`AppState.model_download` (`lib.rs:183`, `lib.rs:679`) **asynchronously**. At the instant
`complete_onboarding` runs, the bg thread may not have stored the handle yet, so
`model_download.is_some() && !is_finished()` is unreliable. The frontend's `engineReady` is
race-free (set by an event that has either fired or not). One bool across the IPC boundary is
cheaper and more correct than a new `AtomicBool` in `AppState` plus a bg-thread write.

### `complete_onboarding` changes (`ipc.rs:489`)

Add a `ready: bool` parameter; after the existing logic, call `crate::tray::show_settings(&app)`
(`tray.rs:170`, already `pub(crate)`) and `app.emit("post-onboarding-hint", serde_json::json!({
"ready": ready, "hotkey": <settings.hotkey> }))`. `hotkey` is read from `state.settings` (held
briefly). The fully-qualified `serde_json::json!` matches `model_store.rs:161`; `Emitter` is
already in scope (`ipc.rs:10`).

### `settings/main.ts` listener (`main.ts:133` neighbors)

Add `import { toast } from "./ui";` and register, alongside the existing
`listen("navigate-history")` / `listen("ui-lang-changed")`:

```ts
void listen<{ ready: boolean; hotkey: string }>("post-onboarding-hint", (e) => {
  const { ready, hotkey } = e.payload;
  if (ready) toast("success", t("onboarding.all_set").replace("{hotkey}", hotkey));
  else toast("info", t("onboarding.toast_preparing").replace("{hotkey}", hotkey));
});
```

`toast()` auto-mounts the toaster via `ensureToaster()` (`ui.ts:207`) on first call, so no
explicit mount is needed. The listener is registered at settings-window load.

### Window lifecycle (load-bearing, doc-verified)

All three windows are declared in `tauri.conf.json` with `visible: false` and **no `create`
field**. Per the Tauri 2 config reference, `create` defaults to **`true`**, meaning each window
is **initialized at app startup** — its webview loads and its JS runs — regardless of
`visible:false` (the splashscreen→main pattern relies on exactly this). Therefore
`settings/main.ts` has already executed and registered its `post-onboarding-hint` listener
**before** first-run onboarding completes. `complete_onboarding`'s `show_settings` only flips
visibility on an already-loaded window; the subsequent `emit` reaches the live listener and
renders the toast.

## Doc verification (anti-stale rule, AGENTS.md)

Verified via ctx7 `/websites/v2_tauri_app` (2 queries, 2026-08-08):
1. **Window init at startup:** config reference — *"The `create` setting determines if a window
   is initialized at app startup. If set to false, the window must be manually created…"* —
   `create` defaults `true` → `visible:false` windows load webview + JS at startup. Confirms the
   listener is live before onboarding completes.
2. **Global emit + typed listen:** `develop/calling-frontend` — `use tauri::{AppHandle,
   Emitter};` + `app.emit("event", &payload)` (all listeners, all webviews); frontend
   `listen<T>("event", e => e.payload)`. Struct payloads use `#[derive(Clone, Serialize)]`; the
   `json!` macro (existing in `overlay.rs`) is the lighter equivalent for ad-hoc fields.
3. Command param casing (`ready`, single word) is camelCase≡snake_case — no conversion concern;
   the `modelId`→`model_id` mapping already in `onboarding_select_model` proves the mechanism.

## i18n

Reuse `onboarding.all_set` (en: "You're all set. Press {hotkey} anywhere to dictate.",
`en.ts:219`) for the success toast. **One new key**, `onboarding.toast_preparing`, en canonical:

| Key | en value |
|---|---|
| `onboarding.toast_preparing` | `molvi is still preparing. Press {hotkey} to dictate when it's ready.` |

Humanizer-reviewed (`humanizer-zh` skill): two short sentences, no em dash (AI-tell #13); mirrors the sibling `onboarding.all_set` ("You're all set. Press {hotkey} anywhere to dictate."). Don't "restore" an em dash.

Added to all 36 locales (`src/i18n/locales/*.ts`), set-equal with `en`. `{hotkey}` token is
ASCII-verbatim in every locale (including RTL + CJK), per project convention. Model/engine names
are proper nouns, untranslated.

## Privacy (§10.1)

The `post-onboarding-hint` payload is `{ ready: bool, hotkey: String }`. The hotkey is a
configuration key-combo (e.g. `Alt+\``) — metadata, not transcript/audio/dictation/snippet/
dictionary content. `ready` is a trivial boolean. Neither crosses any logging site; the emit is
an in-process event to a webview, never `tracing::`. The existing `complete_onboarding` logs
only `"onboarding complete"` (metadata). Substrate-compliant.

## Performance (blaze)

Zero hot-loop impact. `complete_onboarding` is a one-shot first-run finalization; `show_settings`
+ `emit` + `toast` are one-shot UI operations on a first-run-only path. The default
RU/PTT/Smart path (capture → engine → finalize → paste, RTF ≤ 0.03) is byte-for-byte untouched.
No new `AppState` field, no hot-path read, no allocation on the inference path.

## Files affected

- `src-tauri/src/ipc.rs` — `complete_onboarding` gains `ready: bool`; calls
  `tray::show_settings(&app)` + `app.emit("post-onboarding-hint", serde_json::json!({ ready,
  hotkey }))`. (`Emitter` already in scope, `ipc.rs:10`.)
- `src/onboarding.ts` — `complete()` passes `{ ready: engineReady }` to `complete_onboarding`.
- `src/settings/main.ts` — `import { toast } from "./ui"`; register the
  `post-onboarding-hint` listener alongside the existing global listeners.
- `src/i18n/locales/*.ts` × 36 — add `onboarding.toast_preparing` (en canonical, set-equal).
- No schema change (`Settings` untouched). No `tauri.conf.json` change. No new window.

## Out of scope

- A delayed "now ready!" toast fired when the engine later comes online (for the
  Skip-from-step-1 waiter). The tray status already flips Warming → Ready (`tray::set_status`)
  and Settings → Recognition shows live download progress; a proactive second toast is a possible
  future enhancement, not this change.
- Changing Skip semantics in the model-selection flow (Skip = accept the recommended model —
  unchanged; this design only changes what is *shown after*, via `complete_onboarding`).
- OS-level notifications (would need a new plugin; explicitly out per AGENTS.md).
- Navigating Settings to a specific section on completion (the toast carries the hotkey; no
  guided navigation needed).
