import * as React from "react";
import { Check } from "lucide-react";
import { cn } from "@/lib/utils";

interface CheckboxProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  id?: string;
  className?: string;
  "aria-label"?: string;
}

// Lightweight checkbox (no Radix dependency) matching the shadcn look.
const Checkbox = React.forwardRef<HTMLButtonElement, CheckboxProps>(
  ({ checked, onCheckedChange, disabled, id, className, ...props }, ref) => (
    <button
      ref={ref}
      id={id}
      type="button"
      role="checkbox"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "inline-flex size-4 shrink-0 items-center justify-center rounded-[4px] border border-input transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        checked ? "border-primary bg-primary text-primary-foreground" : "bg-background/60",
        className
      )}
      {...props}
    >
      {checked && <Check className="size-3" />}
    </button>
  )
);
Checkbox.displayName = "Checkbox";

export { Checkbox };
