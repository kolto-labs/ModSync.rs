import { useEffect, useState } from "react";
import { X, ExternalLink, Loader2, AlertTriangle, CheckCircle2 } from "lucide-react";
import { api, type BuildMod, type DraftSteps, type ModInfo, type ModOption, type Unavailable } from "@/lib/api";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Screenshots } from "@/components/Screenshots";
import { useT } from "@/lib/i18n";

interface BuildModDetailProps {
  mod: BuildMod;
  onClose: () => void;
  error?: string;
  /** Mods and options that can't be selected, and why. */
  unavailable?: Map<string, Unavailable>;
}

// Lightweight detail drawer for a build mod (no installed-file data - that's
// only available once a mod is in the library).
export function BuildModDetail({ mod, onClose, error, unavailable }: BuildModDetailProps) {
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

  const [options, setOptions] = useState<ModOption[]>(mod.options ?? []);
  useEffect(() => { setOptions(mod.options ?? []); }, [mod]);
  const [optionError, setOptionError] = useState<string | null>(null);
  const toggleOption = (o: ModOption, selected: boolean) => {
    setOptionError(null);
    api.setOption(mod.build_key, o.guid, selected)
      .then((r) => setOptions(r.options))
      .catch((e: any) => setOptionError(e?.message ?? null));
  };
  const blocked = unavailable?.get(mod.file_id);

  // Steps the app drafted for this mod. Asked for again on open, because a
  // check can draft steps from the downloads after the list was loaded.
  const [draft, setDraft] = useState<DraftSteps | null>(mod.draft ?? null);
  const [draftError, setDraftError] = useState("");
  const [approving, setApproving] = useState(false);
  useEffect(() => {
    let alive = true;
    setDraft(mod.draft ?? null);
    setDraftError("");
    api.draft(mod.build_key, mod.file_id)
      .then((r) => { if (alive) setDraft(r.draft); })
      .catch(() => {});
    return () => { alive = false; };
  }, [mod]);
  const approve = () => {
    setApproving(true);
    setDraftError("");
    api.approveDraft(mod.build_key, mod.file_id)
      .then((r) => setDraft(r.draft))
      .catch((e) => setDraftError(e instanceof Error ? e.message : String(e)))
      .finally(() => setApproving(false));
  };

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

          {/* Why this mod can't be selected, when it can't */}
          {blocked && (
            <section className="rounded-md border border-[hsl(var(--warning)/0.4)] bg-[hsl(var(--warning)/0.08)] p-3">
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-[hsl(var(--warning))]">
                {t(`builds.unavailable.${blocked.category}`)}
              </h3>
              <p className="break-words text-sm leading-relaxed text-foreground">{blocked.reason}</p>
            </section>
          )}

          {/* Steps the app drafted, which the player checks and approves */}
          {draft && <DraftReview draft={draft} approving={approving} error={draftError} onApprove={approve} />}

          {/* Install instructions straight from the build guide */}
          {/* Choices inside the mod */}
          {options.length > 0 && (
            <section>
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                {t("modDetail.options")}
              </h3>
              <OptionList options={options} onToggle={toggleOption} unavailable={unavailable} />
              {optionError && <p className="mt-2 text-sm text-destructive">{optionError}</p>}
            </section>
          )}

          {(mod.dependencies?.length ?? 0) > 0 && (
            <section>
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                {t("modDetail.requires")}
              </h3>
              <ul className="list-disc space-y-0.5 pl-5 text-sm text-muted-foreground">
                {mod.dependencies!.map((n) => <li key={n}>{n}</li>)}
              </ul>
            </section>
          )}

          {(mod.restrictions?.length ?? 0) > 0 && (
            <section className="border-l-2 border-warning pl-3">
              <h3 className="mb-1 text-sm font-semibold text-warning">{t("modDetail.cannotCombine")}</h3>
              <ul className="list-disc space-y-0.5 pl-5 text-sm text-muted-foreground">
                {mod.restrictions!.map((n) => <li key={n}>{n}</li>)}
              </ul>
            </section>
          )}

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

function DraftReview({ draft, approving, error, onApprove }: {
  draft: DraftSteps; approving: boolean; error: string; onApprove: () => void;
}) {
  const t = useT();
  const open = draft.needs_review;
  if (draft.waiting_for_download) {
    return (
      <section className="rounded-md border border-[hsl(var(--info)/0.4)] bg-[hsl(var(--info)/0.08)] p-3">
        <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-[hsl(var(--info))]">
          {t("modDetail.draftWaitingTitle")}
        </h3>
        <p className="text-sm leading-relaxed text-foreground">{t("modDetail.draftWaitingExplain")}</p>
        {draft.steps.length > 0 && (
          <ol className="mt-2 list-decimal space-y-0.5 pl-5 font-mono text-xs text-muted-foreground">
            {draft.steps.map((s, i) => <li key={i} className="break-words">{s}</li>)}
          </ol>
        )}
      </section>
    );
  }
  return (
    <section className={open
      ? "rounded-md border border-[hsl(var(--warning)/0.4)] bg-[hsl(var(--warning)/0.08)] p-3"
      : "rounded-md border p-3"}>
      <h3 className={`mb-1 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide ${open ? "text-[hsl(var(--warning))]" : "text-muted-foreground"}`}>
        {open ? <AlertTriangle className="size-3.5" /> : <CheckCircle2 className="size-3.5" />}
        {open ? t("modDetail.draftTitle") : draft.confirmed ? t("modDetail.draftConfirmedTitle") : t("modDetail.draftApprovedTitle")}
      </h3>
      <p className="text-sm leading-relaxed text-foreground">
        {open ? t("modDetail.draftExplain") : draft.confirmed ? t("modDetail.draftConfirmedExplain") : t("modDetail.draftApprovedExplain")}
      </p>
      {draft.steps.length > 0 ? (
        <ol className="mt-2 list-decimal space-y-0.5 pl-5 font-mono text-xs text-muted-foreground">
          {draft.steps.map((s, i) => <li key={i} className="break-words">{s}</li>)}
        </ol>
      ) : (
        <p className="mt-2 text-sm italic text-muted-foreground">{t("modDetail.draftNoSteps")}</p>
      )}
      {(draft.unclear.length > 0 || draft.rejected > 0) && (
        <div className="mt-2">
          <p className="text-sm font-medium text-foreground">{t("modDetail.draftUnclear")}</p>
          <ul className="list-disc space-y-0.5 pl-5 text-xs text-muted-foreground">
            {draft.unclear.map((u, i) => <li key={i} className="break-words">{u}</li>)}
            {draft.rejected > 0 && <li>{t("modDetail.draftRejected", { count: String(draft.rejected) })}</li>}
          </ul>
        </div>
      )}
      {open && draft.can_approve && (
        <Button className="mt-3" size="sm" variant="outline" disabled={approving} onClick={onApprove}>
          {approving ? <Loader2 className="animate-spin" /> : <CheckCircle2 />} {t("modDetail.draftApprove")}
        </Button>
      )}
      {open && !draft.can_approve && (
        <p className="mt-2 text-sm text-muted-foreground">{t("modDetail.draftCannotApprove")}</p>
      )}
      {error && <p className="mt-2 text-sm text-destructive">{error}</p>}
    </section>
  );
}

function OptionList({ options, onToggle, unavailable }: {
  options: ModOption[];
  onToggle: (o: ModOption, selected: boolean) => void;
  unavailable?: Map<string, Unavailable>;
}) {
  const t = useT();
  return (
    <ul className="space-y-2">
      {options.map((o) => {
        // An option that can't be installed can be turned off but not on.
        const blocked = unavailable?.get(o.guid);
        return (
          <li key={o.guid} className="space-y-1">
            <label className={`flex items-start gap-2 text-sm${blocked && !o.selected ? " opacity-60" : ""}`}>
              <Checkbox
                checked={o.selected}
                disabled={!!blocked && !o.selected}
                onCheckedChange={(v) => onToggle(o, v)}
                aria-label={o.name}
                className="mt-0.5"
              />
              <span className="min-w-0">
                <span className="text-foreground">{o.name}</span>
                {o.description && <span className="block text-xs text-muted-foreground">{o.description}</span>}
                {blocked && (
                  <span className="block break-words text-xs text-[hsl(var(--warning))]" title={blocked.reason}>
                    {t(`builds.unavailable.${blocked.category}`)}: {blocked.reason}
                  </span>
                )}
              </span>
            </label>
            {o.options.length > 0 && (
              <div className="pl-6"><OptionList options={o.options} onToggle={onToggle} unavailable={unavailable} /></div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
