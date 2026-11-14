import { useEffect, useState } from "react";
import {
  FolderOpen, Plus, Pencil, Trash2, Check, X, CheckCircle2, Star,
} from "lucide-react";
import { api, type GameKey, type Profile } from "@/lib/api";
import { pickDirectory } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import { useConfirm } from "@/components/useConfirm";
import { useT } from "@/lib/i18n";

interface GameInstallsSectionProps {
  profiles: Profile[];
  activeProfile: string;
  setActiveProfile: (id: string) => void;
  refreshProfiles: () => Promise<void> | void;
  addLog: (message: string, tag?: string) => void;
}

const GAME_LABEL: Record<GameKey, string> = { KOTOR1: "KOTOR 1", KOTOR2: "KOTOR 2" };

export function GameInstallsSection({
  profiles, activeProfile, setActiveProfile, refreshProfiles, addLog,
}: GameInstallsSectionProps) {
  const t = useT();
  const [confirm, confirmDialog] = useConfirm();
  const [adding, setAdding] = useState(false);
  const [editId, setEditId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [detected, setDetected] = useState<
    { game: GameKey; path: string; already_added: boolean }[]>([]);

  // Look for Steam installs so the player does not have to find the folder.
  useEffect(() => {
    api.detectGames().then((r) => setDetected(r.installs)).catch(() => setDetected([]));
  }, [profiles]);

  const useDetected = async (d: { game: GameKey; path: string }) => {
    setBusy(true);
    try {
      // The built-in "KOTOR 1"/"KOTOR 2" entry is usually sitting empty: fill
      // it in rather than adding a second entry for the same game.
      const blank = profiles.find((p) => p.id === d.game && !p.path);
      if (blank) await api.updateProfile(blank.id, { path: d.path });
      else await api.createProfile({ name: `${GAME_LABEL[d.game]} (Steam)`, game: d.game, path: d.path });
      addLog(`Added ${GAME_LABEL[d.game]} from ${d.path}.`, "success");
      await refreshProfiles();
    } catch (e: any) {
      addLog(`Failed to add install: ${e?.message}`, "error");
    } finally {
      setBusy(false);
    }
  };

  // Add form state.
  const [newName, setNewName] = useState("");
  const [newGame, setNewGame] = useState<GameKey>("KOTOR1");
  const [newPath, setNewPath] = useState("");

  // Edit form state.
  const [editName, setEditName] = useState("");
  const [editPath, setEditPath] = useState("");

  const resetAdd = () => { setAdding(false); setNewName(""); setNewGame("KOTOR1"); setNewPath(""); };

  const makeActive = async (id: string) => {
    if (id === activeProfile) return;
    setActiveProfile(id);
    try {
      await api.setActiveProfile(id);
      addLog("Active game install changed.", "muted");
    } catch (e: any) {
      addLog(`Failed to set active install: ${e?.message}`, "error");
    }
  };

  const create = async () => {
    if (!newName.trim() || !newPath.trim()) return;
    setBusy(true);
    try {
      await api.createProfile({ name: newName.trim(), game: newGame, path: newPath.trim() });
      addLog(`Added game install "${newName.trim()}".`, "success");
      resetAdd();
      await refreshProfiles();
    } catch (e: any) {
      addLog(`Failed to add install: ${e?.message}`, "error");
    } finally {
      setBusy(false);
    }
  };

  const startEdit = (p: Profile) => {
    setEditId(p.id);
    setEditName(p.name);
    setEditPath(p.path);
  };

  const saveEdit = async (id: string) => {
    setBusy(true);
    try {
      await api.updateProfile(id, { name: editName.trim(), path: editPath.trim() });
      addLog("Game install updated.", "success");
      setEditId(null);
      await refreshProfiles();
    } catch (e: any) {
      addLog(`Failed to update install: ${e?.message}`, "error");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (p: Profile) => {
    if (!(await confirm({ message: t("settings.installs.deleteConfirm", { name: p.name }), confirmLabel: t("common.delete") }))) return;
    setBusy(true);
    try {
      await api.deleteProfile(p.id);
      addLog(`Deleted game install "${p.name}".`, "muted");
      await refreshProfiles();
    } catch (e: any) {
      addLog(`Failed to delete install: ${e?.message}`, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      {confirmDialog}
      <CardHeader className="flex-row items-center justify-between space-y-0">
        <CardTitle>{t("settings.installs.title")}</CardTitle>
        {!adding && (
          <Button variant="outline" size="sm" onClick={() => setAdding(true)}>
            <Plus /> {t("settings.installs.add")}
          </Button>
        )}
      </CardHeader>
      <CardContent className="space-y-2">
        {detected.some((d) => !d.already_added) && (
          <div className="space-y-2 rounded-md border border-primary/40 bg-primary/5 p-3">
            <p className="text-sm font-medium">{t("settings.installs.detected")}</p>
            {detected.filter((d) => !d.already_added).map((d) => (
              <div key={d.path} className="flex items-center gap-3">
                <Badge variant={d.game === "KOTOR1" ? "info" : "secondary"}>{GAME_LABEL[d.game]}</Badge>
                <p className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground" title={d.path}>{d.path}</p>
                <Button size="sm" disabled={busy} onClick={() => useDetected(d)}>
                  <Plus /> {t("settings.installs.useDetected")}
                </Button>
              </div>
            ))}
          </div>
        )}

        {profiles.length === 0 && !adding && (
          <p className="text-sm text-muted-foreground">{t("settings.installs.empty")}</p>
        )}

        {profiles.map((p) => {
          const isActive = p.id === activeProfile;
          const isEditing = editId === p.id;
          return (
            <div
              key={p.id}
              className={cn(
                "rounded-md border p-3 transition-colors",
                isActive ? "border-primary/50 bg-primary/5" : "bg-card/40"
              )}
            >
              {isEditing ? (
                <div className="space-y-2">
                  <div className="space-y-1.5">
                    <Label>{t("common.name")}</Label>
                    <Input value={editName} onChange={(e) => setEditName(e.target.value)} />
                  </div>
                  <div className="space-y-1.5">
                    <Label>{t("common.path")}</Label>
                    <div className="flex gap-2">
                      <Input value={editPath} onChange={(e) => setEditPath(e.target.value)} />
                      <Button variant="outline" size="icon" title={t("common.browse")}
                              onClick={async () => { const d = await pickDirectory(t("builds.pickGameFolder", { game: GAME_LABEL[p.game] })); if (d) setEditPath(d); }}>
                        <FolderOpen />
                      </Button>
                    </div>
                  </div>
                  <div className="flex gap-2">
                    <Button size="sm" disabled={busy || !editName.trim() || !editPath.trim()}
                            onClick={() => saveEdit(p.id)}>
                      <Check /> {t("common.save")}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setEditId(null)}>
                      <X /> {t("common.cancel")}
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="flex items-center gap-3">
                  <div className="min-w-0 flex-1 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="truncate text-sm font-medium text-foreground" title={p.name}>{p.name}</p>
                      <Badge variant={p.game === "KOTOR1" ? "info" : "secondary"}>
                        {GAME_LABEL[p.game]}
                      </Badge>
                      {isActive && (
                        <span className="inline-flex items-center gap-1 rounded-full bg-[hsl(var(--success)/0.15)] px-2 py-0.5 text-[11px] font-medium text-[hsl(var(--success))]">
                          <CheckCircle2 className="size-3" /> {t("settings.installs.active")}
                        </span>
                      )}
                      {p.is_default && (
                        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground" title={t("settings.installs.defaultTitle")}>
                          <Star className="size-3" /> {t("settings.installs.default")}
                        </span>
                      )}
                    </div>
                    <p className="truncate font-mono text-xs text-muted-foreground" title={p.path}>
                      {p.path || t("settings.installs.noPath")}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    {!isActive && (
                      <Button variant="ghost" size="sm" onClick={() => makeActive(p.id)}>
                        {t("settings.installs.setActive")}
                      </Button>
                    )}
                    <Button variant="ghost" size="icon" className="size-8" title={t("common.edit")}
                            onClick={() => startEdit(p)}>
                      <Pencil />
                    </Button>
                    <Button variant="ghost" size="icon" className="size-8" title={t("common.delete")}
                            disabled={p.is_default || busy} onClick={() => remove(p)}>
                      <Trash2 />
                    </Button>
                  </div>
                </div>
              )}
            </div>
          );
        })}

        {adding && (
          <div className="rounded-md border bg-card/40 p-3">
            <div className="space-y-2">
              <div className="flex gap-2">
                <div className="flex-1 space-y-1.5">
                  <Label>{t("common.name")}</Label>
                  <Input value={newName} placeholder={t("settings.installs.namePlaceholder")}
                         onChange={(e) => setNewName(e.target.value)} />
                </div>
                <div className="w-40 space-y-1.5">
                  <Label>{t("common.game")}</Label>
                  <Select value={newGame} onChange={(e) => setNewGame(e.target.value as GameKey)}>
                    <option value="KOTOR1">KOTOR 1</option>
                    <option value="KOTOR2">KOTOR 2</option>
                  </Select>
                </div>
              </div>
              <div className="space-y-1.5">
                <Label>{t("common.path")}</Label>
                <div className="flex gap-2">
                  <Input value={newPath} placeholder={t("settings.installs.pathPlaceholder")}
                         onChange={(e) => setNewPath(e.target.value)} />
                  <Button variant="outline" size="icon" title={t("common.browse")}
                          onClick={async () => { const d = await pickDirectory(t("builds.pickGameFolder", { game: GAME_LABEL[newGame] })); if (d) setNewPath(d); }}>
                    <FolderOpen />
                  </Button>
                </div>
              </div>
              <div className="flex gap-2">
                <Button size="sm" disabled={busy || !newName.trim() || !newPath.trim()} onClick={create}>
                  <Check /> {t("common.add")}
                </Button>
                <Button variant="ghost" size="sm" onClick={resetAdd}>
                  <X /> {t("common.cancel")}
                </Button>
              </div>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
