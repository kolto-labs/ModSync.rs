import * as React from "react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

interface EmptyStateProps {
  title: string;
  subtitle?: string;
  action?: { label: string; onClick: () => void };
  className?: string;
  children?: React.ReactNode;
}

export function EmptyState({ title, subtitle, action, className, children }: EmptyStateProps) {
  return (
    <div className={cn("flex h-full flex-col items-center justify-center gap-3 p-8 text-center", className)}>
      <div className="space-y-1">
        <p className="text-sm font-medium text-foreground">{title}</p>
        {subtitle && <p className="max-w-sm text-sm text-muted-foreground">{subtitle}</p>}
      </div>
      {action && (
        <Button variant="outline" size="sm" onClick={action.onClick}>
          {action.label}
        </Button>
      )}
      {children}
    </div>
  );
}
