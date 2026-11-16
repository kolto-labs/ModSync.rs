import { useState } from "react";
import { Loader2 } from "lucide-react";
import { api, type BuildInfo } from "@/lib/api";
import { Dialog } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";

interface AddBuildDialogProps {
  open: boolean;
  onClose: () => void;
  onAdded: (build: BuildInfo) => void;
  addLog: (message: string, tag?: string) => void;
}

/**
 * Add a custom mod build from a guide URL. The build is scraped just like the
 * built-in ones, so any neocities-style build page can be plugged in.
 */
export function AddBuildDialog({ open, onClose, onAdded, addLog }: AddBuildDialogProps) {
  const t = useT();
  const [label, setLabel] = useState("");
  const [game, setGame] = useState("KOTOR1");
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);

  const reset = () => { setLabel(""); setGame("KOTOR1"); setUrl(""); setBusy(false); };
  const close = () => { reset(); onClose(); };

  const submit = async () => {
    const u = url.trim();
    if (!u) { addLog(t("builds.addBuildNoUrl"), "warning"); return; }
    setBusy(true);
    try {
      const r = await api.addBuild(label.trim() || u, game, u);
      addLog(t("builds.addBuildAdded", { label: r.build.label }), "success");
      onAdded(r.build);
      close();
    } catch (e: any) {
      addLog(t("builds.addBuildFailed", { error: e?.message ?? "error" }), "error");
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onClose={close} title={t("builds.addBuildTitle")}>
      <div className="space-y-3">
        <p className="text-sm text-muted-foreground">{t("builds.addBuildHint")}</p>
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">{t("builds.addBuildLabel")}</label>
          <Input
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={t("builds.addBuildLabelPlaceholder")}
          />
        </div>
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">{t("builds.addBuildGame")}</label>
          <Select value={game} onChange={(e) => setGame(e.target.value)}>
            <option value="KOTOR1">KOTOR 1</option>
            <option value="KOTOR2">KOTOR 2</option>
          </Select>
        </div>
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">{t("builds.addBuildUrl")}</label>
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://kotor.neocities.org/..."
            onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
          />
        </div>
        <div className="flex justify-end gap-2 pt-1">
          <Button variant="ghost" onClick={close} disabled={busy}>{t("common.cancel")}</Button>
          <Button onClick={submit} disabled={busy}>
            {busy && <Loader2 className="animate-spin" />}
            {t("builds.addBuildSubmit")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
