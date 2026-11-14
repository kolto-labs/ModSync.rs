export type Locale = "en" | "es" | "de";

// Flat dictionary: hierarchical dotted keys → translated string.
export type Dict = Record<string, string>;

export const LOCALES: { value: Locale; label: string }[] = [
  { value: "en", label: "English" },
  { value: "es", label: "Español" },
  { value: "de", label: "Deutsch" },
];

export const DEFAULT_LOCALE: Locale = "en";
