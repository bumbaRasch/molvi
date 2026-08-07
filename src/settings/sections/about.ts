import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

import { SettingsGroup } from "../ui";
import type { SectionBuilder } from "../types";
import { t } from "../../i18n";

// Proper nouns / repo names — not localized. Opened in the system browser via
// tauri-plugin-opener (capability scopes opener:allow-open-url to these
// domains). Nemotron first per project priority.
const ACKS: { name: string; url: string }[] = [
  { name: "Nemotron", url: "https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b" },
  { name: "parakeet-rs", url: "https://github.com/altunenes/parakeet-rs" },
  { name: "GigaAM", url: "https://github.com/salute-developers/GigaAM" },
  { name: "transcribe-rs", url: "https://github.com/cjpais/transcribe-rs" },
  { name: "ONNX Runtime", url: "https://github.com/microsoft/onnxruntime" },
  { name: "Tauri", url: "https://github.com/tauri-apps/tauri" },
];

export const buildAbout: SectionBuilder = () => {
  const versionLine = document.createElement("div");
  versionLine.className = "muted";
  // "molvi" is the brand name — kept hardcoded (not localized).
  versionLine.textContent = "molvi";
  void getVersion()
    .then((v) => { versionLine.textContent = `molvi ${v}`; })
    .catch((e) => { console.error("getVersion failed", e); }); // metadata-only

  const desc = document.createElement("p");
  desc.textContent = t("about.desc");

  const ack = document.createElement("p");
  ack.className = "muted acks";
  ACKS.forEach((a, i) => {
    if (i > 0) ack.append(", ");
    const link = document.createElement("a");
    link.href = a.url;
    link.textContent = a.name;
    link.title = a.url;
    // External navigation goes through the opener plugin (CSP blocks direct
    // webview nav). preventDefault stops the in-webview navigation attempt;
    // openUrl routes to the system default browser.
    link.addEventListener("click", (e) => {
      e.preventDefault();
      void openUrl(a.url);
    });
    ack.append(link);
  });

  const main = SettingsGroup(t("about.title"), [versionLine, desc], t("about.title_tip"));
  const ackGroup = SettingsGroup(t("about.acks_title"), [ack], t("about.acks_tip"));

  const root = document.createElement("div");
  root.append(main, ackGroup);
  return { el: root };
};
