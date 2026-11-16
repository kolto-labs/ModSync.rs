import { useEffect, useState } from "react";
import { CheckCircle2, AlertTriangle, Eye, EyeOff } from "lucide-react";
import { api, type NexusValidation } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useT } from "@/lib/i18n";

/** Paste, save and check a Nexus API key. Shared by Settings and the first-run setup. */
export function NexusKeyForm({
  addLog,
  onValidated,
}: {
  addLog: (message: string, tag?: string) => void;
  onValidated?: (v: NexusValidation) => void;
}) {
  const t = useT();
  const [nexusKey, setNexusKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [validation, setValidation] = useState<NexusValidation | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.getSettings().then((s) => {
      setNexusKey(s.nexus_api_key || "");
      if (s.nexus_api_key) api.nexusValidate().then(setValidation).catch(() => {});
    }).catch(() => {}).finally(() => setLoading(false));
  }, []);

  const saveNexus = async () => {
    setBusy(true);
    setValidation(null);
    try {
      const latest = await api.getSettings();
      await api.setSettings({ ...latest, nexus_api_key: nexusKey.trim() });
      const v = await api.nexusValidate();
      setValidation(v);
      onValidated?.(v);
      addLog(v.ok ? `Nexus key valid (${v.name}).` : `Nexus key invalid.`, v.ok ? "success" : "error");
    } catch (e: any) {
      addLog(`Failed to save Nexus key: ${e?.message}`, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="space-y-1.5">
        <Label htmlFor="nexuskey">{t("settings.nexus.label")}</Label>
        <div className="flex gap-2">
          <div className="relative flex-1">
            <Input
              id="nexuskey"
              disabled={loading}
              type={showKey ? "text" : "password"}
              value={nexusKey}
              placeholder={t("settings.nexus.placeholder")}
              onChange={(e) => { setValidation(null); setNexusKey(e.target.value); }}
              onKeyDown={(e) => e.key === "Enter" && nexusKey.trim() && saveNexus()}
              className="pr-9"
            />
            <button
              type="button"
              onClick={() => setShowKey((v) => !v)}
              className="absolute inset-y-0 right-0 flex items-center px-2.5 text-muted-foreground hover:text-foreground"
              aria-label={showKey ? t("settings.nexus.hideKey") : t("settings.nexus.showKey")}
              title={showKey ? t("settings.nexus.hideKey") : t("settings.nexus.showKey")}
            >
              {showKey ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
            </button>
          </div>
          <Button onClick={saveNexus} disabled={busy || loading} aria-label={`${t("common.save")} ${t("settings.nexus.label")}`}>
            {busy ? t("settings.nexus.saving") : t("common.save")}
          </Button>
        </div>
      </div>
      {validation?.ok && validation.limits?.daily_remaining !== undefined && (
        <p className="text-xs text-muted-foreground">
          {t("settings.nexus.allowance", {
            daily: validation.limits.daily_remaining,
            hourly: validation.limits.hourly_remaining ?? "?",
          })}
        </p>
      )}
      {validation && (
        validation.ok ? (
          <p className="flex items-center gap-1.5 text-xs text-[hsl(var(--success))]">
            <CheckCircle2 className="size-3.5" />
            {t("settings.nexus.valid", { name: validation.name ?? "" })}
            {validation.is_premium ? " · Premium" : ""}
          </p>
        ) : (
          <p className="flex items-center gap-1.5 text-xs text-[hsl(var(--warning))]">
            <AlertTriangle className="size-3.5" /> {t("settings.nexus.invalid")}
          </p>
        )
      )}
    </div>
  );
}
