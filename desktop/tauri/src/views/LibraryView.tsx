import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { FolderOpen, Power, ScrollText, Trash2 } from "lucide-react";
import { api, type LibraryMod, type Profile } from "@/lib/api";
import { LibraryRow } from "@/components/LibraryRow";
import { ModDetail } from "@/components/ModDetail";
import { ContextMenu, type ContextMenuItem } from "@/components/ui/context-menu";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Select } from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { useConfirm } from "@/components/useConfirm";
import { useT } from "@/lib/i18n";

interface LibraryViewProps {
  onGoToBuilds: () => void;
  onGoToConflicts: () => void;
  addLog: (message: string, tag?: string) => void;
  refreshTick: number;
  profiles: Profile[];
  activeProfile: string;
  setActiveProfile: (id: string) => void;
  /** Live per-mod progress while a bulk action runs. */
  bulkProgress?: { action: string; current: number; total: number; mod: string } | null;
}

// Friendly labels for raw install-method enum names.
const METHOD_LABELS: Record<string, string> = {
  TSLPATCHER: "TSLPatcher",
  HOLOPATCHER: "HoloPatcher",
  DIRECT_COPY: "Direct copy",
  OVERRIDE_COPY: "Override copy",
  TLK_REPLACE: "Dialog (TLK)",
  MULTI_VARIANT: "Multi-variant",
  MULTIPLE: "Multiple",
  GAME_PATCHER: "Game patcher",
  MANUAL: "Manual",
};

const methodLabel = (m: string) => METHOD_LABELS[m] ?? m;

/** Normalize a mod name for duplicate detection. */
const dupeKey = (m: LibraryMod) => m.name.trim().toLowerCase();

