import * as React from "react";
import { cn } from "@/lib/utils";

export interface ContextMenuItem {
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
}

interface ContextMenuProps {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

/**
 * A lightweight right-click menu rendered at (x, y). Closes on outside click,
 * escape, scroll, or window resize. Flips when it would overflow the viewport.
 */
export function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const ref = React.useRef<HTMLDivElement>(null);
  const [pos, setPos] = React.useState({ x, y });

  React.useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const nx = x + width > window.innerWidth ? Math.max(8, window.innerWidth - width - 8) : x;
    const ny = y + height > window.innerHeight ? Math.max(8, window.innerHeight - height - 8) : y;
    setPos({ x: nx, y: ny });
  }, [x, y]);

  React.useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onClose);
    window.addEventListener("scroll", onClose, true);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("scroll", onClose, true);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      style={{ top: pos.y, left: pos.x }}
      className="fixed z-[60] min-w-[11rem] animate-fade-in rounded-md border bg-card p-1 shadow-xl"
      role="menu"
    >
      {items.map((item, i) => {
        const Icon = item.icon;
        return (
          <button
            key={i}
            role="menuitem"
            disabled={item.disabled}
            onClick={() => { if (!item.disabled) { item.onSelect(); onClose(); } }}
            className={cn(
              "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm transition-colors",
              item.disabled
                ? "cursor-not-allowed text-muted-foreground/50"
                : item.danger
                  ? "text-destructive hover:bg-destructive/10"
                  : "hover:bg-accent",
            )}
          >
            {Icon && <Icon className="size-4 shrink-0" />}
            <span className="flex-1 truncate">{item.label}</span>
          </button>
        );
      })}
    </div>
  );
}
