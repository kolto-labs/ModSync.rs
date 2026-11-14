import { useEffect, useState } from "react";
import { FolderOpen } from "lucide-react";
import { api } from "@/lib/api";
import { pickFile } from "@/lib/tauri";
import { useT } from "@/lib/i18n";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function ArchiveSection({ addLog }: { addLog: (message: string, tag?: string) => void }) {
  const t = useT();
  const [path, setPath] = useState("");
  const [resolved, setResolved] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    Promise.all([api.getSettings(), api.extractorStatus()])
      .then(([settings, status]) => {
        setPath(settings.seven_zip_path ?? "");
        setResolved(status.seven_zip_path);
      })
      .catch(() => addLog(t("settings.archive.loadFailed"), "error"))
      .finally(() => setLoading(false));
  }, []);

  const persist = async (value: string) => {
    setSaving(true);
    setSaved(false);
    try {
      const settings = await api.getSettings();
      await api.setSettings({ ...settings, seven_zip_path: value });
      const status = await api.extractorStatus();
      setPath(value);
      setResolved(status.seven_zip_path);
      setSaved(true);
    } catch {
      addLog(t("settings.archive.saveFailed"), "error");
    } finally {
      setSaving(false);
    }
  };

  const browse = async () => {
    const selected = await pickFile([]);
    if (selected) { setPath(selected); setSaved(false); }
  };

  return (
    <Card>
      <CardHeader><CardTitle>{t("settings.archive.title")}</CardTitle></CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">{t("settings.archive.hint")}</p>
        <div className="space-y-1.5">
          <Label htmlFor="seven-zip-path">{t("settings.archive.path")}</Label>
          <div className="flex gap-2">
            <Input id="seven-zip-path" value={path} disabled={loading || saving}
              placeholder={t("settings.archive.placeholder")}
              onChange={(e) => { setPath(e.target.value); setSaved(false); }} />
            <Button variant="outline" size="icon" disabled={loading || saving}
              title={t("common.browse")} aria-label={t("common.browse")} onClick={browse}>
              <FolderOpen />
            </Button>
          </div>
        </div>
        {!loading && (
          <p className="break-all text-xs text-muted-foreground" role="status">
            {resolved ? t("settings.archive.detected", { path: resolved }) : t("settings.archive.notFound")}
          </p>
        )}
        <p className="text-xs text-muted-foreground">{t("settings.archive.fallbackHint")}</p>
        <div className="flex items-center gap-3">
          <Button size="sm" disabled={loading || saving} onClick={() => persist(path.trim())}>
            {saving ? t("common.saving") : t("settings.archive.save")}
          </Button>
          <Button size="sm" variant="ghost" disabled={loading || saving || !path}
            onClick={() => persist("")}>{t("common.clear")}</Button>
          {saved && <span className="text-xs text-[hsl(var(--success))]">{t("common.saved")}</span>}
        </div>
      </CardContent>
    </Card>
  );
}
