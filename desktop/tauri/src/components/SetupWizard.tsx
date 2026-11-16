import { useEffect, useState } from "react";
import { CheckCircle2, ExternalLink, Gamepad2, KeyRound, LogIn, Sparkles } from "lucide-react";
import { api, type GameKey, type Profile } from "@/lib/api";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { LoginForm } from "@/components/LoginDialog";
import { NexusKeyForm } from "@/components/NexusKeyForm";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

const STEPS = ["welcome", "game", "deadlystream", "nexus", "done"] as const;
type Step = (typeof STEPS)[number];

const GAME_LABEL: Record<GameKey, string> = { KOTOR1: "KOTOR 1", KOTOR2: "KOTOR 2" };

interface SetupWizardProps {
  open: boolean;
  /** Called when the player finishes or closes the setup; either way it is not shown again. */
  onFinish: () => void;
  loggedIn: boolean;
  username: string;
  onLoggedIn: (username: string) => void;
  profiles: Profile[];
  refreshProfiles: () => Promise<void> | void;
  addLog: (message: string, tag?: string) => void;
}

// A short walk-through on first launch: find the game, sign in to DeadlyStream,
// add a Nexus key. Every step can be skipped and done later in Settings.
export function SetupWizard({
  open, onFinish, loggedIn, username, onLoggedIn, profiles, refreshProfiles, addLog,
}: SetupWizardProps) {
  const t = useT();
  const [step, setStep] = useState<Step>("welcome");
  const [nexusOk, setNexusOk] = useState(false);
  const [detected, setDetected] = useState<{ game: GameKey; path: string; already_added: boolean }[]>([]);
  const [adding, setAdding] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    api.detectGames().then((r) => setDetected(r.installs)).catch(() => setDetected([]));
  }, [open, profiles]);

  useEffect(() => {
    if (!open) return;
    api.status().then((s) => setNexusOk(!!s.nexus_key_set)).catch(() => {});
  }, [open]);

  const idx = STEPS.indexOf(step);
  const next = () => setStep(STEPS[Math.min(idx + 1, STEPS.length - 1)]);
  const back = () => setStep(STEPS[Math.max(idx - 1, 0)]);

  const addDetected = async (d: { game: GameKey; path: string }) => {
    setAdding(d.path);
    try {
      // Fill in the empty built-in entry for this game rather than adding a second one.
      const blank = profiles.find((p) => p.id === d.game && !p.path);
      if (blank) await api.updateProfile(blank.id, { path: d.path });
      else await api.createProfile({ name: `${GAME_LABEL[d.game]} (Steam)`, game: d.game, path: d.path });
      addLog(`Added ${GAME_LABEL[d.game]} from ${d.path}.`, "success");
      await refreshProfiles();
    } catch (e: any) {
      addLog(`Failed to add install: ${e?.message}`, "error");
    } finally {
      setAdding(null);
    }
  };

  const configured = profiles.filter((p) => p.path);

  return (
    <Dialog open={open} onClose={onFinish} title={t("setup.title")} className="max-w-lg">
      {/* Progress dots */}
      <div className="mb-5 flex items-center gap-1.5" aria-hidden>
        {STEPS.map((s, i) => (
          <span
            key={s}
            className={cn("h-1 flex-1 rounded-full transition-colors", i <= idx ? "bg-primary" : "bg-muted")}
          />
        ))}
      </div>

      <div className="min-h-56">
        {step === "welcome" && (
          <StepBody icon={Sparkles} title={t("setup.welcome.title")}>
            <p>{t("setup.welcome.body")}</p>
            <ul className="list-disc space-y-1 pl-5">
              <li>{t("setup.welcome.item.game")}</li>
              <li>{t("setup.welcome.item.deadlystream")}</li>
              <li>{t("setup.welcome.item.nexus")}</li>
            </ul>
            <p>{t("setup.welcome.skipNote")}</p>
          </StepBody>
        )}

        {step === "game" && (
          <StepBody icon={Gamepad2} title={t("setup.game.title")}>
            <p>{t("setup.game.body")}</p>
            {configured.length > 0 && (
              <ul className="space-y-1.5">
                {configured.map((p) => (
                  <li key={p.id} className="flex items-center gap-2 text-foreground">
                    <CheckCircle2 className="size-4 shrink-0 text-[hsl(var(--success))]" />
                    <span className="truncate" title={p.path}>{p.name}: {p.path}</span>
                  </li>
                ))}
              </ul>
            )}
            {detected.filter((d) => !d.already_added).map((d) => (
              <div key={d.path} className="flex items-center gap-2 rounded-md border px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium text-foreground">{t("setup.game.found", { game: GAME_LABEL[d.game] })}</p>
                  <p className="truncate text-xs" title={d.path}>{d.path}</p>
                </div>
                <Button size="sm" onClick={() => addDetected(d)} disabled={adding !== null}>
                  {adding === d.path ? t("common.saving") : t("setup.game.use")}
                </Button>
              </div>
            ))}
            {configured.length === 0 && detected.length === 0 && <p>{t("setup.game.notFound")}</p>}
          </StepBody>
        )}

        {step === "deadlystream" && (
          <StepBody icon={LogIn} title={t("setup.deadlystream.title")}>
            <p>{t("setup.deadlystream.body")}</p>
            {loggedIn ? (
              <p className="flex items-center gap-1.5 text-foreground">
                <CheckCircle2 className="size-4 text-[hsl(var(--success))]" />
                {t("setup.deadlystream.signedIn", { name: username })}
              </p>
            ) : (
              <LoginForm onLoggedIn={onLoggedIn} />
            )}
          </StepBody>
        )}

        {step === "nexus" && (
          <StepBody icon={KeyRound} title={t("setup.nexus.title")}>
            <p>
              {t("settings.nexus.hint")}{" "}
              <button
                className="inline-flex items-center gap-0.5 text-[hsl(var(--info))] hover:underline"
                onClick={() => api.openUrl("https://www.nexusmods.com/users/myaccount?tab=api").catch(() => {})}
              >
                {t("settings.nexus.getKey")} <ExternalLink className="size-3" />
              </button>
            </p>
            <NexusKeyForm addLog={addLog} onValidated={(v) => setNexusOk(v.ok)} />
          </StepBody>
        )}

        {step === "done" && (
          <StepBody
            icon={CheckCircle2}
            title={configured.length > 0 && loggedIn && nexusOk ? t("setup.done.title") : t("setup.done.titlePartial")}
          >
            <ul className="space-y-1.5">
              <Summary ok={configured.length > 0} label={t("setup.done.game")} />
              <Summary ok={loggedIn} label={t("setup.done.deadlystream")} />
              <Summary ok={nexusOk} label={t("setup.done.nexus")} />
            </ul>
            <p>{t("setup.done.body")}</p>
          </StepBody>
        )}
      </div>

      <div className="mt-6 flex items-center gap-2">
        {idx > 0 && step !== "done" && (
          <Button variant="ghost" onClick={back}>{t("setup.back")}</Button>
        )}
        <div className="ml-auto flex gap-2">
          {step === "welcome" && (
            <Button variant="ghost" onClick={onFinish}>{t("setup.skipAll")}</Button>
          )}
          {step === "done" ? (
            <Button onClick={onFinish}>{t("setup.finish")}</Button>
          ) : (
            // A step that is not done yet offers a quiet "Skip for now" rather than a bold "Next".
            step === "welcome" || stepDone(step, configured.length > 0, loggedIn, nexusOk)
              ? <Button onClick={next}>{step === "welcome" ? t("setup.start") : t("setup.next")}</Button>
              : <Button variant="outline" onClick={next}>{t("setup.skip")}</Button>
          )}
        </div>
      </div>
    </Dialog>
  );
}

/** Whether the step's job is done, so the button reads "Next" rather than "Skip for now". */
function stepDone(step: Step, hasGame: boolean, loggedIn: boolean, nexusOk: boolean): boolean {
  if (step === "game") return hasGame;
  if (step === "deadlystream") return loggedIn;
  if (step === "nexus") return nexusOk;
  return false;
}

function StepBody({ icon: Icon, title, children }: { icon: typeof Sparkles; title: string; children: React.ReactNode }) {
  return (
    <div className="space-y-3 text-sm text-muted-foreground animate-fade-in">
      <h3 className="flex items-center gap-2 text-base font-semibold text-foreground">
        <Icon className="size-5 text-primary" /> {title}
      </h3>
      {children}
    </div>
  );
}

function Summary({ ok, label }: { ok: boolean; label: string }) {
  const t = useT();
  return (
    <li className="flex items-center gap-2">
      <CheckCircle2 className={cn("size-4 shrink-0", ok ? "text-[hsl(var(--success))]" : "text-muted-foreground/40")} />
      <span className={ok ? "text-foreground" : ""}>{label}</span>
      {!ok && <span className="text-xs">({t("setup.done.later")})</span>}
    </li>
  );
}
