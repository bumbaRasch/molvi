import { invoke } from "@tauri-apps/api/core";

import { toast } from "../ui";
import type { SnippetEntry } from "../types";
import { t } from "../../i18n";
import { buildListSection, type ListSectionDeps } from "./list-section";

// Atomic import: pick → apply → toast. No preview (small-scale feature) —
// snippets are signatures/addresses, not hundreds of rows. Contrast the
// dictionary's 2-IPC preview split.
function snippetImport({ reload, showError }: ListSectionDeps): void {
  void (async () => {
    try {
      await invoke("snippet_import");
      toast("success", t("snippets.imported"));
      await reload();
    } catch (e) {
      showError(e);
    }
  })();
}

export const buildSnippets = buildListSection<SnippetEntry>({
  titleKey: "snippets.title",
  titleTipKey: "snippets.title_tip",
  filterKey: "snippets.filter",
  filterPhKey: "snippets.filter_ph",
  keyLabelKey: "snippets.cue",
  keyPhKey: "snippets.cue_ph",
  valueLabelKey: "snippets.expansion",
  valuePhKey: "snippets.expansion_ph",
  emptyKey: "common.empty_snip",
  emptyKeyErr: "snippets.empty_cue",
  addedKey: "snippets.added",
  removedKey: "snippets.removed",
  undoKey: "snippets.undo",
  exportedKey: "snippets.exported",
  keyField: "cue",
  valueField: "expansion",
  listCmd: "snippet_list",
  addCmd: "snippet_add",
  removeCmd: "snippet_remove",
  exportCmd: "snippet_export",
  runImport: snippetImport,
});
