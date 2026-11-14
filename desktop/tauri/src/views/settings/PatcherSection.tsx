import { useCallback, useEffect, useState } from "react";
import { Zap, AlertTriangle, FolderOpen, Loader2 } from "lucide-react";
import { api, type AppStatus, type PatcherStatus } from "@/lib/api";
import { pickFile } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { useT } from "@/lib/i18n";

interface PatcherSectionProps {
  status: AppStatus | null;
  addLog: (message: string, tag?: string) => void;
}

const SOURCE_LABEL: Record<PatcherStatus["source"], string> = {
  bundled: "settings.patcher.sourceBundled",
  custom: "settings.patcher.sourceCustom",
  system: "settings.patcher.sourceSystem",
  none: "settings.patcher.sourceNone",
};

export function PatcherSection({ addLog }: PatcherSectionProps) {
  const t = useT();
  const [ps, setPs] = useState<PatcherStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [customPath, setCustomPath] = useState("");
  const [saving, setSaving] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const r = await api.patcherStatus();
      setPs(r);
      setCustomPath(r.custom_patcher_path ?? "");
    } catch (e: any) {
      addLog(`Failed to load patcher status: ${e?.message}`, "error");
    } finally {
      setLoading(false);
    }
  }, [addLog]);

  useEffect(() => { refresh(); }, [refresh]);

  const persist = async (path: string) => {
    setSaving(true);
    try {
      const settings = await api.getSettings();
      await api.setSettings({ ...settings, custom_patcher_path: path });
      await refresh();
      addLog(path ? "Custom patcher saved." : "Custom patcher cleared.", "success");
    } catch (e: any) {
      addLog(`Failed to save custom patcher: ${e?.message}`, "error");
    } finally {
      setSaving(false);
    }
  };

  const browse = async () => {
    const f = await pickFile(["exe"]);
    if (f) setCustomPath(f);
  };

  const available = !!ps?.available;
  const strategies = ps?.strategies ?? [];

  return (
    <>
      <Card>
        <CardHeader><CardTitle>{t("settings.patcher.title")}</CardTitle></CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">{t("settings.patcher.description")}</p>

          {loading ? (
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" /> {t("settings.patcher.loading")}
            </p>
          ) : (
            <>
              {/* Engine status */}
              <div className="flex flex-wrap items-center gap-2">
                {available ? (
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-[hsl(var(--success)/0.15)] px-2.5 py-0.5 text-xs font-medium text-[hsl(var(--success))]">
                    <Zap className="size-3.5" /> {t("settings.patcher.ready")}
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-[hsl(var(--warning)/0.15)] px-2.5 py-0.5 text-xs font-medium text-[hsl(var(--warning))]">
                    <AlertTriangle className="size-3.5" /> {t("settings.patcher.notAvailable")}
                  </span>
                )}
                {ps && (
                  <Badge variant="muted">
                    {t("settings.patcher.sourceLabel")}: {t(SOURCE_LABEL[ps.source])}
                  </Badge>
                )}
              </div>

              {/* Resolved path */}
              {ps?.path && (
                <div className="space-y-1">
                  <Label className="text-xs text-muted-foreground">{t("settings.patcher.resolvedPath")}</Label>
                  <p className="truncate font-mono text-xs text-muted-foreground" title={ps.path}>
                    {ps.path}
                  </p>
                </div>
              )}

              {/* Strategy cascade */}
              {strategies.length > 0 && (
                <div className="space-y-1.5">
                  <Label className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    {t("settings.patcher.strategiesTitle")}
                  </Label>
                  <p className="text-xs text-muted-foreground">{t("settings.patcher.strategiesHint")}</p>
                  <ol className="space-y-1">
                    {strategies.map((s, i) => (
                      <li key={s} className="flex items-center gap-2 text-sm">
                        <span className="inline-flex size-5 shrink-0 items-center justify-center rounded-full bg-muted text-[11px] font-mono text-muted-foreground">
                          {i + 1}
                        </span>
                        <span className="font-mono text-xs">{s}</span>
                      </li>
                    ))}
                  </ol>
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle>{t("settings.patcher.customTitle")}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-muted-foreground">{t("settings.patcher.customHint")}</p>
          <div className="space-y-1.5">
            <Label>{t("settings.patcher.customLabel")}</Label>
            <div className="flex gap-2">
              <Input
                value={customPath}
                placeholder={t("settings.patcher.customPlaceholder")}
                onChange={(e) => setCustomPath(e.target.value)}
              />
              <Button variant="outline" size="icon" title={t("common.browse")} onClick={browse}>
                <FolderOpen />
              </Button>
            </div>
          </div>
          <div className="flex gap-2">
            <Button size="sm" disabled={saving} onClick={() => persist(customPath.trim())}>
              {saving ? t("common.saving") : t("common.save")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={saving || !customPath}
              onClick={() => { setCustomPath(""); persist(""); }}
            >
              {t("common.clear")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </>
  );
}
