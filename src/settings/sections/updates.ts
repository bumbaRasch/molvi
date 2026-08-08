import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";

import { Button, SettingsGroup, Toggle, toast } from "../ui";
import type { CheckResult, SectionBuilder } from "../types";
import { errText, patcher } from "../persist";
import { t } from "../../i18n";

export const buildUpdates: SectionBuilder = (store) => {
  const patch = patcher(store);
  const s = store.get().settings!;

  const versionLine = document.createElement("div");
  versionLine.className = "muted";
  versionLine.textContent = t("updates.version_loading");
  // R1: idiomatic core-app getVersion (capability core:app:default added).
  void getVersion()
    .then((v) => { versionLine.textContent = t("updates.version").replace("{v}", v); })
    .catch((e) => {
      versionLine.textContent = t("updates.version_na");
      console.error("getVersion failed", e); // metadata-only
    });

  const checkOnStartup = Toggle(
    t("updates.check_startup"),
    s.updater.check_on_startup,
    (v) => patch((n) => { n.updater.check_on_startup = v; }),
  );

  const checkBtn = Button(t("updates.check_now"), () => void check());
  const applyBtn = Button(t("updates.apply"), () => void apply());
  applyBtn.classList.add("hidden"); // appears only when a check finds an update

  // Toggle + check button on one row (space-between): toggle left, button right.
  const checkRow = document.createElement("div");
  checkRow.className = "row-between";
  checkRow.append(checkOnStartup.wrap, checkBtn);

  async function check(): Promise<void> {
    checkBtn.disabled = true;
    try {
      const res = await invoke<CheckResult>("check_update");
      if (res.up_to_date) {
        toast("info", t("updates.up_to_date"));
        applyBtn.classList.add("hidden");
      } else {
        toast(
          "warning",
          t("updates.available")
            .replace("{new}", res.version ?? "")
            .replace("{current}", res.current_version),
        );
        applyBtn.classList.remove("hidden");
      }
    } catch (e) {
      toast("error", t("updates.check_error").replace("{msg}", errText(e)));
    } finally {
      checkBtn.disabled = false;
    }
  }

  async function apply(): Promise<void> {
    applyBtn.disabled = true;
    try {
      await invoke("apply_update");
      // apply_update restarts the app on a real install (diverges); this only
      // returns on the no-op path (nothing staged) — word accordingly.
      toast("info", t("updates.no_update"));
    } catch (e) {
      toast("error", t("updates.install_error").replace("{msg}", errText(e)));
    } finally {
      applyBtn.disabled = false;
    }
  }

  const group = SettingsGroup(t("updates.title"), [versionLine, checkRow, applyBtn], t("updates.title_tip"));
  const root = document.createElement("div");
  root.append(group);
  return { el: root };
};
