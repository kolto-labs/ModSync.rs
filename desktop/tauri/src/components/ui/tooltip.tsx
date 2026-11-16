import * as React from "react";
import { cn } from "@/lib/utils";

interface TooltipProps {
  content: React.ReactNode;
  children: React.ReactNode;
  side?: "top" | "bottom" | "left" | "right";
  className?: string;
}

const SIDE: Record<NonNullable<TooltipProps["side"]>, string> = {
  top: "bottom-full left-1/2 -translate-x-1/2 mb-1.5",
  bottom: "top-full left-1/2 -translate-x-1/2 mt-1.5",
  left: "right-full top-1/2 -translate-y-1/2 mr-1.5",
  right: "left-full top-1/2 -translate-y-1/2 ml-1.5",
};

// Pure-CSS hover tooltip (no Radix).
export function Tooltip({ content, children, side = "top", className }: TooltipProps) {
  return (
    <span className="group/tooltip relative inline-flex">
      {children}
      <span
        role="tooltip"
        className={cn(
          "pointer-events-none absolute z-50 whitespace-nowrap rounded-md border bg-card px-2 py-1 text-xs text-card-foreground opacity-0 shadow-md transition-opacity group-hover/tooltip:opacity-100",
          SIDE[side],
          className
        )}
      >
        {content}
      </span>
    </span>
  );
}
