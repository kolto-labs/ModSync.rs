import type { ModStatus } from "./api";

type BadgeVariant =
  | "default" | "secondary" | "muted" | "success" | "warning" | "info" | "destructive" | "outline";

export const STATUS_META: Record<ModStatus, { label: string; variant: BadgeVariant }> = {
  PENDING:         { label: "Pending",     variant: "muted" },
  DOWNLOADING:     { label: "Downloading", variant: "info" },
  EXTRACTING:      { label: "Extracting",  variant: "info" },
  READY:           { label: "Ready",       variant: "warning" },
  INSTALLING:      { label: "Installing",  variant: "warning" },
  WAITING_PATCHER: { label: "Patcher",     variant: "warning" },
  DONE:            { label: "Done",        variant: "success" },
  SKIPPED:         { label: "Skipped",     variant: "muted" },
  MANUAL:          { label: "Manual step", variant: "warning" },
  ERROR:           { label: "Error",       variant: "destructive" },
};

export const ACTIVE_STATUSES: ModStatus[] = ["DOWNLOADING", "EXTRACTING", "INSTALLING", "WAITING_PATCHER"];
