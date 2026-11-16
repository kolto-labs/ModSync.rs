import {
  createContext, createElement, useCallback, useContext, useEffect, useMemo, useState,
  type ReactNode,
} from "react";
import { api } from "@/lib/api";
import { en } from "./en";
import { es } from "./es";
import { de } from "./de";
import { DEFAULT_LOCALE, type Dict, type Locale } from "./types";

export { LOCALES, DEFAULT_LOCALE } from "./types";
export type { Locale } from "./types";

const DICTS: Record<Locale, Dict> = { en, es, de };

export type TFn = (key: string, vars?: Record<string, string | number>) => string;

interface LanguageContextValue {
  locale: Locale;
  setLocale: (l: Locale) => void;
  t: TFn;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

function interpolate(template: string, vars?: Record<string, string | number>): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (m, name) =>
    name in vars ? String(vars[name]) : m
  );
}

function isLocale(v: unknown): v is Locale {
  return v === "en" || v === "es" || v === "de";
}

export function LanguageProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(DEFAULT_LOCALE);

  // Load the persisted locale from backend settings on init (default "en").
  useEffect(() => {
    let alive = true;
    api.getSettings()
      .then((s) => { if (alive && isLocale(s.language)) setLocaleState(s.language); })
      .catch(() => { /* default to "en" */ });
    return () => { alive = false; };
  }, []);

  // Keep <html lang> in sync.
  useEffect(() => {
    if (typeof document !== "undefined") document.documentElement.lang = locale;
  }, [locale]);

  // Update in-memory state instantly, then persist to backend settings.
  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next);
    api.getSettings()
      .then((s) => api.setSettings({ ...s, language: next }))
      .catch(() => { /* persistence is best-effort */ });
  }, []);

  const t = useCallback<TFn>((key, vars) => {
    const dict = DICTS[locale];
    const raw = dict[key] ?? en[key] ?? key;
    return interpolate(raw, vars);
  }, [locale]);

  const value = useMemo<LanguageContextValue>(() => ({ locale, setLocale, t }), [locale, setLocale, t]);

  return createElement(LanguageContext.Provider, { value }, children);
}

export function useLanguage(): LanguageContextValue {
  const ctx = useContext(LanguageContext);
  if (!ctx) throw new Error("useLanguage must be used within a LanguageProvider");
  return ctx;
}

// Convenience hook returning just the translate function.
export function useT(): TFn {
  return useLanguage().t;
}
