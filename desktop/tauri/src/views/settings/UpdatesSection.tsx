import { useEffect, useState } from "react";
import { Download, RefreshCw, ExternalLink } from "lucide-react";
import { api, connectEvents, type AppStatus, type UpdateInfo } from "@/lib/api";
import { applyUpdate, isTauri } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useT } from "@/lib/i18n";

interface UpdatesSectionProps {
  status: AppStatus | null;
  addLog: (message: string, tag?: string) => void;
}

export function UpdatesSection({ status, addLog }: UpdatesSectionProps) {
  const t = useT();
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    api.updateCheck().then(setUpdate).catch(() => {});
  }, []);

  const installUpdate = async () => {
    setInstalling(true);
    setProgress(0);
    // Watch download progress over the WS while the backend fetches the exe.
    const disconnect = connectEvents((e) => {
      if (e.type === "update_progress") setProgress(e.pct);
    });
    try {
      const r = await api.updateDownload();
      disconnect();
      if (r.ok && r.path && isTauri()) {
        addLog(`Update v${r.version} downloaded - restarting to apply…`, "success");
        const ok = await applyUpdate(r.path);
        if (!ok) {
          addLog("Could not apply the update automatically; opening the release page.", "warning");
          api.updateOpen(update?.url ?? undefined).catch(() => {});
        }
        // On success the app exits and relaunches; nothing more to do here.
      } else if (r.ok && !isTauri()) {
        addLog("Update downloaded. Open the release page to install manually.", "info");
        api.updateOpen(update?.url ?? undefined).catch(() => {});
      }
    } catch (e: any) {
      disconnect();
      addLog(`Update failed: ${e?.message}`, "error");
    } finally {
      setInstalling(false);
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    try {
      const info = await api.updateCheck();
      setUpdate(info);
      if (info.available) addLog(`Update available: v${info.latest_version}`, "info");
      else if (!info.error) addLog("You're on the latest version.", "muted");
    } catch (e: any) {
      addLog(`Update check failed: ${e?.message}`, "error");
    } finally {
      setChecking(false);
    }
  };

  return (
    <Card>
      <CardHeader><CardTitle>{t("settings.updates.title")}</CardTitle></CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-3">
          <p className="text-sm text-muted-foreground">
            {t("settings.updates.appName")}{" "}
            <span className="font-mono text-foreground">
              v{update?.current_version ?? status?.version ?? "?"}
            </span>
          </p>
          <Button variant="outline" size="sm" className="ml-auto"
                  onClick={checkUpdates} disabled={checking}>
            <RefreshCw className={checking ? "animate-spin" : ""} />
            {checking ? t("settings.updates.checking") : t("settings.updates.check")}
          </Button>
        </div>
        {update?.available ? (
          <div className="rounded-md border border-[hsl(var(--info)/0.4)] bg-[hsl(var(--info)/0.1)] p-3">
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium text-[hsl(var(--info))]">
                {t("settings.updates.available", { version: update.latest_version ?? "" })}
              </span>
              <div className="ml-auto flex items-center gap-2">
                <Button variant="ghost" size="sm"
                        onClick={() => api.updateOpen(update.url ?? undefined).catch(() => {})}>
                  <ExternalLink /> {t("settings.updates.releaseNotes")}
                </Button>
                {update.can_self_update === false ? (
                  <Button size="sm"
                          onClick={() => api.updateOpen(update.nexus_url ?? update.url ?? undefined).catch(() => {})}>
                    <ExternalLink /> {t("settings.updates.getIt")}
                  </Button>
                ) : (
                  <Button size="sm" onClick={installUpdate} disabled={installing}>
                    <Download /> {installing ? t("settings.updates.installing") : t("settings.updates.install")}
                  </Button>
                )}
              </div>
            </div>
            {update.can_self_update === false && (
              <p className="mt-2 text-xs text-muted-foreground">{t("settings.updates.manual")}</p>
            )}
            {installing && (
              <div className="mt-2">
                <Progress value={progress} />
                <p className="mt-1 text-xs text-muted-foreground">{t("settings.updates.downloading", { pct: progress })}</p>
              </div>
            )}
            {update.notes && !installing && (
              <pre className="mt-2 max-h-32 overflow-auto whitespace-pre-wrap font-sans text-xs text-muted-foreground">
                {update.notes}
              </pre>
            )}
          </div>
        ) : update && !update.error ? (
          <p className="text-xs text-[hsl(var(--success))]">{t("settings.updates.upToDate")}</p>
        ) : null}
      </CardContent>
    </Card>
  );
}
