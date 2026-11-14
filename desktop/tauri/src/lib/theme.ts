import { useSyncExternalStore } from "react";

// Light / dark appearance. Dark is the default so the app keeps its usual look
// until the player chooses otherwise; "system" follows the Windows setting.
export type Theme = "dark" | "light" | "system";

const KEY = "kmi.theme";
const listeners = new Set<() => void>();
const media = typeof window !== "undefined" ? window.matchMedia?.("(prefers-color-scheme: dark)") : undefined;

function read(): Theme {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "dark" || v === "light" || v === "system") return v;
  } catch { /* storage unavailable: use the default */ }
  return "dark";
}

let current: Theme = read();

/** The theme actually shown, with "system" resolved. */
export function resolvedTheme(t: Theme = current): "dark" | "light" {
  if (t === "system") return media?.matches === false ? "light" : "dark";
  return t;
}

function apply() {
  const dark = resolvedTheme() === "dark";
  const root = document.documentElement;
  root.classList.toggle("dark", dark);
  root.classList.toggle("light", !dark);
  root.style.colorScheme = dark ? "dark" : "light";
}

export function setTheme(t: Theme) {
  current = t;
  try { localStorage.setItem(KEY, t); } catch { /* not remembered, still applied */ }
  apply();
  listeners.forEach((l) => l());
}

/** Apply the saved theme before the first paint, and follow system changes. */
export function initTheme() {
  apply();
  media?.addEventListener?.("change", () => {
    if (current === "system") { apply(); listeners.forEach((l) => l()); }
  });
}

export function useTheme(): [Theme, (t: Theme) => void] {
  const theme = useSyncExternalStore(
    (cb) => { listeners.add(cb); return () => { listeners.delete(cb); }; },
    () => current,
  );
  return [theme, setTheme];
}
