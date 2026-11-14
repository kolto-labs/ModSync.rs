import { cn } from "@/lib/utils";

interface NavItemProps {
  label: string;
  active?: boolean;
  onClick: () => void;
  trailing?: React.ReactNode;
}

export function NavItem({ label, active, onClick, trailing }: NavItemProps) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      onClick={onClick}
      className={cn(
        // Full-width rows, so the active one fills edge to edge with a gold marker.
        "flex w-full items-center gap-3 border-l-[3px] py-2.5 pl-[17px] pr-5 text-sm transition-colors",
        active
          ? "border-primary bg-sidebar-accent font-medium text-foreground"
          : "border-transparent text-sidebar-foreground hover:bg-accent/50 hover:text-foreground"
      )}
    >
      <span className="flex-1 truncate text-left">{label}</span>
      {trailing}
    </button>
  );
}
