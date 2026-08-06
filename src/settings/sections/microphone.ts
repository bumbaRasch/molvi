import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { Button, Select, type SelectCtrl, SettingsGroup, toast } from "../ui";
import type { SectionBuilder } from "../types";
import { errText, patcher } from "../persist";
import { t } from "../../i18n";

export const buildMicrophone: SectionBuilder = (store) => {
  const patch = patcher(store);

  const devHost = document.createElement("div");
  let devCtrl: SelectCtrl | null = null;

  const buildDevices = async (): Promise<void> => {
    let devices: string[];
    try {
      devices = await invoke<string[]>("list_audio_devices");
    } catch (e) {
      toast("error", t("common.error_prefix") + errText(e));
      return;
    }
    const cur = store.get().settings!.audio.input_device ?? "";
    const opts = [
      { value: "", label: t("microphone.device_default") },
      ...devices.map((d) => ({ value: d, label: d })),
    ];
    const ctrl = Select(t("microphone.device"), opts, cur, (v) => patch((n) => { n.audio.input_device = v || null; }));
    if (devCtrl) devCtrl.wrap.replaceWith(ctrl.wrap);
    else devHost.append(ctrl.wrap);
    devCtrl = ctrl;
  };

  const refresh = Button(t("common.refresh"), () => { void buildDevices(); });

  const devRow = document.createElement("div");
  devRow.className = "device-row";
  devRow.append(devHost, refresh);

  // R2 + mic-preview: meter (label + bar) left, on-demand toggle right. The
  // meter is driven by the EXISTING mic-level event, which the poller emits
  // while the overlay is visible (recording) OR preview is on. `.row-between`
  // + inline flex keeps settings.css untouched (ponytail: no new class).
  const meterLabel = document.createElement("span");
  meterLabel.className = "muted";
  meterLabel.textContent = t("microphone.level");
  const fill = document.createElement("div");
  fill.className = "level-fill";
  const bar = document.createElement("div");
  bar.className = "level-bar";
  bar.append(fill);
  const meter = document.createElement("div");
  meter.className = "meter";
  meter.style.flex = "1 1 auto";
  meter.append(meterLabel, bar);

  // On-demand preview: toggles capture without recording so the user can test
  // mic volume. Resets the bar to 0 on stop (no events flow while off).
  let previewOn = false;
  const previewBtn = Button(t("microphone.preview"), () => {
    previewOn = !previewOn;
    void invoke("set_mic_preview", { enabled: previewOn });
    previewBtn.textContent = previewOn ? t("microphone.preview_stop") : t("microphone.preview");
    if (!previewOn) fill.style.width = "0%";
  });

  const meterRow = document.createElement("div");
  meterRow.className = "row-between";
  meterRow.append(meter, previewBtn);

  const group = SettingsGroup(t("microphone.title"), [devRow, meterRow], t("microphone.title_tip"));
  const root = document.createElement("div");
  root.append(group);

  let unlisten: (() => void) | null = null;
  let disposed = false; // TOCTOU guard: cleanup may run before listen() resolves.
  void (async () => {
    try {
      const fn = await listen<{ level: number }>("mic-level", (e) => {
        // ponytail: level ≈ RMS×1000 (0..1000); cosmetic /10 → %. Not calibrated.
        fill.style.width = `${Math.min(100, e.payload.level / 10)}%`;
      });
      if (disposed) fn(); // pane left during await — drop the just-resolved listener
      else unlisten = fn;
    } catch (e) {
      console.error("mic-level listen failed", e); // metadata-only
    }
  })();

  void buildDevices();

  return {
    el: root,
    cleanup: () => {
      disposed = true;
      unlisten?.();
      // Stop capture if the user navigated away while previewing (no leak).
      if (previewOn) void invoke("set_mic_preview", { enabled: false });
    },
  };
};
