import type { ReactNode } from "react";
import { Sidebar } from "@/components/Sidebar";
import type { AppStatus } from "@/lib/api";
import type { ViewId } from "@/lib/views";

interface AppShellProps {
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
  children: ReactNode;
}

export function AppShell({ children, ...sidebar }: AppShellProps) {
  return (
    <div className="flex h-full bg-background text-foreground">
      <Sidebar {...sidebar} />
      <main className="min-w-0 flex-1 overflow-hidden">{children}</main>
    </div>
  );
}
