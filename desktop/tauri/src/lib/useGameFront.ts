import { useEffect } from "react";
import { api } from "./api";
import { closeGameFront, finishGameFront, isTauri, openGameFront } from "./tauri";

/** Polling makes the active browser job replayable after a frontend reload. */
export function useGameFront(ready: boolean, log: (message: string, tag?: string) => void) {
  useEffect(() => {
    if (!ready || !isTauri()) return;
    let stopped = false, busy = false, current: string | null = null;
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event").then(async ({ listen }) => {
      const offMessage = await listen<string>("gamefront-message", e => log(e.payload, "info"));
      // The player closed the GameFront window before the mod arrived, for
      // example because it stayed blank. Carry on in their regular browser.
      const offClosed = await listen("gamefront-closed", async () => {
        try {
          if (!(await api.gamefrontRequest()).request) return;
          const fallback = await api.gamefrontExternal();
          log(`GameFront was opened in your regular browser. Save the download in ${fallback.destination} or your Downloads folder.`, "warning");
        } catch { /* The backend is gone; nothing is waiting on GameFront. */ }
      });
      const off = () => { offMessage(); offClosed(); };
      if (stopped) off(); else unlisten = off;
    }).catch(() => {});
    const poll = async () => {
      if (busy || stopped) return;
      busy = true;
      try {
        const { request } = await api.gamefrontRequest();
        if (stopped || request?.id === current) return;
        if (current) await finishGameFront(current).catch(() => {});
        // Remember the job before opening it, so a failing browser window is
        // tried once per mod instead of being reopened every second.
        current = request?.id ?? null;
        if (request) {
          try {
            await openGameFront(request);
          } catch (error) {
            const reason = error instanceof Error ? error.message : String(error);
            log(`GameFront window problem: ${reason}`, "warning");
            // The built-in window failed or froze: close whatever is left of
            // it and hand the download to the player's regular browser.
            await closeGameFront().catch(() => {});
            try {
              const fallback = await api.gamefrontExternal();
              log(`The GameFront window could not open. Your regular browser was opened instead. Save the download in ${fallback.destination} or your Downloads folder.`, "warning");
            } catch {
              log(`The GameFront window could not open. Open ${request.url} in your browser and save the download in ${request.destination}.`, "warning");
            }
          }
        }
      } catch { /* Retry when the backend becomes reachable again. */ }
      finally { busy = false; }
    };
    void poll();
    const timer = window.setInterval(poll, 1000);
    return () => { stopped = true; window.clearInterval(timer); unlisten?.(); };
  }, [ready, log]);
}
