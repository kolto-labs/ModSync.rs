import { LogIn, LogOut } from "lucide-react";
import { Avatar } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { useT } from "@/lib/i18n";

interface AccountMenuProps {
  loggedIn: boolean;
  username: string;
  onSignIn: () => void;
  onSignOut: () => void;
  onOpenAccount: () => void;
}

export function AccountMenu({ loggedIn, username, onSignIn, onSignOut, onOpenAccount }: AccountMenuProps) {
  const t = useT();
  if (!loggedIn) {
    return (
      <Button variant="outline" size="sm" className="w-full" onClick={onSignIn}>
        <LogIn /> {t("common.signIn")}
      </Button>
    );
  }

  return (
    <div className="flex items-center gap-2.5">
      <Tooltip content={t("account.openSettings")} side="top">
        <button
          type="button"
          onClick={onOpenAccount}
          className="flex min-w-0 flex-1 items-center gap-2.5 rounded-md p-1 text-left transition-colors hover:bg-sidebar-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Avatar name={username} />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-medium text-foreground">{username || t("account.account")}</p>
            <p className="truncate text-xs text-muted-foreground">{t("account.signedIn")}</p>
          </div>
        </button>
      </Tooltip>
      <Tooltip content={t("common.signOut")} side="top">
        <Button variant="ghost" size="icon" className="size-8" onClick={onSignOut}>
          <LogOut />
        </Button>
      </Tooltip>
    </div>
  );
}
