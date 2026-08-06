import { invoke } from "@tauri-apps/api/core";

import { Button, SettingsGroup, TextInput, toast } from "../ui";
import { errText } from "../persist";
import type { DictEntry, ImportPreview, SectionBuilder } from "../types";
import { t } from "../../i18n";

export const buildDictionary: SectionBuilder = () => {
  const root = document.createElement("div");
  const listHost = document.createElement("div");
  listHost.className = "dic-list";

  let loaded: DictEntry[] = []; // cached for live filter + undo
  const filterInput = TextInput(
    t("dictionary.filter"), "", () => {},
    { placeholder: t("dictionary.filter_ph") },
  );
  filterInput.wrap.querySelector("input")!.addEventListener("input", (e) => {
    const q = (e.target as HTMLInputElement).value.trim().toLowerCase();
    renderList(loaded.filter((it) =>
      it.entry.toLowerCase().includes(q) || it.replacement.toLowerCase().includes(q)
    ), q);
  });

  // R10: the add form doubles as edit (delete-then-readd on submit).
  let editing: string | null = null;
  const entryIn = TextInput(t("dictionary.entry"), "", () => {}, { placeholder: t("dictionary.entry_ph") });
  const replIn = TextInput(t("dictionary.replacement"), "", () => {}, { placeholder: t("dictionary.replacement_ph") });
  const submitBtn = Button(t("common.add"), () => void submit());
  const cancelBtn = Button(t("common.cancel"), () => resetForm());
  cancelBtn.classList.add("hidden");
  const formRow = document.createElement("div");
  formRow.className = "dic-form";
  formRow.append(entryIn.wrap, replIn.wrap, submitBtn, cancelBtn);

  const importBtn = Button(t("common.import"), () => void runImport());
  const exportBtn = Button(t("common.export"), () => void runExport());
  const toolRow = document.createElement("div");
  toolRow.className = "dic-tools";
  toolRow.append(importBtn, exportBtn);

  const group = SettingsGroup(
    t("dictionary.title"),
    [toolRow, filterInput.wrap, listHost, formRow],
    t("dictionary.title_tip"),
  );
  root.append(group);

  function showError(e: unknown): void {
    toast("error", `${t("common.error_prefix")}${errText(e)}`);
  }

  async function load(): Promise<void> {
    let items: DictEntry[] = [];
    try {
      items = await invoke<DictEntry[]>("dictionary_list");
    } catch (e) {
      showError(e);
      return;
    }
    loaded = items;
    renderList(items);
  }

  function renderList(items: DictEntry[], filterQ = ""): void {
    listHost.replaceChildren();
    if (items.length === 0) {
      const empty = document.createElement("div");
      empty.className = "muted";
      empty.textContent = filterQ.length > 0 ? t("common.no_matches") : t("common.empty_dict");
      listHost.append(empty);
      return;
    }
    for (const it of items) {
      const row = document.createElement("div");
      row.className = "dic-row";
      const pair = document.createElement("button");
      pair.type = "button";
      pair.className = "dic-pair";
      pair.textContent = `${it.entry} → ${it.replacement}`;
      pair.addEventListener("click", () => beginEdit(it));
      const del = Button(t("common.delete"), () => void remove(it.entry, it.replacement));
      row.append(pair, del);
      listHost.append(row);
    }
  }

  async function submit(): Promise<void> {
    const entry = entryIn.get().trim();
    const replacement = replIn.get().trim();
    if (!entry) {
      showError(new Error(t("dictionary.empty_entry")));
      return;
    }
    try {
      // ponytail: edit = delete-then-readd; the add invoke is identical in both
      // branches, so hoist it out instead of duplicating.
      if (editing !== null) await invoke("dictionary_remove", { entry: editing });
      await invoke("dictionary_add", { entry, replacement });
      toast("success", t("dictionary.added"));
      resetForm();
      await load();
    } catch (e) {
      showError(e);
    }
  }

  function beginEdit(it: DictEntry): void {
    editing = it.entry;
    entryIn.set(it.entry);
    replIn.set(it.replacement);
    submitBtn.textContent = t("common.save");
    cancelBtn.classList.remove("hidden");
  }

  function resetForm(): void {
    editing = null;
    entryIn.set("");
    replIn.set("");
    submitBtn.textContent = t("common.add");
    cancelBtn.classList.add("hidden");
  }

  async function remove(entry: string, replacement: string): Promise<void> {
    try {
      await invoke("dictionary_remove", { entry });
      loaded = loaded.filter((it) => it.entry !== entry);
      const q = filterInput.get().trim().toLowerCase();
      renderList(loaded.filter((it) =>
        it.entry.toLowerCase().includes(q) || it.replacement.toLowerCase().includes(q)
      ), q);
      toast("warning", t("dictionary.removed"), {
        durationMs: 5000,
        action: {
          label: t("dictionary.undo"),
          onClick: () => { void reAdd(entry, replacement); },
        },
      });
    } catch (e) {
      showError(e);
    }
  }

  async function reAdd(entry: string, replacement: string): Promise<void> {
    try {
      await invoke("dictionary_add", { entry, replacement });
      await load();
    } catch (e) {
      showError(e);
    }
  }

  async function runImport(): Promise<void> {
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
  }

  async function apply(path: string, panel: HTMLElement): Promise<void> {
    try {
      await invoke("dictionary_import_apply", { path });
      toast("success", t("dictionary.imported"));
      restoreImportButton(panel);
      await load();
    } catch (e) {
      showError(e);
      restoreImportButton(panel);
    }
  }

  function restoreImportButton(panel: HTMLElement): void {
    panel.replaceWith(importBtn);
  }

  async function runExport(): Promise<void> {
    try {
      await invoke("dictionary_export");
      toast("success", t("dictionary.exported"));
    } catch (e) {
      showError(e);
    }
  }

  void load();
  return { el: root };
};
