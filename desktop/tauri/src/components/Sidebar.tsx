import type { AppStatus } from "@/lib/api";
import { NAV_ITEMS, type ViewId } from "@/lib/views";
import { NavItem } from "@/components/NavItem";
import { AccountMenu } from "@/components/AccountMenu";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Moon, Sun } from "lucide-react";
import { Tooltip } from "@/components/ui/tooltip";
import { useT } from "@/lib/i18n";
import { resolvedTheme, useTheme } from "@/lib/theme";

interface SidebarProps {
  active: ViewId;
  onNavigate: (v: ViewId) => void;
  status: AppStatus | null;
  username: string;
  running: boolean;
  overallPct: number;
  conflictCount: number;
  onSignIn: () => void;
  onSignOut: () => void;
  onOpenAccount: () => void;
}

export function Sidebar({
  active, onNavigate, status, username, running, overallPct, conflictCount,
  onSignIn, onSignOut, onOpenAccount,
}: SidebarProps) {
  const t = useT();
  const [theme, setTheme] = useTheme();
  const isDark = resolvedTheme(theme) === "dark";
  const navItems = NAV_ITEMS.filter((n) => n.id !== "settings");
  const settingsItem = NAV_ITEMS.find((n) => n.id === "settings")!;

  return (
    <aside className="flex h-full w-48 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
      {/* Brand */}
      <div className="flex items-start gap-2 px-5 py-6">
        <div className="min-w-0 flex-1">
          <p className="truncate font-semibold text-foreground">{t("brand.title")}</p>
          {status && <p className="text-xs text-muted-foreground">v{status.version}</p>}
        </div>
        <Tooltip content={isDark ? t("theme.switchToLight") : t("theme.switchToDark")} side="bottom">
          <button
            type="button"
            onClick={() => setTheme(isDark ? "light" : "dark")}
            aria-label={isDark ? t("theme.switchToLight") : t("theme.switchToDark")}
            className="-mr-1.5 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            {isDark ? <Sun className="size-4" /> : <Moon className="size-4" />}
          </button>
        </Tooltip>
      </div>

      {/* Nav (incl. Settings, all above the account footer) */}
      <nav className="flex flex-1 flex-col gap-0.5 overflow-y-auto">
        {navItems.map((item) => {
          let trailing: React.ReactNode = null;
          if (item.id === "conflicts" && conflictCount > 0) {
            trailing = <Badge variant="warning" className="px-1.5 py-0 text-[10px]">{conflictCount}</Badge>;
          } else if (item.id === "builds" && running) {
            trailing = (
              <span className="font-mono text-[11px] text-muted-foreground">{Math.round(overallPct)}%</span>
            );
          }
          return (
            <NavItem
              key={item.id}
              label={t(item.labelKey)}
              active={active === item.id}
              onClick={() => onNavigate(item.id)}
              trailing={trailing}
            />
          );
        })}
        <div className="mt-auto pt-6" />
        <NavItem
          label={t(settingsItem.labelKey)}
          active={active === "settings"}
          onClick={() => onNavigate("settings")}
        />
      </nav>

      {/* Account footer - pinned at the very bottom, below all nav items */}
      <Separator className="bg-sidebar-border" />
      <div className="p-3">
        <AccountMenu
          loggedIn={!!status?.logged_in}
          username={username}
          onSignIn={onSignIn}
          onSignOut={onSignOut}
          onOpenAccount={onOpenAccount}
        />
      </div>
    </aside>
  );
}
