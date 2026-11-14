import { useEffect, useState } from "react";
import { CheckCircle2, LogIn, LogOut, ExternalLink } from "lucide-react";
import { api, type AppStatus } from "@/lib/api";
import { NexusKeyForm } from "@/components/NexusKeyForm";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Avatar } from "@/components/ui/avatar";
import { useT } from "@/lib/i18n";
import { GameFrontSession } from "@/components/GameFrontSession";

interface AccountSectionProps {
  status: AppStatus | null;
  username: string;
  onSignIn: () => void;
  onSignOut: () => void;
  addLog: (message: string, tag?: string) => void;
}

export function AccountSection({ status, username, onSignIn, onSignOut, addLog }: AccountSectionProps) {
  const t = useT();

  // Mod build source site (where build guides are scraped from).
  const [srcUrl, setSrcUrl] = useState("");
  const [srcUser, setSrcUser] = useState("");
  const [srcPass, setSrcPass] = useState("");
  const [srcHasPass, setSrcHasPass] = useState(false);
  const [srcBusy, setSrcBusy] = useState(false);

  useEffect(() => {
    api.sourceSite().then((s) => {
      setSrcUrl(s.url || "");
      setSrcUser(s.username || "");
      setSrcHasPass(s.has_password);
    }).catch(() => {});
  }, []);

  const saveSource = async () => {
    setSrcBusy(true);
    try {
      const r = await api.saveSourceSite({
        url: srcUrl.trim(),
        username: srcUser.trim(),
        password: srcPass,
      });
      setSrcHasPass(r.has_password);
      setSrcPass("");
      addLog(t("settings.source.saved"), "success");
    } catch (e: any) {
      addLog(t("settings.source.saveFailed", { error: e?.message ?? "error" }), "error");
    } finally {
      setSrcBusy(false);
    }
  };

  return (
    <div className="space-y-4">
      {/* DeadlyStream account */}
      <Card>
        <CardHeader><CardTitle>{t("settings.account.title")}</CardTitle></CardHeader>
        <CardContent>
          {status?.logged_in ? (
            <div className="flex items-center gap-3">
              <Avatar name={username} />
              <div className="min-w-0 flex-1">
                <p className="flex items-center gap-1.5 text-sm font-medium text-foreground">
                  {username || t("settings.account.signedInName")}
                  <CheckCircle2 className="size-4 text-[hsl(var(--success))]" />
                </p>
                <p className="text-xs text-muted-foreground">
                  {t("settings.account.signedInHint")}
                </p>
              </div>
              <Button variant="outline" size="sm" onClick={onSignOut}>
                <LogOut /> {t("common.signOut")}
              </Button>
            </div>
          ) : (
            <div className="flex items-center gap-3">
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium text-foreground">{t("settings.account.notSignedIn")}</p>
                <p className="text-xs text-muted-foreground">
                  {t("settings.account.notSignedInHint")}
                </p>
              </div>
              <Button variant="default" size="sm" onClick={onSignIn}>
                <LogIn /> {t("common.signIn")}
              </Button>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Nexus Mods API key */}
      <Card>
        <CardHeader><CardTitle>{t("settings.nexus.title")}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <p className="text-xs text-muted-foreground">
            {t("settings.nexus.hint")}{" "}
            <button
              className="inline-flex items-center gap-0.5 text-[hsl(var(--info))] hover:underline"
              onClick={() => api.openUrl("https://www.nexusmods.com/users/myaccount?tab=api").catch(() => {})}
            >
              {t("settings.nexus.getKey")} <ExternalLink className="size-3" />
            </button>
          </p>
          <NexusKeyForm addLog={addLog} />
        </CardContent>
      </Card>

      <GameFrontSession addLog={addLog} />

      {/* Mod build source site (scraping) */}
      <Card>
        <CardHeader><CardTitle>{t("settings.source.title")}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <p className="text-xs text-muted-foreground">{t("settings.source.hint")}</p>
          <div className="space-y-1.5">
            <Label htmlFor="srcurl">{t("settings.source.url")}</Label>
            <Input
              id="srcurl"
              value={srcUrl}
              placeholder="https://kotor.neocities.org"
              onChange={(e) => setSrcUrl(e.target.value)}
            />
          </div>
          <div className="grid grid-cols-2 gap-2">
            <div className="space-y-1.5">
              <Label htmlFor="srcuser">{t("settings.source.username")}</Label>
              <Input
                id="srcuser"
                value={srcUser}
                placeholder={t("settings.source.optional")}
                onChange={(e) => setSrcUser(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="srcpass">{t("settings.source.password")}</Label>
              <Input
                id="srcpass"
                type="password"
                value={srcPass}
                placeholder={srcHasPass ? t("settings.source.passwordSaved") : t("settings.source.optional")}
                onChange={(e) => setSrcPass(e.target.value)}
              />
            </div>
          </div>
          <div className="flex items-center justify-between">
            <p className="text-xs text-muted-foreground">
              {srcHasPass ? t("settings.source.hasPassword") : t("settings.source.noLoginNeeded")}
            </p>
            <Button onClick={saveSource} disabled={srcBusy}>
              {srcBusy ? t("settings.nexus.saving") : t("common.save")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
