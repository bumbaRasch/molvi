import { invoke } from "@tauri-apps/api/core";

import { Button, Select, SettingsGroup, TextInput, Toggle, toast } from "../ui";
import type { SelectCtrl } from "../ui";
import { errText, flushPending, patcher } from "../persist";
import type { HistoryRow, SectionBuilder } from "../types";
import { getCurrentLang, t } from "../../i18n";

const PAGE_SIZE = 12;

// H5: inline two-step confirm — no Dialog helper, no window.confirm dependency.
function twoStepConfirm(
  label: string,
  onConfirm: () => Promise<void>,
): HTMLElement {
  const container = document.createElement("div");
  container.className = "confirm-wrap";
  const showButton = (): void => {
    container.replaceChildren(
      Button(
        label,
        () => {
          const warning = document.createElement("span");
          warning.className = "confirm-warning";
          warning.textContent = t("common.irreversible");
          const confirmBtn = Button(
            t("common.confirm"),
            () => {
              // Finding 2: block double-fire while the async action is in flight.
              confirmBtn.disabled = true;
              void onConfirm().then(showButton);
            },
            { variant: "destructive" },
          );
          const cancelBtn = Button(t("common.cancel"), showButton);
          container.replaceChildren(warning, confirmBtn, cancelBtn);
          confirmBtn.focus();
        },
        { variant: "destructive" },
      ),
    );
  };
  showButton();
  return container;
}

