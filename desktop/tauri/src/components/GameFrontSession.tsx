import { useEffect, useState } from "react";
import { CheckCircle2, LogIn, LogOut } from "lucide-react";
import { gameFrontSignedIn, isTauri, openGameFront, signOutGameFront } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";

interface GameFrontSessionProps {
  addLog: (message: string, tag?: string) => void;
}

/** GameFront account card, laid out like the DeadlyStream one. */
export function GameFrontSession({ addLog }: GameFrontSessionProps) {
  const t = useT();
  const [signedIn, setSignedIn] = useState(false);
  const [busy, setBusy] = useState(false);
  const desktop = isTauri();

  useEffect(() => {
    if (!desktop) return;
    let stopped = false;
    let unlisten: (() => void) | undefined;
    gameFrontSignedIn().then((v) => { if (!stopped) setSignedIn(v); }).catch(() => {});
    // The GameFront window reports sign-ins as they happen.
    import("@tauri-apps/api/event").then(async ({ listen }) => {
      const off = await listen<boolean>("gamefront-account", (e) => setSignedIn(e.payload));
      if (stopped) off(); else unlisten = off;
    }).catch(() => {});
    return () => { stopped = true; unlisten?.(); };
  }, [desktop]);

  const signIn = async () => {
    setBusy(true);
    try {
      await openGameFront(null);
    } catch (e) {
      addLog(t("settings.gamefront.openFailed", { error: e instanceof Error ? e.message : String(e) }), "error");
    } finally {
      setBusy(false);
    }
  };

  const signOut = async () => {
    setBusy(true);
    try {
      await signOutGameFront();
      setSignedIn(false);
      addLog(t("settings.gamefront.signedOut"), "success");
    } catch (e) {
      addLog(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader><CardTitle>GameFront</CardTitle></CardHeader>
      <CardContent>
        {signedIn ? (
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-1.5 text-sm font-medium text-foreground">
                {t("settings.account.signedInName")}
                <CheckCircle2 className="size-4 text-[hsl(var(--success))]" />
              </p>
              <p className="text-xs text-muted-foreground">{t("settings.gamefront.signedInHint")}</p>
            </div>
            <Button variant="outline" size="sm" disabled={busy} onClick={signOut}>
              <LogOut /> {t("common.signOut")}
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">{t("settings.account.notSignedIn")}</p>
              <p className="text-xs text-muted-foreground">
                {desktop ? t("settings.gamefront.notSignedInHint") : t("settings.gamefront.desktopOnly")}
              </p>
            </div>
            <Button variant="default" size="sm" disabled={busy || !desktop} onClick={signIn}>
              <LogIn /> {t("common.signIn")}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
