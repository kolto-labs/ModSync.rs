import { useEffect, useRef, useState, type MouseEvent } from "react";
import { AlertTriangle, Copy, Trash2 } from "lucide-react";
import { api, type LibraryMod } from "@/lib/api";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

interface LibraryRowProps {
  mod: LibraryMod;
  duplicate?: boolean;
  onToggle: (enabled: boolean) => void;
  onConflictClick: () => void;
  onOpen: () => void;
  onContextMenu: (e: MouseEvent) => void;
  onDelete: () => void;
}

function fmtDate(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  if (isNaN(d.getTime())) return "";
  return d.toLocaleDateString();
}

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

// Module-level cache so thumbnails aren't re-fetched as rows mount/unmount.
const thumbCache = new Map<string, string | null>();

/** A mod has a fetchable thumbnail only if it came from an online source. */
function thumbKey(mod: LibraryMod): string | null {
  if (mod.source_type === "import" || !mod.source_ref) return null;
  // Thumbnails come from the DeadlyStream page; other hosts have none to fetch.
  if (mod.source_host !== "deadlystream") return null;
  return `${mod.game}:${mod.source_ref}:${mod.source_slug}`;
}

function Thumbnail({ mod }: { mod: LibraryMod }) {
  const key = thumbKey(mod);
  const ref = useRef<HTMLDivElement>(null);
  const [url, setUrl] = useState<string | null>(() => (key ? thumbCache.get(key) ?? null : null));
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!key || thumbCache.has(key)) return;
    const el = ref.current;
    if (!el) return;
    let alive = true;
    const io = new IntersectionObserver((entries) => {
      if (!entries[0]?.isIntersecting) return;
      io.disconnect();
      api.modInfo(mod.source_ref, mod.source_slug, mod.game)
        .then((r) => {
          const first = r.images?.[0];
          const proxied = first ? api.imageProxy(first) : null;
          thumbCache.set(key, proxied);
          if (alive) setUrl(proxied);
        })
        .catch(() => { thumbCache.set(key, null); });
    }, { rootMargin: "200px" });
    io.observe(el);
    return () => { alive = false; io.disconnect(); };
  }, [key, mod.source_ref, mod.source_slug, mod.game]);

  return (
    <div
      ref={ref}
      className="flex size-9 shrink-0 items-center justify-center overflow-hidden rounded"
    >
      {url && !failed && (
        <img
          src={url}
          alt=""
          loading="lazy"
          onError={() => setFailed(true)}
          className="size-full object-cover"
        />
      )}
    </div>
  );
}

export function LibraryRow({
  mod, duplicate, onToggle, onConflictClick, onOpen, onContextMenu, onDelete,
}: LibraryRowProps) {
  const t = useT();
  const sub = [mod.game, mod.install_method ? methodLabel(mod.install_method) : null, mod.category, fmtDate(mod.install_ts)]
    .filter(Boolean).join(" · ");

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onContextMenu={onContextMenu}
      onKeyDown={(e) => { if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); onOpen(); } }}
      className="mod-row flex cursor-pointer items-center gap-3 border border-transparent px-3 py-2.5 transition-colors hover:bg-card/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <span className="w-8 shrink-0 text-right font-mono text-xs text-muted-foreground">
        {mod.load_order + 1}
      </span>
      <Thumbnail mod={mod} />
      <div className="min-w-0 flex-1">
        <p className={cn("truncate text-sm", !mod.enabled && "text-muted-foreground")} title={mod.name}>
          {mod.name}
        </p>
        <p className="truncate text-xs text-muted-foreground">{sub}</p>
      </div>
      {duplicate && mod.duplicate_unmergeable && (
        <Badge variant="warning" className="gap-1" title={t("library.duplicateUnmergeableHint")}>
          <Copy className="size-3" />
          {t("library.duplicateUnmergeable")}
        </Badge>
      )}
      {duplicate && !mod.duplicate_unmergeable && (
        <Badge variant="muted" className="gap-1" title={t("library.duplicateHint")}>
          <Copy className="size-3" />
          {t("library.duplicate")}
        </Badge>
      )}
      {mod.has_conflict && (
        <button onClick={(e) => { e.stopPropagation(); onConflictClick(); }} title={t("library.viewConflicts")}>
          <Badge variant="warning" className="gap-1">
            <AlertTriangle className="size-3" />
            {mod.conflict_count > 0 ? mod.conflict_count : t("library.conflict")}
          </Badge>
        </button>
      )}
      <div onClick={(e) => e.stopPropagation()}>
        <Switch
          aria-label={`${t(mod.enabled ? "library.disable" : "library.enable")} ${mod.name}`}
          checked={mod.enabled}
          disabled={!mod.toggleable}
          onCheckedChange={onToggle}
        />
      </div>
      <button
        onClick={(e) => { e.stopPropagation(); onDelete(); }}
        title={t("library.delete")}
        aria-label={t("library.delete")}
        className="shrink-0 rounded-sm p-1.5 text-muted-foreground/70 transition-colors hover:bg-destructive/10 hover:text-destructive focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <Trash2 className="size-4" />
      </button>
    </div>
  );
}
