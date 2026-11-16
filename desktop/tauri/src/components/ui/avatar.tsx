import * as React from "react";
import { cn } from "@/lib/utils";

interface AvatarProps extends React.HTMLAttributes<HTMLDivElement> {
  name?: string;
}

// Rounded circle showing the first initial of a name.
const Avatar = React.forwardRef<HTMLDivElement, AvatarProps>(
  ({ className, name, ...props }, ref) => {
    const initial = (name?.trim()?.[0] ?? "?").toUpperCase();
    return (
      <div
        ref={ref}
        className={cn(
          "flex size-8 shrink-0 select-none items-center justify-center rounded-full bg-sidebar-accent text-sm font-semibold text-accent-foreground",
          className
        )}
        {...props}
      >
        {initial}
      </div>
    );
  }
);
Avatar.displayName = "Avatar";

export { Avatar };
