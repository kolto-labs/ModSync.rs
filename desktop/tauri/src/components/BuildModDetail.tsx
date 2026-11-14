import { useEffect, useState } from "react";
import { X, ExternalLink, Loader2, AlertTriangle } from "lucide-react";
import { api, type BuildMod, type ModInfo } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Screenshots } from "@/components/Screenshots";
import { useT } from "@/lib/i18n";

interface BuildModDetailProps {
  mod: BuildMod;
  onClose: () => void;
  error?: string;
}

// Lightweight detail drawer for a build mod (no installed-file data - that's
// only available once a mod is in the library).
export function BuildModDetail({ mod, onClose, error }: BuildModDetailProps) {
  const t = useT();
  const [info, setInfo] = useState<ModInfo | null>(null);
  // Only DeadlyStream mods have a DeadlyStream page to read details from. The
  // others carry a placeholder id, so asking would just fail.
  const onDeadlyStream = (mod.source_host ?? "deadlystream") === "deadlystream";
  const [infoLoading, setInfoLoading] = useState(onDeadlyStream);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    if (!onDeadlyStream) { setInfo(null); setInfoLoading(false); return; }
    let alive = true;
    setInfo(null);
    setInfoLoading(true);
    api.modInfo(mod.file_id, mod.slug, mod.game)
      .then((r) => { if (alive) setInfo(r); })
      .catch(() => { if (alive) setInfo(null); })
      .finally(() => { if (alive) setInfoLoading(false); });
    return () => { alive = false; };
  }, [mod.file_id, mod.slug, mod.game, onDeadlyStream]);

  const title = info?.title?.trim() || mod.name;
  const description = info?.description?.trim() || mod.description?.trim();
  const images = info?.images ?? [];
  const note = mod.note?.trim();
  const summary = mod.directive_summary?.trim();
  const instructions = mod.instructions?.trim();

  // One link, to the page the mod is actually hosted on. For Nexus that is the
  // exact mod page from the build guide, not a search.
  const links: { label: string; url?: string }[] = [
    onDeadlyStream
      ? { label: t("modDetail.viewOnDeadlyStream"), url: info?.ds_url || mod.url }
      : mod.source_host === "nexus"
        ? { label: t("modDetail.viewOnNexus"), url: mod.url }
        : { label: t("modDetail.viewOnSite", { site: mod.source_label ?? "" }), url: mod.url },
  ];
  for (const [index, url] of (mod.patch_urls ?? []).entries()) {
    links.push({ label: `Linked patch ${index + 1}`, url });
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      <div className="absolute inset-0 bg-black/60 animate-fade-in" onClick={onClose} />
      <aside className="relative z-10 flex h-full w-full max-w-lg animate-fade-in flex-col border-l bg-card shadow-xl">
        {/* Header */}
        <div className="flex items-start gap-3 border-b p-4">
          <div className="min-w-0 flex-1">
            <h2 className="text-base font-semibold leading-tight" title={title}>{title}</h2>
            <div className="mt-1.5 flex flex-wrap items-center gap-2">
              <Badge variant={mod.game === "KOTOR1" ? "info" : "secondary"}>{mod.game}</Badge>
              {mod.install_method_hint && <Badge variant="muted">{mod.install_method_hint}</Badge>}
              {mod.source_label && (
                <Badge
                  variant={mod.auto_downloadable === false ? "warning" : "muted"}
                  title={
                    mod.auto_downloadable === false
                      ? t("builds.manualDownloadHint", { source: mod.source_label ?? "" })
                      : `Downloaded automatically from ${mod.source_label}`
                  }
                >
                  {mod.source_label}
                  {mod.auto_downloadable === false && mod.source_host !== "nexus" && " · manual"}
                </Badge>
              )}
              {info?.author && <span className="text-xs text-muted-foreground">{info.author}</span>}
            </div>
          </div>
          <button onClick={onClose} aria-label={t("common.close")} className="shrink-0 rounded-sm p-1 text-muted-foreground transition-colors hover:text-foreground">
            <X className="size-4" />
          </button>
        </div>

        {/* Body */}
        <div className="min-h-0 flex-1 space-y-5 overflow-auto p-4">
          {mod.source_host === "gamefront" && (
            <p className="text-sm text-muted-foreground">
              GameFront downloads open in your browser. Save the archive to your
              configured downloads folder; the manager then continues automatically.
            </p>
          )}
          {(mod.patch_urls?.length ?? 0) > 0 && (
            <p className="text-sm text-muted-foreground">
              The linked patches are downloaded with the main mod.
            </p>
          )}
          {/* Why this mod failed to install (when it did). */}
          {error && (
            <section className="rounded-md border border-destructive/40 bg-destructive/10 p-3">
              <h3 className="mb-1 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-destructive">
                <AlertTriangle className="size-3.5" /> {t("modDetail.installError")}
              </h3>
              <p className="whitespace-pre-wrap break-words text-sm leading-relaxed text-foreground">{error}</p>
            </section>
          )}

          {/* What the installer will do automatically for this mod */}
          {summary && (
            <section className="rounded-md border border-[hsl(var(--info)/0.4)] bg-[hsl(var(--info)/0.08)] p-3">
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-[hsl(var(--info))]">
                {t("modDetail.autoHandling")}
              </h3>
              <p className="text-sm leading-relaxed text-foreground">
                {t("modDetail.autoHandlingPrefix")} {summary}.
              </p>
            </section>
          )}

          {/* Install instructions straight from the build guide */}
          {instructions && (
            <section>
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                {t("modDetail.guideInstructions")}
              </h3>
              <p className="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">{instructions}</p>
            </section>
          )}

          {/* Build note */}
          {mod.warnings?.trim() && (
            <section className="border-l-2 border-warning pl-3">
              <h3 className="mb-1 text-sm font-semibold text-warning">{t("modDetail.guideWarnings")}</h3>
              <p className="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">{mod.warnings}</p>
            </section>
          )}
          {note && !instructions && !description && !mod.warnings?.trim() && (
            <section className="rounded-md border border-[hsl(var(--warning)/0.4)] bg-[hsl(var(--warning)/0.08)] p-3">
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-[hsl(var(--warning))]">
                {t("modDetail.buildNote")}
              </h3>
              <p className="whitespace-pre-wrap text-sm leading-relaxed text-foreground">{note}</p>
            </section>
          )}

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
          {links.some((l) => l.url) && (
            <section className="flex flex-wrap gap-2">
              {links.filter((l) => l.url).map((l) => (
                <Button key={l.label} variant="outline" size="sm"
                        onClick={() => api.openUrl(l.url!).catch(() => {})}>
                  <ExternalLink /> {l.label}
                </Button>
              ))}
            </section>
          )}
        </div>
      </aside>
    </div>
  );
}
