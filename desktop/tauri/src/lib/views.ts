export type ViewId = "builds" | "library" | "conflicts" | "activity" | "settings";

export interface NavMeta {
  id: ViewId;
  labelKey: string;
}

export const NAV_ITEMS: NavMeta[] = [
  { id: "builds", labelKey: "nav.builds" },
  { id: "library", labelKey: "nav.library" },
  { id: "conflicts", labelKey: "nav.conflicts" },
  { id: "activity", labelKey: "nav.activity" },
  { id: "settings", labelKey: "nav.settings" },
];
