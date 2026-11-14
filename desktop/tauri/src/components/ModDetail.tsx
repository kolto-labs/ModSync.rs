import { useEffect, useState } from "react";
import {
  X, ExternalLink, Trash2, FileText, Loader2, AlertTriangle,
} from "lucide-react";
import {
  api, type LibraryMod, type ModInfo, type DeployedFile, type BakedFile,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Dialog } from "@/components/ui/dialog";
import { Screenshots } from "@/components/Screenshots";
import { cn } from "@/lib/utils";
import { useConfirm } from "@/components/useConfirm";
import { useT } from "@/lib/i18n";

const HOST_NAMES: Record<string, string> = {
  mega: "MEGA", googledrive: "Google Drive", github: "GitHub",
  gamefront: "GameFront", direct: "the download page",
};

interface ModDetailProps {
  mod: LibraryMod;
  profile: string;
  onClose: () => void;
  onToggle: (next: boolean) => void;
  onUninstalled: () => void;
  addLog: (message: string, tag?: string) => void;
}

function fmtSize(bytes: number): string {
  if (!bytes) return "";
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

export function ModDetail({ mod, profile, onClose, onToggle, onUninstalled, addLog }: ModDetailProps) {
  const t = useT();
  const [confirm, confirmDialog] = useConfirm();
  const [info, setInfo] = useState<ModInfo | null>(null);
  const [infoLoading, setInfoLoading] = useState(true);
  const [deployed, setDeployed] = useState<DeployedFile[]>([]);
  const [baked, setBaked] = useState<BakedFile[]>([]);
  const [filesLoading, setFilesLoading] = useState(true);
  const [uninstalling, setUninstalling] = useState(false);
  const [bakedPrompt, setBakedPrompt] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Only DeadlyStream mods have a DeadlyStream page to read details from.
  const onDeadlyStream = mod.source_host === "deadlystream";

  useEffect(() => {
    let alive = true;
    if (onDeadlyStream) {
      setInfoLoading(true);
      api.modInfo(mod.source_ref, mod.source_slug, mod.game)
        .then((r) => { if (alive) setInfo(r); })
        .catch(() => { if (alive) setInfo(null); })
        .finally(() => { if (alive) setInfoLoading(false); });
    } else {
      setInfo(null);
      setInfoLoading(false);
    }

    setFilesLoading(true);
    api.libraryDetail(mod.id, profile)
      .then((r) => { if (alive) { setDeployed(r.deployed_files ?? []); setBaked(r.baked_files ?? []); } })
      .catch(() => { if (alive) { setDeployed([]); setBaked([]); } })
      .finally(() => { if (alive) setFilesLoading(false); });

    return () => { alive = false; };
  }, [mod.id, mod.source_ref, mod.source_slug, mod.game, profile, onDeadlyStream]);

  const runUninstall = async (force: boolean) => {
    setBakedPrompt(null);
    setUninstalling(true);
    try {
      await api.libraryUninstall(profile, mod.id, force);
      addLog(`Uninstalled ${mod.name}.`, "success");
      onUninstalled();
      onClose();
    } catch (e: any) {
      // Baked (TSLPatcher/HoloPatcher) mods can't be cleanly removed without a
      // backup. The backend returns 409 baked_no_backup unless force=true.
      if (!force && (e?.status === 409 || e?.data?.error === "baked_no_backup")) {
        setBakedPrompt(t("modDetail.bakedMessage"));
        setUninstalling(false);
        return;
      }
      addLog(`Failed to uninstall ${mod.name}: ${e?.message}`, "error");
      setUninstalling(false);
    }
  };

  const uninstall = async () => {
    if (!(await confirm({ message: t("modDetail.uninstallConfirm", { name: mod.name }), confirmLabel: t("common.delete") }))) return;
    runUninstall(false);
  };

  const description = info?.description?.trim();
  const images = info?.images ?? [];
  const baked_mode = mod.deploy_kind === "baked" || (deployed.length === 0 && baked.length > 0);

  // One link, to the page the mod is actually hosted on. Nothing when that is
  // not known (imported mods, or installs from before it was recorded).
  const links: { label: string; url?: string }[] = onDeadlyStream
    ? [{ label: t("modDetail.viewOnDeadlyStream"), url: info?.ds_url }]
    : mod.source_host === "nexus"
      ? [{ label: t("modDetail.viewOnNexus"), url: mod.source_url }]
      : mod.source_host
        ? [{ label: t("modDetail.viewOnSite", { site: HOST_NAMES[mod.source_host] ?? mod.source_host }), url: mod.source_url }]
        : [];

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      {confirmDialog}
      <div className="absolute inset-0 bg-black/60 animate-fade-in" onClick={onClose} />
      <aside className="relative z-10 flex h-full w-full max-w-lg animate-fade-in flex-col border-l bg-card shadow-xl">
        {/* Header */}
        <div className="flex items-start gap-3 border-b p-4">
          <div className="min-w-0 flex-1">
            <h2 className="text-base font-semibold leading-tight" title={mod.name}>{mod.name}</h2>
            <div className="mt-1.5 flex flex-wrap items-center gap-2">
              <Badge variant={mod.game === "KOTOR1" ? "info" : "secondary"}>{mod.game}</Badge>
              {mod.install_method && <Badge variant="muted">{mod.install_method}</Badge>}
              <Badge variant={mod.enabled ? "success" : "muted"}>{mod.enabled ? t("library.enabled") : t("library.disabled")}</Badge>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Switch checked={mod.enabled} disabled={!mod.toggleable} onCheckedChange={onToggle} />
            <button onClick={onClose} className="rounded-sm text-muted-foreground transition-colors hover:text-foreground">
              <X className="size-4" />
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="min-h-0 flex-1 space-y-5 overflow-auto p-4">
          {/* Description */}
          <section>
            {infoLoading ? (
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" /> {t("modDetail.loadingDetails")}
              </p>
            ) : description ? (
              <p className="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">{description}</p>
            ) : (
              <p className="text-sm italic text-muted-foreground">{t("modDetail.noDescription")}</p>
            )}
          </section>

          {/* Screenshots */}
          {!infoLoading && images.length > 0 && <Screenshots images={images} />}

          {/* Links */}
          {!infoLoading && links.some((l) => l.url) && (
            <section className="flex flex-wrap gap-2">
              {links.filter((l) => l.url).map((l) => (
                <Button key={l.label} variant="outline" size="sm"
                        onClick={() => api.openUrl(l.url!).catch(() => {})}>
                  <ExternalLink /> {l.label}
                </Button>
              ))}
            </section>
          )}

          {/* Assets / files */}
          <section className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              {baked_mode ? t("modDetail.modifiedFiles") : t("modDetail.installedFiles")}
            </h3>
            {filesLoading ? (
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" /> {t("modDetail.loadingFiles")}
              </p>
            ) : baked_mode ? (
              baked.length === 0 ? (
                <p className="text-sm italic text-muted-foreground">{t("modDetail.noFiles")}</p>
              ) : (
                <ul className="max-h-64 space-y-0.5 overflow-auto rounded-md border bg-background/40 p-2">
                  {baked.map((f) => (
                    <li key={f.rel_path} className="flex items-center gap-2 px-1 py-0.5 text-xs">
                      <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                      <span className="flex-1 truncate font-mono" title={f.rel_path}>{f.rel_path}</span>
                      {f.created && <Badge variant="muted" className="text-[10px]">{t("modDetail.fileNew")}</Badge>}
                    </li>
                  ))}
                </ul>
              )
            ) : deployed.length === 0 ? (
              <p className="text-sm italic text-muted-foreground">{t("modDetail.noFiles")}</p>
            ) : (
              <ul className="max-h-64 space-y-0.5 overflow-auto rounded-md border bg-background/40 p-2">
                {deployed.map((f) => (
                  <li key={f.rel_path} className="flex items-center gap-2 px-1 py-0.5 text-xs">
                    <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="flex-1 truncate font-mono" title={f.rel_path}>{f.rel_path}</span>
                    {f.overwrote && <Badge variant="warning" className="text-[10px]">{t("modDetail.fileOverwrote")}</Badge>}
                    <span className="shrink-0 text-muted-foreground">{fmtSize(f.size)}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>

        {/* Footer */}
        <div className="flex items-center gap-2 border-t p-4">
          <Button variant="destructive" size="sm" disabled={uninstalling} onClick={uninstall}
                  className={cn(uninstalling && "opacity-70")}>
            <Trash2 /> {uninstalling ? t("modDetail.uninstalling") : t("modDetail.uninstall")}
          </Button>
        </div>
      </aside>

      {/* Baked-mod uninstall confirmation (TSLPatcher/HoloPatcher) */}
      <Dialog
        open={!!bakedPrompt}
        onClose={() => setBakedPrompt(null)}
        title={t("modDetail.bakedTitle")}
      >
        <div className="space-y-4">
          <div className="flex gap-3">
            <AlertTriangle className="mt-0.5 size-5 shrink-0 text-[hsl(var(--warning))]" />
            <p className="text-sm text-muted-foreground">{bakedPrompt}</p>
          </div>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setBakedPrompt(null)}>
              {t("common.cancel")}
            </Button>
            <Button variant="destructive" size="sm" onClick={() => runUninstall(true)}>
              <Trash2 /> {t("modDetail.removeAnyway")}
            </Button>
          </div>
        </div>
      </Dialog>
    </div>
  );
}
