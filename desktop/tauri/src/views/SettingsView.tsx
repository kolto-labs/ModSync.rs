import { useEffect, useState } from "react";
import type { AppStatus, Profile } from "@/lib/api";
import { cn } from "@/lib/utils";
import { GeneralSection } from "@/views/settings/GeneralSection";
import { GameInstallsSection } from "@/views/settings/GameInstallsSection";
import { AccountSection } from "@/views/settings/AccountSection";
import { PatcherSection } from "@/views/settings/PatcherSection";
import { UpdatesSection } from "@/views/settings/UpdatesSection";
import { useT } from "@/lib/i18n";

export type SettingsSectionId = "general" | "installs" | "account" | "patcher" | "updates";
type SectionId = SettingsSectionId;

const SECTIONS: { id: SectionId; labelKey: string }[] = [
  { id: "general", labelKey: "settings.section.general" },
  { id: "installs", labelKey: "settings.section.installs" },
  { id: "account", labelKey: "settings.section.account" },
  { id: "patcher", labelKey: "settings.section.patcher" },
  { id: "updates", labelKey: "settings.section.updates" },
];

interface SettingsViewProps {
  status: AppStatus | null;
  username: string;
  onSignIn: () => void;
  onSignOut: () => void;
  addLog: (message: string, tag?: string) => void;
  profiles: Profile[];
  activeProfile: string;
  setActiveProfile: (id: string) => void;
  refreshProfiles: () => Promise<void> | void;
  section: SectionId;
  setSection: (s: SectionId) => void;
  onRunSetup: () => void;
}

export function SettingsView({
  status, username, onSignIn, onSignOut, addLog,
  profiles, activeProfile, setActiveProfile, refreshProfiles,
  section, setSection, onRunSetup,
}: SettingsViewProps) {
  const t = useT();
  const [visited, setVisited] = useState(() => new Set<SectionId>([section]));
  useEffect(() => {
    setVisited(prev => prev.has(section) ? prev : new Set([...prev, section]));
  }, [section]);

  return (
    <div className="flex h-full flex-col">
      <header className="view-header border-b">
        <h1 className="text-base font-semibold">{t("settings.title")}</h1>
        <p className="text-xs text-muted-foreground">{t("settings.subtitle")}</p>
      </header>

      <div className="flex min-h-0 flex-1 flex-col">
        {/* Secondary sidebar */}
        <nav aria-label={t("settings.title")} className="flex shrink-0 gap-1 overflow-x-auto border-b px-5">
          {SECTIONS.map(({ id, labelKey }) => (
            <button
              key={id}
              type="button"
              aria-current={section === id ? "page" : undefined}
              onClick={() => setSection(id)}
              className={cn(
                "shrink-0 border-b-2 px-3 py-3 text-sm transition-colors",
                section === id
                  ? "border-primary text-primary"
                  : "border-transparent text-muted-foreground hover:text-foreground"
              )}
            >
              {t(labelKey)}
            </button>
          ))}
        </nav>

        {/* Content */}
        <div className="min-h-0 flex-1 overflow-auto p-6">
          <div className="max-w-2xl space-y-6">
            {visited.has("general") && <div hidden={section !== "general"}><GeneralSection addLog={addLog} onRunSetup={onRunSetup} /></div>}
            {visited.has("installs") && <div hidden={section !== "installs"}>
              <GameInstallsSection
                profiles={profiles}
                activeProfile={activeProfile}
                setActiveProfile={setActiveProfile}
                refreshProfiles={refreshProfiles}
                addLog={addLog}
              />
            </div>}
            {visited.has("account") && <div hidden={section !== "account"}>
              <AccountSection
                status={status}
                username={username}
                onSignIn={onSignIn}
                onSignOut={onSignOut}
                addLog={addLog}
              />
            </div>}
            {visited.has("patcher") && <div hidden={section !== "patcher"}><PatcherSection status={status} addLog={addLog} /></div>}
            {visited.has("updates") && <div hidden={section !== "updates"}><UpdatesSection status={status} addLog={addLog} /></div>}
          </div>
        </div>
      </div>
    </div>
  );
}
