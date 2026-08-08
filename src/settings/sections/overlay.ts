import { invoke } from "@tauri-apps/api/core";
import { Button, SettingsGroup, TextInput, Toggle } from "../ui";
import type { SectionBuilder } from "../types";
import { patcher } from "../persist";
import { t } from "../../i18n";

export const buildOverlay: SectionBuilder = (store) => {
  const patch = patcher(store);
  const s = store.get().settings!;

  const enabled = Toggle(t("overlay.enabled"), s.overlay.enabled, (v) => patch((n) => { n.overlay.enabled = v; }));
  const waveform = Toggle(t("overlay.waveform"), s.overlay.show_waveform, (v) => patch((n) => { n.overlay.show_waveform = v; }));
  const timer = Toggle(t("overlay.timer"), s.overlay.show_timer, (v) => patch((n) => { n.overlay.show_timer = v; }));

  const soundsEnable = Toggle(t("overlay.sounds"), s.overlay.sounds.enabled, (v) => patch((n) => { n.overlay.sounds.enabled = v; }));
  const startRow = soundRow(t("overlay.sound_start"), s.overlay.sounds.start, (v) => patch((n) => { n.overlay.sounds.start = v; }));
  const stopRow = soundRow(t("overlay.sound_stop"), s.overlay.sounds.stop, (v) => patch((n) => { n.overlay.sounds.stop = v; }));
  const sounds = SettingsGroup(t("overlay.sounds"), [soundsEnable.wrap, startRow, stopRow], t("overlay.sounds_tip"));

  const group = SettingsGroup(t("overlay.title"), [enabled.wrap, waveform.wrap, timer.wrap], t("overlay.title_tip"));
  const root = document.createElement("div");
  root.append(group, sounds);
  return { el: root };
};

// A labelled path input + [Обзор] (pick a .wav via the OS dialog) + [Сбросить]
// (clear → synthesized default). The reset button is hidden while the path is
// empty (default), and shown as soon as a custom path is set. Mirrors Hotkey's
// `.field-row` + `.row-end` layout. `apply` persists the value to the store.
function soundRow(label: string, value: string, apply: (v: string) => void): HTMLElement {
  const resetBtn = Button(t("common.reset"), () => {
    ctrl.set("");
    apply("");
    resetBtn.classList.add("hidden");
  });
  if (value === "") resetBtn.classList.add("hidden");
  const ctrl = TextInput(
    label,
    value,
    (v) => {
      apply(v);
      resetBtn.classList.toggle("hidden", v === "");
    },
    { placeholder: t("overlay.sound_ph") },
  );
  const browseBtn = Button(t("common.browse"), async () => {
    const picked = await invoke<string | null>("pick_sound_file");
    if (picked) {
      ctrl.set(picked);
      apply(picked);
      resetBtn.classList.remove("hidden");
    }
  });
  const btnRow = document.createElement("div");
  btnRow.className = "row-end";
  btnRow.append(resetBtn, browseBtn);
  const row = document.createElement("div");
  // `sound-row` widens the path input (see settings.css) — `.field-row` alone
  // keeps the input natural-width (wanted for Hotkey's shortcut field).
  row.className = "field-row sound-row";
  row.append(ctrl.wrap, btnRow);
  return row;
}