// Consent-first History section (spec §7.1). When enabled === false, ONLY the
// opt-in toggle + Privacy Promise render (blocks 2–5 absent from the DOM).
export const buildHistory: SectionBuilder = (store) => {
  const root = document.createElement("div");
  const patch = patcher(store);
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  function showActionError(e: unknown): void {
    toast("error", `${t("common.error_prefix")}${errText(e)}`);
  }

  const settings = store.get().settings!;

  // H1 block 1: opt-in (always visible). The Privacy Promise lives only as the
  // History title ⓘ now (no separate visible block) — reuses common.privacy_promise.
  const toggle = Toggle(
    t("history.toggle"),
    settings.history.enabled,
    (v) => {
      patch((s) => { s.history.enabled = v; });
      // Enable must await the save (Rust opens the History store on set_settings)
      // before the first query — else the store is still None → stale []. Disable
      // needs no query, stays on the debounce.
      if (v) void enableAndShow();
      else syncEnabled(false);
    },
    t("history.enabled_notice"),
  );
  const optIn = SettingsGroup(t("history.title"), [toggle.wrap], t("common.privacy_promise"));

  const enabledContainer = document.createElement("div");
  root.append(optIn, enabledContainer);

  // Finding 1: flushPending cancels the 300ms debounce and runs the save once,
  // so the query that buildEnabledContent fires sees the opened History store.
  async function enableAndShow(): Promise<void> {
    await flushPending();
    syncEnabled(true);
  }

  function syncEnabled(enabled: boolean): void {
    if (searchTimer) { clearTimeout(searchTimer); searchTimer = null; }
    if (enabled && !enabledContainer.hasChildNodes()) {
      enabledContainer.append(buildEnabledContent());
    } else if (!enabled) {
      enabledContainer.replaceChildren();
    }
  }

  syncEnabled(settings.history.enabled);

  // H1 blocks 2–5 (only built when enabled). Inner closures capture list state.
  function buildEnabledContent(): HTMLElement {
    const wrap = document.createElement("div");

    // H1 block 2: retention (debounced save via patcher).
    const cur = store.get().settings!;
    const maxEntries = TextInput(
      t("history.max_entries"), String(cur.history.max_entries), (v) => {
        const n = parseInt(v, 10);
        if (Number.isFinite(n) && n >= 0) patch((s) => { s.history.max_entries = n; });
      }, { type: "number" },
    );
    const maxAge = TextInput(
      t("history.max_age"), String(cur.history.max_age_days), (v) => {
        const n = parseInt(v, 10);
        if (Number.isFinite(n) && n >= 0) patch((s) => { s.history.max_age_days = n; });
      }, { type: "number" },
    );
    // Two retention fields share one row equally (entries-count + days).
    const retentionRow = document.createElement("div");
    retentionRow.className = "field-pair";
    retentionRow.append(maxEntries.wrap, maxAge.wrap);
    wrap.append(SettingsGroup(t("history.retention_title"), [retentionRow]));

    // H1 block 3 + H7: search (debounced ~250ms; reset offset on change).
    let search: string | null = null;
    let offset = 0;
    // Filter state: null = no filter.
    let langFilter: string | null = null;   // e.g. "ru"; null = All languages
    let sinceFilter: number | null = null;  // ms cutoff; null = All time
    let langSelCtrl: SelectCtrl | null = null; // lang Select appears async; ref for resetFilters
    const listHost = document.createElement("div");
    listHost.className = "hist-list";
    const moreBtn = Button(t("history.load_more"), () => { offset += PAGE_SIZE; void query(true); });
    moreBtn.classList.add("hidden");

    // Task 12.4: bulk-select state + toolbar. APG roving-tabindex composite
    // (NOT listbox — rows hold a checkbox + buttons, which listbox excludes).
    const selectedIds: Set<number> = new Set();
    let lastClickedId: number | null = null;
    const bulkBar = document.createElement("div");
    bulkBar.className = "bulk-bar hidden";
    const bulkLabel = document.createElement("span");
    bulkLabel.className = "bulk-label";
    const bulkDeleteBtn = twoStepConfirm(t("history.bulk_delete"), () => doBulkDelete());
    const bulkClearBtn = Button(t("history.bulk_clear"), () => clearSelection());
    bulkBar.append(bulkLabel, bulkDeleteBtn, bulkClearBtn);
    function refreshBulkBar(): void {
      const n = selectedIds.size;
      bulkBar.classList.toggle("hidden", n === 0);
      bulkLabel.textContent = n > 0 ? t("history.bulk_selected").replace("{n}", String(n)) : "";
    }
    function clearSelection(): void {
      selectedIds.clear();
      for (const cb of listHost.querySelectorAll<HTMLInputElement>(".hist-select")) cb.checked = false;
      refreshBulkBar();
    }
    function currentLoadedIds(): number[] {
      const out: number[] = [];
      for (const row of listHost.querySelectorAll<HTMLElement>(".hist-row")) {
        const id = Number(row.dataset.rowId);
        if (Number.isFinite(id)) out.push(id);
      }
      return out;
    }
    function syncCheckboxes(): void {
      for (const row of listHost.querySelectorAll<HTMLElement>(".hist-row")) {
        const id = Number(row.dataset.rowId);
        const cb = row.querySelector<HTMLInputElement>(".hist-select");
        if (cb) cb.checked = selectedIds.has(id);
      }
    }

    const searchInput = TextInput(t("history.search"), "", (v) => {
      if (searchTimer) clearTimeout(searchTimer);
      searchTimer = setTimeout(() => {
        const t = v.trim();
        search = t.length > 0 ? t : null;
        offset = 0;
        void query(false);
      }, 250);
    }, { placeholder: t("history.search_ph") });

    // Filter row: search input (flex:1) + date Select + (async) lang Select.
    // Replaces the Task-12.3 chip bars — Selects scale to many languages
    // without wrapping, and single-choice filters are the Select's canonical use.
    const DAY_MS = 86_400_000;
    const dateSel = Select(
      t("history.filter_date"),
      [
        { value: "today", label: t("history.date_today") },
        { value: "7d", label: t("history.date_7d") },
        { value: "30d", label: t("history.date_30d") },
        { value: "all", label: t("history.date_all") },
      ],
      "all",
      (v) => {
        sinceFilter = v === "today" ? Date.now() - DAY_MS
          : v === "7d" ? Date.now() - 7 * DAY_MS
          : v === "30d" ? Date.now() - 30 * DAY_MS
          : null;
        offset = 0;
        void query(false);
      },
    );

    const filterRow = document.createElement("div");
    filterRow.className = "hist-filters";
    filterRow.append(searchInput.wrap, dateSel.wrap);

    // Lang Select appears only when history holds >1 language (queried once;
    // fail-open on error). Inserted before the date Select so the row reads
    // input → lang → date.
    void (async (): Promise<void> => {
      try {
        const langs = await invoke<string[]>("history_distinct_langs");
        if (langs.length <= 1) return;
        const langSel = Select(
          t("history.filter_lang"),
          [{ value: "all", label: t("history.lang_all") }].concat(
            langs.map((l) => ({ value: l, label: l })),
          ),
          "all",
          (v) => {
            langFilter = v === "all" ? null : v;
            offset = 0;
            void query(false);
          },
        );
        langSelCtrl = langSel;
        filterRow.insertBefore(langSel.wrap, dateSel.wrap);
      } catch (e) {
        console.error("history_distinct_langs failed", e); // metadata-only
      }
    })();

    // H1 block 4: list (H4 foreground hint + rows + more).
    wrap.append(
      SettingsGroup(t("history.search"), [filterRow]),
      SettingsGroup(t("history.entries_title"), [bulkBar, listHost, moreBtn], t("history.paste_hint")),
    );

    // H1 block 5 + H2: danger. Clear empties the table; Erase DROPs it.
    const dangerHint = document.createElement("p");
    dangerHint.className = "muted";
    dangerHint.textContent = t("history.danger_hint");
    const dangerRow = document.createElement("div");
    dangerRow.className = "hist-danger-row";
    dangerRow.append(
      twoStepConfirm(t("history.clear_all"), doClear),
      twoStepConfirm(t("history.disable_and_erase"), doDisableAndErase),
    );
    wrap.append(SettingsGroup(t("history.danger_title"), [dangerHint, dangerRow]));

    // Task 12.4: keyboard nav (roving tabindex + j/k/arrows/Home/End + Delete
    // + Space). Acts only when the focused element is inside a .hist-row.
    listHost.addEventListener("keydown", (e) => {
      const row = (document.activeElement as HTMLElement | null)?.closest(".hist-row") as HTMLElement | null;
      if (!row || !listHost.contains(row)) return;
      const rows = Array.from(listHost.querySelectorAll<HTMLElement>(".hist-row"));
      const idx = rows.indexOf(row);
      if (idx === -1) return;

      const move = (i: number): void => {
        const clamped = Math.max(0, Math.min(rows.length - 1, i));
        row.tabIndex = -1;
        rows[clamped].tabIndex = 0;
        rows[clamped].focus();
      };

      switch (e.key) {
        case "ArrowDown":
        case "j":
          e.preventDefault();
          move(idx + 1);
          break;
        case "ArrowUp":
        case "k":
          e.preventDefault();
          move(idx - 1);
          break;
        case "Home":
          e.preventDefault();
          move(0);
          break;
        case "End":
          e.preventDefault();
          move(rows.length - 1);
          break;
        case "Enter":
          e.preventDefault();
          (row.querySelector(".hist-main") as HTMLElement | null)?.click();
          break;
        case "Delete": {
          e.preventDefault();
          const id = Number(row.dataset.rowId);
          void doDelete(id, row);
          break;
        }
        case " ": {
          // PRE-CORRECTION D: cb.click() ONLY — native click toggles checked;
          // a manual pre-flip would net to a no-op.
          if (document.activeElement === row) {
            e.preventDefault();
            const cb = row.querySelector<HTMLInputElement>(".hist-select");
            if (cb) cb.click();
          }
          break;
        }
      }
    });

    void query(false);
    return wrap;

    // ── inner closures (capture search / offset / listHost / moreBtn) ──

    // Race guard: rapid filter/offset changes can resolve out-of-order (a slow
    // prior query landing after a newer one). `querySeq` stamps each query; a
    // resolve whose stamp is stale is dropped before rendering. Same pattern as
    // federated-search.ts. Local SQLite is sub-ms so this rarely fires, but it
    // guarantees the list never briefly shows superseded rows.
    let querySeq = 0;

    async function query(append: boolean): Promise<void> {
      const mine = ++querySeq;
      let rows: HistoryRow[] = [];
      try {
        rows = await invoke<HistoryRow[]>("history_query", {
          search, lang: langFilter, since: sinceFilter, limit: PAGE_SIZE, offset,
        });
      } catch (e) {
        console.error("history_query failed", e); // metadata-only: error object
        showActionError(e);
        return;
      }
      if (mine !== querySeq) return; // a newer query superseded this one
      if (!append) listHost.replaceChildren();
      renderRows(rows);
      // ponytail: «more» visibility is a "last page was a full PAGE_SIZE" heuristic —
      // may show on an exactly-full last page (click returns [] and hides it); self-corrects.
      moreBtn.classList.toggle("hidden", rows.length < PAGE_SIZE);
    }

    function renderRows(rows: HistoryRow[]): void {
      if (rows.length === 0 && listHost.children.length === 0) {
        const empty = document.createElement("div");
        empty.className = "muted";
        const hasFilter = search !== null || langFilter !== null || sinceFilter !== null;
        empty.textContent = hasFilter ? t("common.no_matches") : t("history.empty");
        listHost.append(empty);
        return;
      }
      for (const r of rows) listHost.append(renderRow(r));
      // Task 12.4: roving-tabindex seed — exactly one row is tab-0 (the rest -1).
      const first = listHost.querySelector<HTMLElement>(".hist-row");
      if (first) first.tabIndex = 0;
    }

    function renderRow(r: HistoryRow): HTMLElement {
      const row = document.createElement("div");
      row.className = "hist-row";
      row.tabIndex = -1; // roving tabindex: 0 on the focused row, -1 on others (Task 12.4 swaps)
      row.dataset.rowId = String(r.id);
      row.setAttribute("role", "group");
      // Task 12.4: per-row checkbox. ONE click listener — stopPropagation FIRST
      // (stops the .hist-main expand), then range/add/delete logic. Reading
      // checkbox.checked here is correct: the native click toggled it already.
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.className = "hist-select";
      checkbox.setAttribute("aria-label", t("history.select_row"));
      checkbox.checked = selectedIds.has(r.id);
      checkbox.addEventListener("click", (e) => {
        e.stopPropagation();
        if (e.shiftKey && lastClickedId !== null) {
          const ids = currentLoadedIds();
          const i = ids.indexOf(r.id);
          const j = ids.indexOf(lastClickedId);
          if (i !== -1 && j !== -1) {
            const [lo, hi] = i < j ? [i, j] : [j, i];
            for (let k = lo; k <= hi; k++) selectedIds.add(ids[k]);
          }
        } else {
          if (checkbox.checked) selectedIds.add(r.id);
          else selectedIds.delete(r.id);
        }
        lastClickedId = r.id;
        syncCheckboxes();
        refreshBulkBar();
      });
      const metaParts = [
        new Date(r.created_at).toLocaleString(getCurrentLang()),
        r.lang ?? "",
        r.post_mode ?? "",
      ].filter((s) => s.length > 0);

      // .hist-main is a <div>, NOT a <button> — the row itself is the focusable
      // element (Task 12.4 handles Enter/Space). Click on .hist-main = expand.
      const main = document.createElement("div");
      main.className = "hist-main";
      main.setAttribute("aria-expanded", "false");

      const meta = document.createElement("div");
      meta.className = "hist-meta";
      meta.textContent = metaParts.join(" · ");

      const text = document.createElement("div");
      text.className = "hist-text";
      const COLLAPSED = 80;
      let expanded = false;
      text.textContent = r.text.slice(0, COLLAPSED);

      main.append(meta, text);
      main.addEventListener("click", () => toggleExpand());

      function toggleExpand(): void {
        expanded = !expanded;
        text.textContent = expanded ? r.text : r.text.slice(0, COLLAPSED);
        main.setAttribute("aria-expanded", expanded ? "true" : "false");
        row.classList.toggle("expanded", expanded);
      }

      const actions = document.createElement("div");
      actions.className = "hist-actions";
      actions.append(
        Button(t("history.repaste"), () => void doRepaste(r.id)),
        Button(t("common.delete"), () => void doDelete(r.id, row)),
      );
      row.append(checkbox, main, actions);
      return row;
    }

    async function doDelete(id: number, row: HTMLElement): Promise<void> {
      try {
        await invoke("history_delete", { id });
        const rows = Array.from(listHost.querySelectorAll<HTMLElement>(".hist-row"));
        const idx = rows.indexOf(row);
        selectedIds.delete(id);
        row.remove();
        if (listHost.children.length === 0) {
          if (offset > 0) { offset = 0; await query(false); }
          else renderRows([]);
        } else {
          // APG focus persistence: move focus + the tab-0 slot to the next
          // surviving row (else the previous one), so the keyboard user keeps
          // their place in the list after a single-row delete.
          const remaining = Array.from(listHost.querySelectorAll<HTMLElement>(".hist-row"));
          const target = remaining[idx] ?? remaining[idx - 1] ?? null;
          if (target) {
            for (const r of remaining) r.tabIndex = -1;
            target.tabIndex = 0;
            target.focus();
          }
        }
        refreshBulkBar();
      } catch (e) {
        console.error("history_delete failed", e);
        showActionError(e);
      }
    }

    // Task 12.4: bulk delete via the 12.1 IPC. Re-queries from offset 0 so the
    // list (and the roving-tabindex seed) reflects the post-delete table.
    async function doBulkDelete(): Promise<void> {
      const ids = Array.from(selectedIds);
      if (ids.length === 0) return;
      try {
        await invoke("history_bulk_delete", { ids });
        selectedIds.clear();
        offset = 0;
        await query(false);
        refreshBulkBar();
        listHost.querySelector<HTMLElement>(".hist-row")?.focus();
      } catch (e) {
        console.error("history_bulk_delete failed", e);
        showActionError(e);
      }
    }

    async function doRepaste(id: number): Promise<void> {
      try {
        await invoke("re_paste", { id });
        toast("success", t("history.pasted"));
      } catch (e) {
        console.error("re_paste failed", e);
        showActionError(e);
      }
    }

    async function doClear(): Promise<void> {
      try {
        await invoke("history_clear");
        // The DB is empty now — reset filters so the empty state reads
        // "history empty" (history.empty), not "no matches" (common.no_matches).
        search = null;
        langFilter = null;
        sinceFilter = null;
        searchInput.set("");
        dateSel.set("all");
        if (langSelCtrl) langSelCtrl.set("all");
        offset = 0;
        listHost.replaceChildren();
        renderRows([]);
        moreBtn.classList.add("hidden");
      } catch (e) {
        console.error("history_clear failed", e);
        showActionError(e);
      }
    }

    async function doDisableAndErase(): Promise<void> {
      try {
        await invoke("history_disable_and_erase");
        // Rust already persisted enabled=false + saved; sync the in-memory store.
        const next = structuredClone(store.get().settings!);
        next.history.enabled = false;
        store.set({ settings: next });
        toggle.set(false);
        syncEnabled(false);
      } catch (e) {
        console.error("history_disable_and_erase failed", e);
        showActionError(e);
      }
    }
  }

  return {
    el: root,
    cleanup: () => { if (searchTimer) clearTimeout(searchTimer); },
  };
};
