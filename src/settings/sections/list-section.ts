import { invoke } from "@tauri-apps/api/core";

import { Button, SettingsGroup, TextInput, toast } from "../ui";
import { errText } from "../persist";
import type { SectionBuilder } from "../types";
import { t } from "../../i18n";

// Shared list-editor scaffolding for the dictionary + snippets sections, which
// are the same UI (list + add/edit form + live filter + undo-delete + import/
// export) parameterized over their field names, IPC commands, i18n keys, and
// import strategy (dictionary = 2-IPC preview; snippets = atomic). Collapses
// ~150 lines of hand-rolled duplication. Shares the .dic-* CSS classes — they
// are generic list/form layout, not dictionary-specific.

export interface ListSectionDeps {
  reload: () => Promise<void>;
  showError: (e: unknown) => void;
  importBtn: HTMLButtonElement;
}

export interface ListSectionConfig<E> {
  // i18n keys
  titleKey: string;
  titleTipKey: string;
  filterKey: string;
  filterPhKey: string;
  keyLabelKey: string;
  keyPhKey: string;
  valueLabelKey: string;
  valuePhKey: string;
  emptyKey: string; // non-filtered empty state
  emptyKeyErr: string; // "empty key" validation error
  addedKey: string;
  removedKey: string;
  undoKey: string;
  exportedKey: string;
  // Field names — double as the IPC invoke param names (Rust commands declare
  // matching param names: dictionary_add(entry, replacement) / snippet_add(cue, expansion)).
  keyField: keyof E & string;
  valueField: keyof E & string;
  // IPC commands
  listCmd: string;
  addCmd: string;
  removeCmd: string;
  exportCmd: string;
  // Import strategy differs (dictionary = preview panel; snippets = atomic).
  runImport: (deps: ListSectionDeps) => void;
}

export const buildListSection = <E>(cfg: ListSectionConfig<E>): SectionBuilder => () => {
  const keyOf = (it: E): string => String(it[cfg.keyField]);
  const valOf = (it: E): string => String(it[cfg.valueField]);

  const root = document.createElement("div");
  const listHost = document.createElement("div");
  listHost.className = "dic-list";

  let loaded: E[] = []; // cached for live filter + undo
  const filterInput = TextInput(
    t(cfg.filterKey), "", () => {},
    { placeholder: t(cfg.filterPhKey) },
  );
  filterInput.wrap.querySelector("input")!.addEventListener("input", (e) => {
    const q = (e.target as HTMLInputElement).value.trim().toLowerCase();
    renderList(loaded.filter((it) =>
      keyOf(it).toLowerCase().includes(q) || valOf(it).toLowerCase().includes(q)
    ), q);
  });

  // The add form doubles as edit (delete-then-readd on submit).
  let editing: string | null = null;
  const keyIn = TextInput(t(cfg.keyLabelKey), "", () => {}, { placeholder: t(cfg.keyPhKey) });
  const valIn = TextInput(t(cfg.valueLabelKey), "", () => {}, { placeholder: t(cfg.valuePhKey) });
  const submitBtn = Button(t("common.add"), () => void submit());
  const cancelBtn = Button(t("common.cancel"), () => resetForm());
  cancelBtn.classList.add("hidden");
  const formRow = document.createElement("div");
  formRow.className = "dic-form";
  formRow.append(keyIn.wrap, valIn.wrap, submitBtn, cancelBtn);

  const importBtn = Button(t("common.import"), () => cfg.runImport({ reload: load, showError, importBtn }));
  const exportBtn = Button(t("common.export"), () => void runExport());
  const toolRow = document.createElement("div");
  toolRow.className = "dic-tools";
  toolRow.append(importBtn, exportBtn);

  const group = SettingsGroup(
    t(cfg.titleKey),
    [toolRow, filterInput.wrap, listHost, formRow],
    t(cfg.titleTipKey),
  );
  root.append(group);

  function showError(e: unknown): void {
    toast("error", `${t("common.error_prefix")}${errText(e)}`);
  }

  async function load(): Promise<void> {
    let items: E[] = [];
    try {
      items = await invoke<E[]>(cfg.listCmd);
    } catch (e) {
      showError(e);
      return;
    }
    loaded = items;
    renderList(items);
  }

  function renderList(items: E[], filterQ = ""): void {
    listHost.replaceChildren();
    if (items.length === 0) {
      const empty = document.createElement("div");
      empty.className = "muted";
      empty.textContent = filterQ.length > 0 ? t("common.no_matches") : t(cfg.emptyKey);
      listHost.append(empty);
      return;
    }
    for (const it of items) {
      const row = document.createElement("div");
      row.className = "dic-row";
      const pair = document.createElement("button");
      pair.type = "button";
      pair.className = "dic-pair";
      pair.textContent = `${keyOf(it)} → ${valOf(it)}`;
      pair.addEventListener("click", () => beginEdit(it));
      const del = Button(t("common.delete"), () => void remove(keyOf(it), valOf(it)));
      row.append(pair, del);
      listHost.append(row);
    }
  }

  async function submit(): Promise<void> {
    const key = keyIn.get().trim();
    const value = valIn.get().trim();
    if (!key) {
      showError(new Error(t(cfg.emptyKeyErr)));
      return;
    }
    try {
      // ponytail: edit = delete-then-readd; the add invoke is identical in both
      // branches, so hoist it out instead of duplicating.
      if (editing !== null) await invoke(cfg.removeCmd, { [cfg.keyField]: editing });
      await invoke(cfg.addCmd, { [cfg.keyField]: key, [cfg.valueField]: value });
      toast("success", t(cfg.addedKey));
      resetForm();
      await load();
    } catch (e) {
      showError(e);
    }
  }

  function beginEdit(it: E): void {
    editing = keyOf(it);
    keyIn.set(keyOf(it));
    valIn.set(valOf(it));
    submitBtn.textContent = t("common.save");
    cancelBtn.classList.remove("hidden");
  }

  function resetForm(): void {
    editing = null;
    keyIn.set("");
    valIn.set("");
    submitBtn.textContent = t("common.add");
    cancelBtn.classList.add("hidden");
  }

  async function remove(key: string, value: string): Promise<void> {
    try {
      await invoke(cfg.removeCmd, { [cfg.keyField]: key });
      loaded = loaded.filter((it) => keyOf(it) !== key);
      const q = filterInput.get().trim().toLowerCase();
      renderList(loaded.filter((it) =>
        keyOf(it).toLowerCase().includes(q) || valOf(it).toLowerCase().includes(q)
      ), q);
      toast("warning", t(cfg.removedKey), {
        durationMs: 5000,
        action: {
          label: t(cfg.undoKey),
          onClick: () => { void reAdd(key, value); },
        },
      });
    } catch (e) {
      showError(e);
    }
  }

  async function reAdd(key: string, value: string): Promise<void> {
    try {
      await invoke(cfg.addCmd, { [cfg.keyField]: key, [cfg.valueField]: value });
      await load();
    } catch (e) {
      showError(e);
    }
  }

  async function runExport(): Promise<void> {
    try {
      await invoke(cfg.exportCmd);
      toast("success", t(cfg.exportedKey));
    } catch (e) {
      showError(e);
    }
  }

  void load();
  return { el: root };
};
