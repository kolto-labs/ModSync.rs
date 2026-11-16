import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useT } from "@/lib/i18n";

/** The DeadlyStream sign-in fields, shared by the sign-in dialog and the first-run setup. */
export function LoginForm({
  onLoggedIn,
  autoFocus = true,
}: {
  onLoggedIn: (username: string) => void;
  autoFocus?: boolean;
}) {
  const t = useT();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [save, setSave] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  // The backend may still be starting when this mounts, so guess from the
  // webview first and confirm with the backend.
  const [isWindows, setIsWindows] = useState(() => /Windows/i.test(navigator.userAgent));

  useEffect(() => {
    api.health().then((h) => setIsWindows(h.platform === "windows")).catch(() => {});
    api.credentials().then((c) => c.username && setUsername(c.username)).catch(() => {});
  }, []);

  const submit = async () => {
    setError("");
    setBusy(true);
    try {
      const r = await api.login(username.trim(), password, save);
      onLoggedIn(r.username);
    } catch (e: any) {
      setError(e?.message || t("login.failed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="space-y-1.5">
        <Label htmlFor="u">{t("login.username")}</Label>
        <Input id="u" value={username} onChange={(e) => setUsername(e.target.value)} autoFocus={autoFocus} />
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="p">{t("login.password")}</Label>
        <Input
          id="p"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
      </div>
      <div className="flex items-center gap-2 pt-1">
        <Switch id="save" checked={save} onCheckedChange={setSave} />
        <Label htmlFor="save" className="cursor-pointer">{t("login.saveCredentials", { store: t(isWindows ? "login.store.windows" : "login.store.other") })}</Label>
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      <Button className="w-full" onClick={submit} disabled={busy || !username || !password}>
        {busy ? t("login.submitting") : t("login.submit")}
      </Button>
    </div>
  );
}

export function LoginDialog({
  open,
  onClose,
  onLoggedIn,
}: {
  open: boolean;
  onClose: () => void;
  onLoggedIn: (username: string) => void;
}) {
  const t = useT();
  return (
    <Dialog open={open} onClose={onClose} title={t("login.title")}>
      <LoginForm onLoggedIn={(u) => { onLoggedIn(u); onClose(); }} />
    </Dialog>
  );
}
