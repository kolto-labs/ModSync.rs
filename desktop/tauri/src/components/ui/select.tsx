import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {}

// Native select styled to match shadcn (no Radix dependency).
const Select = React.forwardRef<HTMLSelectElement, SelectProps>(
  ({ className, children, ...props }, ref) => (
    // The wrapper carries any sizing className (w-64, max-w-*, etc.) so the
    // control and chevron size together and don't overflow their container.
    <div className={cn("relative inline-flex", className)}>
      <select
        ref={ref}
        className={cn(
          // `text-foreground` + the `[&>option]` rules keep the native popup
          // legible on the dark theme (default options render dark-on-dark).
          "h-9 w-full appearance-none rounded-md border border-input bg-background/60 pl-3 pr-9 text-sm text-foreground shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 [&>option]:bg-card [&>option]:text-foreground"
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
    </div>
  )
);
Select.displayName = "Select";

export { Select };
