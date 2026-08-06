import { invoke } from "@tauri-apps/api/core";

import { Button, SettingsGroup, TextInput, toast } from "../ui";
import { errText } from "../persist";
import type { SectionBuilder, SnippetEntry } from "../types";
import { t } from "../../i18n";

// Voice-cue → stored-block expansion. Mirrors dictionary.ts in shape (list +
// add/edit form + live filter + undo-delete + import/export), with two
// deliberate differences:
//   • cue/expansion vocabulary (the whole dictation must equal the cue exactly);
//   • atomic import (pick → apply), no preview panel — snippets are small-scale
//     (signatures/addresses); the dictionary's 2-IPC preview split is
//     dictionary-scale polish. YAGNI here.
// Shares the .dic-* CSS classes — they are generic list/form layout, not
// dictionary-specific.

export const buildSnippets: SectionBuilder = () => {
  const root = document.createElement("div");
  const listHost = document.createElement("div");
  listHost.className = "dic-list";

  let loaded: SnippetEntry[] = []; // cached for live filter + undo
  const filterInput = TextInput(
    t("snippets.filter"), "", () => {},
    { placeholder: t("snippets.filter_ph") },
  );
  filterInput.wrap.querySelector("input")!.addEventListener("input", (e) => {
    const q = (e.target as HTMLInputElement).value.trim().toLowerCase();
    renderList(loaded.filter((it) =>
      it.cue.toLowerCase().includes(q) || it.expansion.toLowerCase().includes(q)
    ), q);
  });

  // The add form doubles as edit (delete-then-readd on submit).
  let editing: string | null = null;
  const cueIn = TextInput(t("snippets.cue"), "", () => {}, { placeholder: t("snippets.cue_ph") });
  const expIn = TextInput(t("snippets.expansion"), "", () => {}, { placeholder: t("snippets.expansion_ph") });
  const submitBtn = Button(t("common.add"), () => void submit());
  const cancelBtn = Button(t("common.cancel"), () => resetForm());
  cancelBtn.classList.add("hidden");
  const formRow = document.createElement("div");
  formRow.className = "dic-form";
  formRow.append(cueIn.wrap, expIn.wrap, submitBtn, cancelBtn);

  const importBtn = Button(t("common.import"), () => void runImport());
  const exportBtn = Button(t("common.export"), () => void runExport());
  const toolRow = document.createElement("div");
  toolRow.className = "dic-tools";
  toolRow.append(importBtn, exportBtn);

  const group = SettingsGroup(
    t("snippets.title"),
    [toolRow, filterInput.wrap, listHost, formRow],
    t("snippets.title_tip"),
  );
  root.append(group);

  function showError(e: unknown): void {
    toast("error", `${t("common.error_prefix")}${errText(e)}`);
  }

  async function load(): Promise<void> {
    let items: SnippetEntry[] = [];
    try {
      items = await invoke<SnippetEntry[]>("snippet_list");
    } catch (e) {
      showError(e);
      return;
    }
    loaded = items;
    renderList(items);
  }

  function renderList(items: SnippetEntry[], filterQ = ""): void {
    listHost.replaceChildren();
    if (items.length === 0) {
      const empty = document.createElement("div");
      empty.className = "muted";
      empty.textContent = filterQ.length > 0 ? t("common.no_matches") : t("common.empty_snip");
      listHost.append(empty);
      return;
    }
    for (const it of items) {
      const row = document.createElement("div");
      row.className = "dic-row";
      const pair = document.createElement("button");
      pair.type = "button";
      pair.className = "dic-pair";
      pair.textContent = `${it.cue} → ${it.expansion}`;
      pair.addEventListener("click", () => beginEdit(it));
      const del = Button(t("common.delete"), () => void remove(it.cue, it.expansion));
      row.append(pair, del);
      listHost.append(row);
    }
  }

  async function submit(): Promise<void> {
    const cue = cueIn.get().trim();
    const expansion = expIn.get().trim();
    if (!cue) {
      showError(new Error(t("snippets.empty_cue")));
      return;
    }
    try {
      // ponytail: edit = delete-then-readd; the add invoke is identical in both
      // branches, so hoist it out instead of duplicating.
      if (editing !== null) await invoke("snippet_remove", { cue: editing });
      await invoke("snippet_add", { cue, expansion });
      toast("success", t("snippets.added"));
      resetForm();
      await load();
    } catch (e) {
      showError(e);
    }
  }

  function beginEdit(it: SnippetEntry): void {
    editing = it.cue;
    cueIn.set(it.cue);
    expIn.set(it.expansion);
    submitBtn.textContent = t("common.save");
    cancelBtn.classList.remove("hidden");
  }

  function resetForm(): void {
    editing = null;
    cueIn.set("");
    expIn.set("");
    submitBtn.textContent = t("common.add");
    cancelBtn.classList.add("hidden");
  }

  async function remove(cue: string, expansion: string): Promise<void> {
    try {
      await invoke("snippet_remove", { cue });
      loaded = loaded.filter((it) => it.cue !== cue);
      const q = filterInput.get().trim().toLowerCase();
      renderList(loaded.filter((it) =>
        it.cue.toLowerCase().includes(q) || it.expansion.toLowerCase().includes(q)
      ), q);
      toast("warning", t("snippets.removed"), {
        durationMs: 5000,
        action: {
          label: t("snippets.undo"),
          onClick: () => { void reAdd(cue, expansion); },
        },
      });
    } catch (e) {
      showError(e);
    }
  }

  async function reAdd(cue: string, expansion: string): Promise<void> {
    try {
      await invoke("snippet_add", { cue, expansion });
      await load();
    } catch (e) {
      showError(e);
    }
  }

  // Atomic import: pick → apply → toast. No preview (small-scale feature).
  async function runImport(): Promise<void> {
    try {
      await invoke("snippet_import");
      toast("success", t("snippets.imported"));
      await load();
    } catch (e) {
      showError(e);
    }
  }

  async function runExport(): Promise<void> {
    try {
      await invoke("snippet_export");
      toast("success", t("snippets.exported"));
    } catch (e) {
      showError(e);
    }
  }

  void load();
  return { el: root };
};
