import { useState } from "react";
import { ChevronDown, ChevronUp, Lightbulb } from "lucide-react";
import { api, type Conflict, type ConflictParticipant } from "@/lib/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

export interface ConflictGroup {
  gkey: string;
  participants: ConflictParticipant[];
  items: Conflict[];
  severity: Conflict["severity"];
  winner_mod_id: string | null;
  same_build?: boolean;
}

interface ConflictCardProps {
  group: ConflictGroup;
  profile: string;
  addLog: (message: string, tag?: string) => void;
  onResolved: (conflicts?: Conflict[]) => void;
}

function joinNames(names: string[]): string {
  const u = [...new Set(names)];
  if (u.length === 0) return "";
  if (u.length === 1) return `"${u[0]}"`;
  if (u.length === 2) return `"${u[0]}" and "${u[1]}"`;
  return u.slice(0, -1).map(n => `"${n}"`).join(", ") + `, and "${u[u.length - 1]}"`;
}

// Shared button style used inside conflict cards.
const BTN_BASE =
  "inline-flex items-center gap-1.5 rounded-sm border px-1.5 py-0.5 text-[11px] transition-colors";

export function ConflictCard({ group, profile, addLog, onResolved }: ConflictCardProps) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [showAllFiles, setShowAllFiles] = useState(false);

  const { participants, items, severity, winner_mod_id } = group;
  const isDeclared = items[0]?.type === "declared";
  const sameBuild = group.same_build ?? items[0]?.same_build ?? false;
  const winner = participants.find(p => p.mod_id === winner_mod_id);
  const losers = winner_mod_id
    ? participants.filter(p => p.mod_id !== winner_mod_id && p.enabled)
    : [];

  const files = items.filter(c => c.type !== "declared").map(c => c.resource);
  // Declared incompatibilities carry the overlap the backend computed.
  const sharedFiles = items[0]?.shared_files ?? [];
  const sharedFileCount = items[0]?.shared_file_count ?? sharedFiles.length;
  const description = items[0]?.description ?? "";
  const recommendation = items[0]?.recommendation ?? "";
  const modNames = participants.map(p => p.mod_name);

  const disableMods = async (modIds: string[]) => {
    if (busy || !profile || modIds.length === 0) return;
    setBusy(true);
    try {
      let latest: Conflict[] | undefined;
      for (const id of modIds) {
        const r = await api.libraryDisable(profile, id);
        if (r.conflicts) latest = r.conflicts;
      }
      const names = modIds
        .map(id => participants.find(p => p.mod_id === id)?.mod_name ?? id)
        .join(", ");
      addLog(t("conflicts.resolvedLog", { mods: names }), "success");
      onResolved(latest);
    } catch (e: any) {
      addLog(t("conflicts.resolveFailed", { error: e?.message ?? "error" }), "error");
    } finally {
      setBusy(false);
    }
  };

  // ---- Load-order / file-sharing notes (info severity) ----
  // These are expected in curated builds. Show them collapsed with minimal
  // visual weight; no action buttons.
  if (!isDeclared && severity === "info") {
    return (
      <div className="rounded-lg border bg-card/20 p-3 transition-opacity">
        <div className="flex items-center gap-2">
          <span className="size-1.5 shrink-0 rounded-full bg-[hsl(var(--info))]" />
          <p className="min-w-0 flex-1 truncate text-sm text-muted-foreground" title={modNames.join(", ")}>
            {joinNames(modNames)}
          </p>
          {sameBuild && (
            <Badge variant="info" className="shrink-0 text-[10px]">{t("conflicts.sameBuild")}</Badge>
          )}
          <Badge variant="info" className="shrink-0 text-[10px]">{t("conflicts.noActionNeeded")}</Badge>
          <button
            onClick={() => setExpanded(v => !v)}
            className="shrink-0 text-muted-foreground/60 hover:text-muted-foreground"
            aria-label={expanded ? "Collapse" : "Expand"}
          >
            {expanded ? <ChevronUp className="size-4" /> : <ChevronDown className="size-4" />}
          </button>
        </div>
        {expanded && (
          <div className="mt-2.5 space-y-1.5 pl-3.5">
            {description && (
              <p className="text-xs leading-relaxed text-muted-foreground">{description}</p>
            )}
            {files.length > 0 && (
              <p className="text-[11px] text-muted-foreground/60">
                {files.length === 1 ? files[0] : `${files.length} shared files`}
                {files.length > 1 && (
                  <span className="ml-1 text-muted-foreground/40">
                    ({files.slice(0, 3).join(", ")}{files.length > 3 ? `, +${files.length - 3} more` : ""})
                  </span>
                )}
              </p>
            )}
          </div>
        )}
      </div>
    );
  }

  // ---- File-level warning (mods from different builds sharing files) ----
  if (!isDeclared) {
    const enabledParticipants = participants.filter(p => p.enabled && p.toggleable !== false);
    return (
      <div className={cn("rounded-lg border bg-card/40 p-4", "border-[hsl(var(--warning)/0.25)]")}>
        <div className="flex items-start gap-2">
          <span className="mt-1 size-2 shrink-0 rounded-full bg-[hsl(var(--warning))]" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium" title={modNames.join(", ")}>
              {joinNames(modNames)}
            </p>
            {files.length > 0 && (
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                {files.length === 1 ? files[0] : `${files.length} shared files`}
              </p>
            )}
          </div>
          <Badge variant="warning" className="shrink-0">shared files</Badge>
        </div>
        {description && (
          <p className="mt-2.5 text-sm leading-relaxed text-foreground">{description}</p>
        )}
        {recommendation && (
          <div className="mt-2.5 flex items-start gap-2 rounded-md border border-[hsl(var(--info)/0.3)] bg-[hsl(var(--info)/0.08)] p-2.5">
            <Lightbulb className="mt-0.5 size-4 shrink-0 text-[hsl(var(--info))]" />
            <p className="text-sm text-muted-foreground">{recommendation}</p>
          </div>
        )}
        {enabledParticipants.length > 0 && (
          <div className="mt-3 space-y-2 border-t pt-3">
            <p className="text-xs text-muted-foreground">{t("conflicts.ifIssuesDisable")}</p>
            <div className="flex flex-wrap gap-2">
              {enabledParticipants.map(p => (
                <Button key={p.mod_id} size="sm" variant="outline" disabled={busy}
                  onClick={() => disableMods([p.mod_id])}>
                  {t("conflicts.disableMod", { mod: p.mod_name })}
                </Button>
              ))}
            </div>
          </div>
        )}
      </div>
    );
  }

  // ---- Declared incompatibility ----
  // Structured to answer, in order: does this actually collide, what does each
  // mod change, and what can I do about it. The old card asserted the mods were
  // incompatible and then contradicted itself further down by admitting they
  // shared no files; leading with the evidence avoids that.
  const overlaps = sharedFileCount > 0;
  return (
    <div
      className={cn(
        "rounded-lg border bg-card/40 p-4",
        overlaps ? "border-destructive/25" : "border-border",
      )}
    >
      <div className="flex items-start gap-2">
        <span
          className={cn(
            "mt-1 size-2 shrink-0 rounded-full",
            overlaps ? "bg-destructive" : "bg-muted-foreground/40",
          )}
        />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-foreground">{joinNames(modNames)}</p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {overlaps
              ? `Both write ${sharedFileCount} of the same file${sharedFileCount === 1 ? "" : "s"}`
              : "No files in common"}
          </p>
        </div>
        <Badge variant={overlaps ? "warning" : "info"} className="shrink-0">
          {overlaps ? "review needed" : "no overlap"}
        </Badge>
      </div>

      {/* Evidence first: what each mod actually writes, with the shared files
          called out. This is the whole basis for the player's decision. */}
      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        {participants.map(p => {
          const shared = new Set(sharedFiles);
          const list = p.files ?? [];
          return (
            <div key={p.mod_id} className="rounded-md border bg-muted/20 p-2.5">
              <p className="truncate text-xs font-medium text-foreground" title={p.mod_name}>
                {p.mod_name}
              </p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                {p.file_count ?? list.length} file
                {(p.file_count ?? list.length) === 1 ? "" : "s"}
                {p.install_method ? ` · ${p.install_method.toLowerCase().replace(/_/g, " ")}` : ""}
              </p>
              <ul className="mt-1.5 space-y-0.5">
                {(showAllFiles ? list : list.slice(0, 4)).map(f => (
                  <li
                    key={f}
                    className={cn(
                      "truncate font-mono text-[11px]",
                      shared.has(f)
                        ? "text-[hsl(var(--warning))]"
                        : "text-muted-foreground/70",
                    )}
                    title={shared.has(f) ? `${f} (written by both)` : f}
                  >
                    {f.replace(/^Override\//, "")}
                  </li>
                ))}
              </ul>
              {list.length > 4 && (
                <button
                  type="button"
                  onClick={() => setShowAllFiles(v => !v)}
                  className="mt-1 text-[11px] text-muted-foreground underline hover:text-foreground"
                >
                  {showAllFiles ? "Show fewer" : `Show all ${list.length}`}
                </button>
              )}
            </div>
          );
        })}
      </div>

      {overlaps ? (
        <p className="mt-2.5 text-xs text-muted-foreground">
          Files written by both are highlighted. The one installed later wins.
        </p>
      ) : (
        <p className="mt-2.5 text-xs text-muted-foreground">
          {joinNames(modNames)} change different files, so nothing is being
          overwritten. The warning comes from a mod readme describing the kind of
          mod it clashes with, not from anything found in your install.
        </p>
      )}

      {/* Per-mod action, with the reason inline when there is nothing to press.
          A patcher mod has no toggle at all, so saying why beats a dead button
          or an unexplained gap. */}
      <div className="mt-3 space-y-2 border-t pt-3">
        <p className="text-xs text-muted-foreground">
          {overlaps
            ? "If something looks wrong in game, turn one off:"
            : "Nothing to do unless you see a problem in game."}
        </p>
        <div className="flex flex-wrap items-center gap-2">
          {participants.filter(p => p.enabled).map(p =>
            p.toggleable === false ? (
              <span
                key={p.mod_id}
                className="rounded-sm border border-dashed px-2 py-1 text-[11px] text-muted-foreground"
                title="Patcher mods write into shared game files"
              >
                {p.mod_name} · can only be removed by reinstalling
              </span>
            ) : (
              <Button
                key={p.mod_id}
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={() => disableMods([p.mod_id])}
              >
                {t("conflicts.disableMod", { mod: p.mod_name })}
              </Button>
            ),
          )}
        </div>
      </div>
    </div>
  );
}
