import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { FolderOpen, CheckCircle2, FileText } from "lucide-react";
import type { BuildMod, ModStatus } from "@/lib/api";
import { STATUS_META, ACTIVE_STATUSES } from "@/lib/status";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

export interface ModRuntime {
  status: ModStatus;
  detail: string;
  progress: number; // 0..100
  progressLabel: string;
  error?: string;
  manualFolder?: string;
  manualReadme?: string;
}

export const DEFAULT_RUNTIME: ModRuntime = {
  status: "PENDING",
  detail: "",
  progress: 0,
  progressLabel: "",
};

function Row({
  mod,
  rt,
  onOpen,
  onContextMenu,
  selectable,
  selected,
  onToggle,
  onManualOpen,
  onManualDone,
}: {
  mod: BuildMod;
  rt: ModRuntime;
  onOpen?: () => void;
  onContextMenu?: (e: MouseEvent) => void;
  selectable?: boolean;
  selected?: boolean;
  onToggle?: (fileId: string) => void;
  onManualOpen?: (mod: BuildMod) => void;
  onManualDone?: (mod: BuildMod) => void;
}) {
  const t = useT();
  const [readmeOpen, setReadmeOpen] = useState(false);
  const meta = STATUS_META[rt.status];
  const active = ACTIVE_STATUSES.includes(rt.status);
  const showBar = active && (rt.progress > 0 || rt.status !== "WAITING_PATCHER");
  const deemphasized = selectable && !selected;

  return (
    <div
      data-fileid={mod.file_id}
      role={onOpen ? "button" : undefined}
      tabIndex={onOpen ? 0 : undefined}
      onClick={onOpen}
      onContextMenu={onContextMenu}
      onKeyDown={
        onOpen
          ? (e) => { if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); onOpen(); } }
          : undefined
      }
      className={cn(
        "mod-row flex flex-col gap-1 border border-transparent px-3 py-2.5 transition-colors",
        active ? "bg-accent/40 border-border" : "hover:bg-card/60",
        deemphasized && "opacity-50",
        onOpen && "cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      )}
    >
      <div className="flex items-center gap-3">
        {selectable && (
          <div onClick={(e) => e.stopPropagation()} className="flex shrink-0 items-center">
            <Checkbox
              checked={!!selected}
              onCheckedChange={() => onToggle?.(mod.file_id)}
              aria-label={mod.name}
            />
          </div>
        )}
        <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground">
          {mod.install_order}
        </span>
        <span className="flex-1 truncate text-sm" title={mod.name}>
          {mod.name}
        </span>
        {rt.progressLabel && active ? (
          <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
            {rt.progressLabel}
          </span>
        ) : null}
        {/* Where the mod comes from. Only shown when the app cannot fetch it
            itself, since that is the case worth knowing before starting a
            build - the rest download without you doing anything. */}
        {mod.auto_downloadable === false && mod.source_label && (
          <Badge
            variant="warning"
            className="shrink-0"
            title={t("builds.manualDownloadHint", { source: mod.source_label })}
          >
            {mod.source_label}
          </Badge>
        )}
        {mod.installed && rt.status === "PENDING" && (
          <Badge variant="success" className="shrink-0 border-0 bg-transparent px-0">Installed</Badge>
        )}
        {(!mod.installed || rt.status !== "PENDING") && (
          <Badge variant={meta.variant} className="shrink-0 border-0 bg-transparent px-0">
            {rt.status === "DOWNLOADING" && rt.detail ? rt.detail : meta.label}
          </Badge>
        )}
      </div>
      {showBar && <div className="pl-11"><Progress value={rt.progress} className="h-1" /></div>}
      {rt.status === "ERROR" && (rt.error || rt.detail) && (
        <div className="ml-11 flex items-center gap-1 text-[11px] text-destructive/90">
          <span className="truncate" title={rt.error || rt.detail}>{rt.error || rt.detail}</span>
          {onOpen && <span className="shrink-0 underline opacity-80">{t("builds.viewError")}</span>}
        </div>
      )}
      {rt.status === "MANUAL" && (
        <div
          className="ml-11 flex flex-col gap-2"
          onClick={(e) => e.stopPropagation()}
        >
          <div className="space-y-0.5">
            <p className="text-[11px] font-medium text-[hsl(var(--warning))]">
              {t("builds.manualNeeded")}
            </p>
            <p className="text-[11px] text-muted-foreground">
              {t("builds.manualInstructions")}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {onManualOpen && (
              <button
                onClick={() => onManualOpen(mod)}
                className="inline-flex items-center gap-1 rounded-sm border border-[hsl(var(--warning)/0.4)] px-1.5 py-0.5 text-[11px] text-[hsl(var(--warning))] transition-colors hover:bg-[hsl(var(--warning)/0.1)]"
              >
                <FolderOpen className="size-3" /> {t("builds.manualOpenFolder")}
              </button>
            )}
            {onManualDone && (
              <button
                onClick={() => onManualDone(mod)}
                className="inline-flex items-center gap-1 rounded-sm border border-[hsl(var(--warning)/0.4)] px-1.5 py-0.5 text-[11px] text-[hsl(var(--warning))] transition-colors hover:bg-[hsl(var(--warning)/0.1)]"
              >
                <CheckCircle2 className="size-3" /> {t("builds.manualMarkDone")}
              </button>
            )}
            {rt.manualReadme && (
              <button
                onClick={() => setReadmeOpen((v) => !v)}
                className="inline-flex items-center gap-1 rounded-sm border border-border px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-muted/50"
              >
                <FileText className="size-3" />
                {readmeOpen ? t("builds.manualHideReadme") : t("builds.manualShowReadme")}
              </button>
            )}
          </div>
          {readmeOpen && rt.manualReadme && (
            <pre className="max-h-52 overflow-auto rounded-md bg-muted/40 p-3 text-[11px] leading-relaxed text-muted-foreground whitespace-pre-wrap">
              {rt.manualReadme}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

/** How long the list leaves the scroll position alone after the player scrolls it. */
const USER_SCROLL_PAUSE_MS = 10_000;

export function ModList({
  mods,
  runtime,
  onOpenMod,
  onContextMenu,
  selectable,
  selected,
  onToggle,
  onManualOpen,
  onManualDone,
}: {
  mods: BuildMod[];
  runtime: Record<string, ModRuntime>;
  onOpenMod?: (mod: BuildMod) => void;
  onContextMenu?: (e: MouseEvent, mod: BuildMod) => void;
  selectable?: boolean;
  selected?: Set<string>;
  onToggle?: (fileId: string) => void;
  onManualOpen?: (mod: BuildMod) => void;
  onManualDone?: (mod: BuildMod) => void;
}) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);

  const userScrolledAt = useRef(0);
  const noteUserScroll = () => { userScrolledAt.current = Date.now(); };

  // Keep the earliest mod still in progress in view. Several mods download at
  // once, so following whichever one reported last made the list jump back and
  // forth between them; the earliest one only ever moves down the list.
  const followId = useMemo(
    () => mods.find((m) => ACTIVE_STATUSES.includes(runtime[m.file_id]?.status))?.file_id ?? null,
    [mods, runtime],
  );

  useEffect(() => {
    const box = containerRef.current;
    if (!followId || !box) return;
    // Leave the list alone while the player is looking through it themselves.
    if (Date.now() - userScrolledAt.current < USER_SCROLL_PAUSE_MS) return;
    const el = box.querySelector<HTMLElement>(`[data-fileid="${CSS.escape(followId)}"]`);
    if (!el) return;
    // Scroll only this list. scrollIntoView would also scroll every container
    // around it, shifting the whole screen.
    const b = box.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    const gap = 8;
    if (r.top < b.top) box.scrollTo({ top: box.scrollTop + r.top - b.top - gap, behavior: "smooth" });
    else if (r.bottom > b.bottom) box.scrollTo({ top: box.scrollTop + r.bottom - b.bottom + gap, behavior: "smooth" });
  }, [followId]);

  if (mods.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t("builds.empty")}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="h-full space-y-0.5 overflow-auto pr-1"
      onWheel={noteUserScroll}
      onTouchMove={noteUserScroll}
      onPointerDown={noteUserScroll}
      onKeyDown={noteUserScroll}
    >
      {mods.map((m) => (
        <Row
          key={m.file_id}
          mod={m}
          rt={runtime[m.file_id] ?? DEFAULT_RUNTIME}
          onOpen={onOpenMod ? () => onOpenMod(m) : undefined}
          onContextMenu={onContextMenu ? (e) => onContextMenu(e, m) : undefined}
          selectable={selectable}
          selected={selected?.has(m.file_id)}
          onToggle={onToggle}
          onManualOpen={onManualOpen}
          onManualDone={onManualDone}
        />
      ))}
    </div>
  );
}
