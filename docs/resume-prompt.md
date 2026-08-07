# Resume prompt — paste into the fresh session

> Copy everything in the block below (starting at "СТАРТ") and paste it as your
> first message in the new session. The fresh session has no memory of this one —
> this prompt + the committed docs carry all the state.

---

СТАРТ

Продолжаем работу над molvi (Windows 11 push-to-talk dictation, Tauri 2 + local CPU ASR). Это — свежая сессия после брейнсторма Track A (мульти-платформенный порт, OSS). Дизайн-фаза ПОЛНОСТЬЮ завершена и закоммичена. Сейчас — фаза ПЛАНА.

## Что прочитать ПЕРВЫМ делом (по порядку, обязательно)
1. `AGENTS.md` — библия проекта (toolchain, deps, архитектура, privacy §10.1, blaze NFR, правила верификации доков). Она уже поправлена в этом брейнсторме.
2. `docs/next-session-handoff.md` — карта восстановления. Содержит: что сделано, точные следующие шаги, gate-команды, binary-lock caveat, OPEN-решения.
3. `docs/superpowers/specs/2026-08-07-molvi-multiplatform-port-design.md` — **THE spec** (Track A дизайн: решения D1–D6, doc-верифицированная crate-матрица на август 2026, 3 блокера, 3 спайка, inline-cfg архитектура, per-platform specifics, Wayland scoping OPEN, NFR).
4. `docs/superpowers/specs/2026-08-07-paste-focus-guard-spike.md` — спайк #3 (paste focus-guard: macOS ⌘V, tauri-nspanel, verify/restore/no-restore форма).
5. `docs/mobile-strategy.md` — мобайл = отдельный продукт (не сейчас).

## Главные правила (HARD)
- **НИЧЕГО не делай по памяти.** Каждый crate/API/сигнатуру — перепроверяй через skill `find-docs` (ctx7: `npx ctx7@latest …`) + docs.rs/crates.io на август 2026. AGENTS.md перечисляет живые ctx7-id: `/pykeio/ort` (НЕ `/pyke.io/ort`), `/enigo-rs/enigo`, `/websites/v2_tauri_app`, `/cjpais/transcribe-rs`, `/altunenes/parakeet-rs`. Для multi-model crate'ов ctx7-autodocs ненадёжны — сверяй с исходником в `~/.cargo/registry`.
- **Не пиши код до плана.** Последовательность: brainstorm → spec (✅ готов) → **writing-plans** → execute.
- **Blaze = PERFORMANCE NFR, не совместимость.** Главный код можно рефакторить (обратная совместимость не нужна), но дефолтный RU/PTT/Smart путь держит RTF ≤ 0.03 + hot-loop без аллокаций/локов/blocking — замером на каждой платформе. Nemotron feeds ONLY на 8960-sample boundary (не трогать).
- **Privacy §10.1 HARD RULE:** никогда не логировать transcript/partials/post-proc/dict/history/snippet/command/prompt — никакой уровень. 6 `log_privacy` субстратов держать зелёными.
- **Архитектура D2:** inline `#[cfg(target_os=...)]` per feature module + `[target.'cfg(windows)'.dependencies]` в Cargo.toml. **НЕТ `mod platform`** (doc-верерифицировано как premature для 6 single-use fn).

## СЛЕДУЮЩИЙ ШАГ = вызвать skill `writing-plans`
Превратить spec в пофазный план реализации → `docs/superpowers/plans/2026-08-07-molvi-multiplatform-port.md`. Предложенное секционирование (из handoff §3):
- **Phase 1** — разблокировать кросс-платформенные сборки: файлы лицензии (`MIT OR Apache-2.0`); **Step 0** (cfg-gate 4 Win32-сайтов: `audio.rs:6-7`, `ort_affinity.rs:10,14`, `profiles.rs:13-18`, `paste.rs:9-10`; `model_store.rs:214` уже сделан); **CI matrix** (`.github/workflows`: windows/macos-14/ubuntu — CI ЕСТЬ механизм спайков #1/#2).
- **Phase 2** — macOS порт (Apple Silicon): tauri-nspanel (overlay focusable:false сломан, tauri#14102), enigo Accessibility-permission, paste ⌘V (Key::Command).
- **Phase 3** — Linux/Wayland (Wayland scoping OPEN: portal vs evdev).
После плана — execute (через `executing-plans` / `subagent-driven-development`).

## Gate-команды (для любого кода)
`cargo fmt` + `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` + `cargo test --manifest-path src-tauri/Cargo.toml --lib` + (binary-unlocked) `cargo test --test log_privacy` + `npx tsc --noEmit` + `npm run build`. Binary-lock: НЕ убивай запущенный `cargo tauri dev` — если держит molvi.exe, юзай `cargo check --all-targets` + `cargo test --lib`.

## СТАРТ
Прочитай AGENTS.md + handoff + spec + spike #3, подтверди мне понимание плана в 5–7 строках, затем вызови skill `writing-plans` и начни превращать spec в пофазный план. Задавай уточняющие вопросы по ходу. Ничего по памяти — всё верифицируй через find-docs/ctx7.

КОНЕЦ
