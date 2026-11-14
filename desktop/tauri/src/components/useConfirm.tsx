import { useCallback, useRef, useState, type ReactNode } from "react";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";

interface ConfirmOptions {
  message: string;
  confirmLabel: string;
  /** Red button for destructive actions (the default); "default" for plain warnings. */
  variant?: "destructive" | "default";
  /** An optional third choice, e.g. "go and fix it". Counts as not confirmed. */
  alt?: { label: string; onSelect: () => void };
}

// Promise-based replacement for window.confirm that matches the rest of the app.
// Render `dialog` somewhere in the component, then `await confirm({...})`.
export function useConfirm(): [(o: ConfirmOptions) => Promise<boolean>, ReactNode] {
  const t = useT();
  const [opts, setOpts] = useState<ConfirmOptions | null>(null);
  const resolver = useRef<((ok: boolean) => void) | null>(null);

  const confirm = useCallback((o: ConfirmOptions) => new Promise<boolean>((resolve) => {
    resolver.current = resolve;
    setOpts(o);
  }), []);

  const answer = (ok: boolean, alt = false) => {
    if (alt) opts?.alt?.onSelect();
    resolver.current?.(ok);
    resolver.current = null;
    setOpts(null);
  };

  const dialog = (
    <Dialog open={!!opts} onClose={() => answer(false)}>
      <p className="whitespace-pre-line text-sm">{opts?.message}</p>
      <div className="mt-5 flex justify-end gap-2">
        {opts?.alt && (
          <Button variant="ghost" className="mr-auto" onClick={() => answer(false, true)}>{opts.alt.label}</Button>
        )}
        <Button variant="outline" onClick={() => answer(false)}>{t("common.cancel")}</Button>
        <Button variant={opts?.variant ?? "destructive"} onClick={() => answer(true)}>{opts?.confirmLabel}</Button>
      </div>
    </Dialog>
  );
  return [confirm, dialog];
}
