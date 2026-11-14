import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

export interface LogLine {
  id: number;
  message: string;
  tag: string;
  ts: number; // epoch ms when the line was recorded
}

const TAG_COLOR: Record<string, string> = {
  success: "text-[hsl(var(--success))]",
  error: "text-destructive",
  warning: "text-[hsl(var(--warning))]",
  info: "text-[hsl(var(--info))]",
  muted: "text-muted-foreground",
};

/** Local wall-clock HH:MM:SS for a log line. */
export function fmtLogTime(ts: number): string {
  const d = new Date(ts);
  if (isNaN(d.getTime())) return "--:--:--";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** Plain-text rendering of the log, used for copy/export. */
export function logsToText(lines: LogLine[]): string {
  return lines
    .map((l) => {
      const d = new Date(l.ts);
      const stamp = isNaN(d.getTime()) ? "" : d.toISOString();
      const tag = l.tag ? `[${l.tag.toUpperCase()}] ` : "";
      return `${stamp} ${tag}${l.message}`;
    })
    .join("\n");
}

export function LogPanel({ lines }: { lines: LogLine[] }) {
  const t = useT();
  const endRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines.length]);

  return (
    <div className="h-full overflow-auto rounded-md bg-[hsl(var(--sidebar))] p-3 font-mono text-xs leading-relaxed">
      {lines.length === 0 && (
        <div className="text-muted-foreground/60">{t("activity.empty")}</div>
      )}
      {lines.map((l) => (
        <div key={l.id} className={cn("flex gap-2 whitespace-pre-wrap", TAG_COLOR[l.tag] ?? "text-foreground/90")}>
          <span className="shrink-0 select-none text-muted-foreground/50 tabular-nums">
            {fmtLogTime(l.ts)}
          </span>
          <span className="min-w-0 flex-1">{l.message}</span>
        </div>
      ))}
      <div ref={endRef} />
    </div>
  );
}
