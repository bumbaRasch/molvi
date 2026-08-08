import { captureHotkey } from "../hotkey-capture";
import { Button, SettingsGroup, TextInput, Toggle } from "../ui";
import type { SectionBuilder } from "../types";
import { patcher } from "../persist";
import { t } from "../../i18n";

export const buildHotkey: SectionBuilder = (store) => {
  const patch = patcher(store);
  const s = store.get().settings!;

  const hotkeyIn = TextInput(t("hotkey.combo"), s.hotkey, (v) => patch((n) => { n.hotkey = v; }), { placeholder: t("hotkey.combo_ph") });

  // Primary (accent) so the record action is visually distinct from the plain
  // "Отмена" next to it — two same-styled buttons read as "one and the same".
  const captureBtn = Button(t("hotkey.record"), () => handle[captureArmed ? "cancel" : "start"]());
  captureBtn.classList.add("primary");
  // Cancel button: visible affordance to abort capture (mirrors Esc).
  const cancelBtn = Button(t("common.cancel"), () => handle.cancel());
  const btnRow = document.createElement("div");
  btnRow.className = "row-end";
  btnRow.append(cancelBtn, captureBtn);
  const hotkeyRow = document.createElement("div");
  hotkeyRow.className = "field-row";
  hotkeyRow.append(hotkeyIn.wrap, btnRow);

  // Track armed state for the toggle label (the helper owns the real flag).
  let captureArmed = false;
  const handle = captureHotkey({
    onCombo: (combo) => {
      hotkeyIn.set(combo);
      patch((n) => { n.hotkey = combo; });
    },
    onStateChange: (armed) => {
      captureArmed = armed;
      captureBtn.textContent = armed ? t("hotkey.recording") : t("hotkey.record");
    },
  });
  // ponytail: capture covers common keys (letters, digits, F1-F12, punctuation,
  // modifiers). The TextInput stays manually editable as a fallback for anything
  // the serializer doesn't cover; full key-code table omitted (YAGNI).

  const altgr = Toggle(
    t("hotkey.altgr_mirror"),
    s.hotkey_altgr_mirror,
    (v) => patch((n) => { n.hotkey_altgr_mirror = v; }),
    t("hotkey.altgr_mirror_tip"),
  );

  const group = SettingsGroup(t("hotkey.title"), [hotkeyRow, altgr.wrap], t("hotkey.title_tip"));
  const root = document.createElement("div");
  root.append(group);
  return {
    el: root,
    cleanup: () => handle.cleanup(),
  };
};
