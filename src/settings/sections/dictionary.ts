import { invoke } from "@tauri-apps/api/core";

import { Button, toast } from "../ui";
import type { DictEntry, ImportPreview } from "../types";
import { t } from "../../i18n";
import { buildListSection, type ListSectionDeps } from "./list-section";

// Dictionary import = 2-IPC preview (file pick → counts → confirm → apply).
// Dictionary-scale polish: with hundreds of rows the user needs to see conflict
// counts before clobbering. Contrast snippets' atomic import. Kept out of the
// shared factory (the import flow is the one genuine difference between sections).
function dictionaryImport({ reload, showError, importBtn }: ListSectionDeps): void {
  void (async () => {
    let prev: ImportPreview | null = null;
    try {
      prev = await invoke<ImportPreview | null>("dictionary_import_preview");
    } catch (e) {
      showError(e);
      return;
    }
    if (!prev) return;

    const panel = document.createElement("div");
    panel.className = "import-preview";
    const text = document.createElement("span");
    text.className = "muted";
    text.textContent = t("dictionary.preview_text")
      .replace("{total}", String(prev.total))
      .replace("{new}", String(prev.new))
      .replace("{conflicts}", String(prev.conflicts));
    const confirmBtn = Button(t("common.import"), () => void apply(prev!.path, panel));
    const cancelBtn = Button(t("common.cancel"), () => restoreImportButton(panel));
    panel.append(text, confirmBtn, cancelBtn);
    importBtn.replaceWith(panel);
  })();

  async function apply(path: string, panel: HTMLElement): Promise<void> {
    try {
      await invoke("dictionary_import_apply", { path });
      toast("success", t("dictionary.imported"));
      restoreImportButton(panel);
      await reload();
    } catch (e) {
      showError(e);
      restoreImportButton(panel);
    }
  }

  function restoreImportButton(panel: HTMLElement): void {
    panel.replaceWith(importBtn);
  }
}

export const buildDictionary = buildListSection<DictEntry>({
  titleKey: "dictionary.title",
  titleTipKey: "dictionary.title_tip",
  filterKey: "dictionary.filter",
  filterPhKey: "dictionary.filter_ph",
  keyLabelKey: "dictionary.entry",
  keyPhKey: "dictionary.entry_ph",
  valueLabelKey: "dictionary.replacement",
  valuePhKey: "dictionary.replacement_ph",
  emptyKey: "common.empty_dict",
  emptyKeyErr: "dictionary.empty_entry",
  addedKey: "dictionary.added",
  removedKey: "dictionary.removed",
  undoKey: "dictionary.undo",
  exportedKey: "dictionary.exported",
  keyField: "entry",
  valueField: "replacement",
  listCmd: "dictionary_list",
  addCmd: "dictionary_add",
  removeCmd: "dictionary_remove",
  exportCmd: "dictionary_export",
  runImport: dictionaryImport,
});