export function LibraryView({
  onGoToBuilds, onGoToConflicts, addLog, refreshTick,
  profiles, activeProfile, setActiveProfile, bulkProgress,
}: LibraryViewProps) {
  const t = useT();
  const [confirm, confirmDialog] = useConfirm();
  const [mods, setMods] = useState<LibraryMod[]>([]);
  const [mergeable, setMergeable] = useState(0);
  const [query, setQuery] = useState("");
  const [enabledOnly, setEnabledOnly] = useState(false);
  const [dupesOnly, setDupesOnly] = useState(false);
  const [method, setMethod] = useState("all");
  const [category, setCategory] = useState("all");
  const [loading, setLoading] = useState(true);
  const [errored, setErrored] = useState(false);
  const [openMod, setOpenMod] = useState<LibraryMod | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; mod: LibraryMod } | null>(null);
  const [bulkBusy, setBulkBusy] = useState(false);
  const [confirmBulk, setConfirmBulk] = useState(false);
  // Progress for the running bulk action. The Activity log lives on another
  // tab, so without this the window just freezes with no sign of life.
  const [bulkStatus, setBulkStatus] = useState<string | null>(null);
  const [hasBaseline, setHasBaseline] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const loadedProfile = useRef("");

  const load = useCallback(async () => {
    if (!activeProfile) { setMods([]); setLoading(false); return; }
    setLoading(loadedProfile.current !== activeProfile);
    setErrored(false);
    try {
      const r = await api.library(activeProfile);
      setMods(r.mods ?? []);
      setMergeable(r.mergeable_duplicates ?? 0);
      loadedProfile.current = activeProfile;
    } catch {
      setErrored(true);
      setMods([]);
      setMergeable(0);
    } finally {
      setLoading(false);
    }
  }, [activeProfile]);

  useEffect(() => { load(); }, [load, refreshTick]);

  useEffect(() => {
    if (!activeProfile) return;
    api.baselineStatus(activeProfile)
      .then(r => setHasBaseline(r.has_baseline))
      .catch(() => setHasBaseline(false));
  }, [activeProfile, refreshTick]);

  const runDedupe = useCallback(async () => {
    if (!activeProfile || bulkBusy) return;
    setBulkBusy(true);
    try {
      const r = await api.dedupeLibrary(activeProfile);
      addLog(r.removed
        ? t("library.dedupeDone", { count: r.removed })
        : t("library.dedupeNone"), r.removed ? "success" : "warning");
      if (r.removed && r.remaining) {
        addLog(t("library.dedupeRemaining", { count: r.remaining }), "warning");
      }
      setDupesOnly(false);
      await load();
    } catch (e: any) {
      addLog(t("library.dedupeFailed", { error: e?.message ?? "error" }), "error");
    } finally {
      setBulkBusy(false);
    }
  }, [activeProfile, bulkBusy, addLog, load, t]);

  const runReset = useCallback(async () => {
    if (!activeProfile || bulkBusy) return;
    setBulkBusy(true);
    setBulkStatus("Restoring game backup…");
    try {
      const r = await api.baselineReset(activeProfile);
      addLog(
        `Game reset. Removed ${r.override_removed} Override file(s) and `
        + `${r.modules_removed} module(s); restored ${r.restored.join(", ") || "nothing"}.`,
        "success");
      await load();
    } catch (e: any) {
      addLog(`Could not reset the game: ${e?.data?.message ?? e?.message ?? "error"}`,
        "error");
    } finally {
      setBulkBusy(false);
      setConfirmReset(false);
      setBulkStatus(null);
    }
  }, [activeProfile, bulkBusy, addLog, load]);

  // Filters change what "shown" means, so an in-flight confirmation would no
  // longer describe what the button is about to remove. Reset it.
  useEffect(() => { setConfirmBulk(false); },
    [query, enabledOnly, dupesOnly, method, category, activeProfile]);


  // Distinct methods/categories present, for the filter dropdowns.
  const methods = useMemo(
    () => Array.from(new Set(mods.map((m) => m.install_method).filter(Boolean))).sort(),
    [mods],
  );
  const categories = useMemo(
    () => Array.from(new Set(mods.map((m) => m.category).filter(Boolean))).sort(),
    [mods],
  );

  // Names that occur more than once → duplicate set.
  const dupeKeys = useMemo(() => {
    const counts = new Map<string, number>();
    for (const m of mods) counts.set(dupeKey(m), (counts.get(dupeKey(m)) ?? 0) + 1);
    return new Set([...counts].filter(([, n]) => n > 1).map(([k]) => k));
  }, [mods]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return mods
      .filter((m) => (enabledOnly ? m.enabled : true))
      .filter((m) => (method === "all" ? true : m.install_method === method))
      .filter((m) => (category === "all" ? true : m.category === category))
      .filter((m) => (dupesOnly ? dupeKeys.has(dupeKey(m)) : true))
      .filter((m) => (q ? m.name.toLowerCase().includes(q) : true))
      .sort((a, b) => a.load_order - b.load_order);
  }, [mods, query, enabledOnly, method, category, dupesOnly, dupeKeys]);

  // Patcher mods rewrite shared game files, so there is no per-mod undo for
  // them. Splitting the counts up front means the bar can offer only actions
  // that can succeed, instead of starting a removal that fails 154 times.
  const removable = useMemo(() => filtered.filter(m => m.toggleable), [filtered]);
  const patcherOnly = useMemo(() => filtered.filter(m => !m.toggleable), [filtered]);

  const runBulkToggle = useCallback(async (action: "enable" | "disable") => {
    if (!activeProfile || bulkBusy) return;
    setBulkBusy(true);
    try {
      setBulkStatus(`${action === "enable" ? "Enabling" : "Disabling"} ${filtered.length} mod(s)…`);
      const r = await api.bulkToggle(activeProfile, filtered.map(m => m.id), action);
      addLog(`${action === "enable" ? "Enabled" : "Disabled"} ${r.changed.length} mod(s).`,
        "success");
      r.failed.forEach(f => addLog(`${f.mod}: ${f.reason}`, "warning"));
      await load();
    } catch (e: any) {
      addLog(`Bulk ${action} failed: ${e?.message ?? "error"}`, "error");
    } finally {
      setBulkBusy(false);
      setBulkStatus(null);
    }
  }, [activeProfile, bulkBusy, filtered, addLog, load]);

  const runBulkUninstall = useCallback(async (force: boolean) => {
    if (!activeProfile || bulkBusy) return;
    // Without force only the cleanly-removable ones are even attempted.
    const targets = force ? filtered : removable;
    if (targets.length === 0) return;
    setBulkBusy(true);
    try {
      setBulkStatus(`Removing ${targets.length} mod(s)… this can take a while.`);
      const r = await api.bulkUninstall(activeProfile, targets.map(m => m.id), force);
      addLog(
        force
          ? `Removed ${r.removed.length} of ${r.requested} mod(s) from the library. `
          + `Files patchers wrote are still in the game.`
          : `Uninstalled ${r.removed.length} of ${r.requested} mod(s).`,
        r.removed.length ? "success" : "warning",
      );
      r.failed.forEach(f => addLog(`Could not remove ${f.mod}: ${f.reason}`, "warning"));
      await load();
    } catch (e: any) {
      addLog(`Bulk uninstall failed: ${e?.message ?? "error"}`, "error");
    } finally {
      setBulkBusy(false);
      setConfirmBulk(false);
      setBulkStatus(null);
    }
  }, [activeProfile, bulkBusy, filtered, removable, addLog, load]);

  const total = mods.length;
  const enabledCount = mods.filter((m) => m.enabled).length;
  const dupeCount = dupeKeys.size;

  const switchProfile = (id: string) => {
    setActiveProfile(id);
    api.setActiveProfile(id).catch(() => { });
  };

  const toggle = async (mod: LibraryMod, next: boolean) => {
    // Optimistic update, revert on error.
    setMods((prev) => prev.map((m) => (m.id === mod.id ? { ...m, enabled: next } : m)));
    setOpenMod((cur) => (cur && cur.id === mod.id ? { ...cur, enabled: next } : cur));
    try {
      if (next) await api.libraryEnable(activeProfile, mod.id);
      else await api.libraryDisable(activeProfile, mod.id);
    } catch (e: any) {
      setMods((prev) => prev.map((m) => (m.id === mod.id ? { ...m, enabled: !next } : m)));
      setOpenMod((cur) => (cur && cur.id === mod.id ? { ...cur, enabled: !next } : cur));
      addLog(`Failed to ${next ? "enable" : "disable"} ${mod.name}: ${e?.message}`, "error");
    }
  };

  const openFolder = async (mod: LibraryMod) => {
    try {
      await api.libraryOpenFolder(activeProfile, mod.id);
    } catch (e: any) {
      addLog(t("library.openFolderFailed", { name: mod.name }), "warning");
    }
  };

  // Delete (uninstall) a mod entirely. Baked TSLPatcher/HoloPatcher mods can't be
  // cleanly removed without a backup, so the backend asks for confirmation
  // (409 baked_no_backup) before we force it.
  const deleteMod = async (mod: LibraryMod, force = false) => {
    if (!force && !(await confirm({ message: t("library.deleteConfirm", { name: mod.name }), confirmLabel: t("common.delete") }))) return;
    try {
      await api.libraryUninstall(activeProfile, mod.id, force);
      addLog(t("library.deleted", { name: mod.name }), "success");
      if (openMod?.id === mod.id) setOpenMod(null);
      load();
    } catch (e: any) {
      if (!force && (e?.status === 409 || e?.data?.error === "baked_no_backup")) {
        const msg = t("library.deleteBakedMessage", { name: mod.name });
        if (await confirm({ message: t("library.deleteForceConfirm", { message: msg }), confirmLabel: t("common.delete") })) {
          deleteMod(mod, true);
        }
        return;
      }
      addLog(t("library.deleteFailed", { name: mod.name, error: e?.message ?? "error" }), "error");
    }
  };

  const openContextMenu = (e: MouseEvent, mod: LibraryMod) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, mod });
  };

  const menuItems = (mod: LibraryMod): ContextMenuItem[] => {
    const items: ContextMenuItem[] = [
      { label: t("modDetail.viewDetails"), icon: ScrollText, onSelect: () => setOpenMod(mod) },
      {
        label: t("library.openFolder"), icon: FolderOpen,
        onSelect: () => openFolder(mod), disabled: !mod.source_exists,
      },
      {
        label: mod.enabled ? t("library.disable") : t("library.enable"), icon: Power,
        onSelect: () => toggle(mod, !mod.enabled), disabled: !mod.toggleable,
      },
      {
        label: t("library.delete"), icon: Trash2,
        onSelect: () => deleteMod(mod), danger: true,
      },
    ];
    return items;
  };

  return (
    <div className="flex h-full flex-col">
      {confirmDialog}
      <header className="space-y-3 view-header border-b">
        <div className="flex items-center gap-3">
          <div>
            <h1 className="text-base font-semibold">{t("library.title")}</h1>
            <p className="text-xs text-muted-foreground">
              {t("library.enabledSummary", { enabled: enabledCount, total })}
              {dupeCount > 0 && ` · ${t("library.duplicateSummary", { count: dupeCount })}`}
            </p>
          </div>
          <div className="ml-auto flex items-center gap-2">
            <span className="text-xs text-muted-foreground">{t("library.enabledOnly")}</span>
            <Switch aria-label={t("library.enabledOnly")} checked={enabledOnly} onCheckedChange={setEnabledOnly} />
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Select
            value={activeProfile}
            onChange={(e) => switchProfile(e.target.value)}
            className="max-w-[14rem]"
            disabled={profiles.length === 0}
          >
            {profiles.length === 0 && <option value="">{t("library.noProfiles")}</option>}
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </Select>
          <Input
            aria-label={t("library.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("library.search")}
            className="max-w-[16rem] flex-1"
          />
          <Select value={method} onChange={(e) => setMethod(e.target.value)} className="w-auto">
            <option value="all">{t("library.allMethods")}</option>
            {methods.map((m) => (
              <option key={m} value={m}>{methodLabel(m)}</option>
            ))}
          </Select>
          {categories.length > 0 && (
            <Select value={category} onChange={(e) => setCategory(e.target.value)} className="w-auto">
              <option value="all">{t("library.allCategories")}</option>
              {categories.map((c) => (
                <option key={c} value={c}>{c}</option>
              ))}
            </Select>
          )}
          {dupeCount > 0 && (
            <Button
              variant={dupesOnly ? "secondary" : "ghost"}
              size="sm"
              onClick={() => setDupesOnly((v) => !v)}
            >
              {t("library.duplicatesOnly")}
            </Button>
          )}
          {mergeable > 0 && (
            <Button
              variant="outline"
              size="sm"
              disabled={bulkBusy}
              title={t("library.dedupeHint")}
              onClick={runDedupe}
            >
              {t("library.dedupe")}
            </Button>
          )}
        </div>
      </header>

      {/* Bulk actions apply to whatever the filters currently show. Removing a
          178-mod build one dialog at a time is not realistic, and filtering
          first is a clearer way to choose than ticking 178 checkboxes. */}
      {!loading && filtered.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 border-b bg-card/20 px-5 py-2">
          <span className="text-xs text-muted-foreground">
            <span className="font-medium text-foreground">{filtered.length}</span>{" "}
            mod{filtered.length === 1 ? "" : "s"} shown
            {filtered.length < total && ` of ${total}`}
            {patcherOnly.length > 0 && (
              <>
                {" · "}
                <span className="text-muted-foreground">
                  {patcherOnly.length} installed by a patcher
                </span>
              </>
            )}
          </span>
          {(bulkProgress || bulkStatus) && (
            <span className="flex min-w-0 items-center gap-1.5 text-xs text-[hsl(var(--info))]">
              <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-[hsl(var(--info))]" />
              {bulkProgress ? (
                <>
                  <span className="shrink-0 tabular-nums">
                    {bulkProgress.current}/{bulkProgress.total}
                  </span>
                  <span className="truncate" title={bulkProgress.mod}>
                    {bulkProgress.mod}
                  </span>
                </>
              ) : (
                bulkStatus
              )}
            </span>
          )}
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <Button size="sm" variant="ghost" disabled={bulkBusy}
              onClick={() => runBulkToggle("enable")}>
              Enable shown
            </Button>
            <Button size="sm" variant="ghost" disabled={bulkBusy}
              onClick={() => runBulkToggle("disable")}>
              Disable shown
            </Button>
            {hasBaseline && total > 0 && (
              confirmReset ? (
                <>
                  <span className="text-xs text-[hsl(var(--destructive))]">
                    Remove every mod and restore the clean game?
                  </span>
                  <Button size="sm" variant="destructive" disabled={bulkBusy}
                    onClick={runReset}>
                    Restore backup
                  </Button>
                  <Button size="sm" variant="ghost" disabled={bulkBusy}
                    onClick={() => setConfirmReset(false)}>
                    Cancel
                  </Button>
                </>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={bulkBusy}
                  onClick={() => setConfirmReset(true)}
                  title="Remove installed mods and restore the original executable"
                >
                  Restore game backup
                </Button>
              )
            )}
            {confirmBulk ? (
              <>
                <span className="text-xs text-[hsl(var(--destructive))]">
                  {removable.length > 0
                    ? `Remove ${removable.length} mod${removable.length === 1 ? "" : "s"}?`
                    : `Remove ${patcherOnly.length} patcher mod${patcherOnly.length === 1 ? "" : "s"}? Game files stay changed.`}
                </span>
                <Button
                  size="sm"
                  variant="destructive"
                  disabled={bulkBusy}
                  onClick={() => runBulkUninstall(removable.length === 0)}
                >
                  {removable.length > 0 ? "Uninstall" : "Remove entries"}
                </Button>
                <Button size="sm" variant="ghost" disabled={bulkBusy}
                  onClick={() => setConfirmBulk(false)}>
                  Cancel
                </Button>
              </>
            ) : (
              <Button
                size="sm"
                variant="outline"
                disabled={bulkBusy || filtered.length === 0}
                onClick={() => setConfirmBulk(true)}
                title={
                  removable.length > 0
                    ? `Removes ${removable.length} mod(s) and their files`
                    : "These were installed by a patcher and cannot be removed individually"
                }
              >
                {removable.length > 0
                  ? `Uninstall ${removable.length} shown`
                  : "Remove patcher entries"}
              </Button>
            )}
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {loading ? (
          <EmptyState title={t("library.loading")} />
        ) : errored ? (
          <EmptyState title={t("library.loadFailed")}
            action={{ label: t("library.retryLoad"), onClick: load }} />
        ) : total === 0 ? (
          <EmptyState
           
            title={t("library.emptyTitle")}
            subtitle={t("library.emptySubtitle")}
            action={{ label: t("library.goToBuilds"), onClick: onGoToBuilds }}
          />
        ) : filtered.length === 0 ? (
          <EmptyState
           
            title={t("library.noMatchTitle")}
            subtitle={t("library.noMatchSubtitle")}
          />
        ) : (
          <div className="space-y-0.5">
            {filtered.map((m) => (
              <LibraryRow
                key={`${m.game}:${m.id}`}
                mod={m}
                duplicate={dupeKeys.has(dupeKey(m))}
                onToggle={(next) => toggle(m, next)}
                onConflictClick={onGoToConflicts}
                onOpen={() => setOpenMod(m)}
                onContextMenu={(e) => openContextMenu(e, m)}
                onDelete={() => deleteMod(m)}
              />
            ))}
          </div>
        )}
      </div>

      {!loading && total > 0 && (
        <footer className="border-t bg-card/40 px-5 py-2">
          <Button variant="ghost" size="sm" onClick={onGoToBuilds}>{t("library.installMore")}</Button>
        </footer>
      )}

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menuItems(menu.mod)}
          onClose={() => setMenu(null)}
        />
      )}

      {openMod && (
        <ModDetail
          mod={openMod}
          profile={activeProfile}
          onClose={() => setOpenMod(null)}
          onToggle={(next) => toggle(openMod, next)}
          onUninstalled={load}
          addLog={addLog}
        />
      )}
    </div>
  );
}
