import { useRef, useState } from "react";
import { AlertTriangle, Check, CheckCircle2, ClipboardCopy, XCircle } from "lucide-react";
import type { CheckReport as Report } from "@/lib/api";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

const STAGE_KEYS: Record<string, string> = {
  Environment: "check.stage.environment",
  Conflicts: "check.stage.conflicts",
  InstallOrder: "check.stage.order",
  ComponentValidation: "check.stage.components",
  FomodConfiguration: "check.stage.fomod",
  DryRun: "check.stage.dryRun",
};

function reportText(report: Report, t: (k: string, v?: Record<string, string | number>) => string): string {
  const lines = [
    report.success ? t("check.passed") : t("check.failed"),
    t("check.counts", { errors: report.errorCount, warnings: report.warningCount }),
    "",
  ];
  for (const s of report.stages) {
    lines.push(`${s.passed ? (s.hasWarnings ? "!" : "OK") : "X"}  ${t(STAGE_KEYS[s.stage] ?? s.stage)}: ${s.summary}`);
    for (const m of s.messages ?? []) lines.push(`    ${m}`);
  }
  for (const i of report.dryRun?.issues ?? []) {
    lines.push(`${i.severity}: ${i.componentName ? `${i.componentName}: ` : ""}${i.message}${i.affectedPath ? ` (${i.affectedPath})` : ""}`);
  }
  return lines.join("\n");
}

interface CheckReportProps {
  report: Report | null;
  onClose: () => void;
}

// The engine's check results: each stage, then every error and warning.
export function CheckReport({ report, onClose }: CheckReportProps) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const firstIssue = useRef<HTMLDivElement | null>(null);
  if (!report) return null;
  const issues = report.dryRun?.issues ?? [];
  const firstIssueIndex = issues.findIndex((i) => i.severity === "Error");
  const firstFailedStage = report.stages.findIndex((s) => !s.passed);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(reportText(report, t));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* clipboard may be unavailable */ }
  };

  return (
    <Dialog open onClose={onClose} title={t("check.title")} className="max-w-xl">
      <p className={cn("text-sm", report.success ? "text-[hsl(var(--success))]" : "text-destructive")}>
        {report.success ? t("check.passed") : t("check.failed")}
      </p>
      <p className="text-xs text-muted-foreground">
        {t("check.counts", { errors: report.errorCount, warnings: report.warningCount })}
      </p>
      <div className="mt-4 max-h-[55vh] space-y-3 overflow-auto pr-1">
        {report.stages.map((s, i) => (
          <div key={s.stage} ref={i === firstFailedStage && firstIssueIndex < 0 ? firstIssue : undefined} className="space-y-1">
            <div className="flex items-center gap-2">
              {!s.passed ? <XCircle className="size-4 shrink-0 text-destructive" />
                : s.hasWarnings ? <AlertTriangle className="size-4 shrink-0 text-[hsl(var(--warning))]" />
                  : <CheckCircle2 className="size-4 shrink-0 text-[hsl(var(--success))]" />}
              <span className="text-sm font-medium">{t(STAGE_KEYS[s.stage] ?? s.stage)}</span>
            </div>
            {s.summary && <p className="pl-6 text-xs text-muted-foreground">{s.summary}</p>}
            {(s.messages ?? []).map((m, j) => (
              <p key={j} className="whitespace-pre-wrap break-words pl-6 text-xs text-muted-foreground">{m}</p>
            ))}
          </div>
        ))}
        {issues.map((issue, i) => (
          <div
            key={i}
            ref={i === firstIssueIndex ? firstIssue : undefined}
            className={cn(
              "rounded-md border p-2.5 text-sm",
              issue.severity === "Error"
                ? "border-destructive/40 bg-destructive/10"
                : "border-[hsl(var(--warning)/0.4)] bg-[hsl(var(--warning)/0.08)]",
            )}
          >
            {issue.componentName && <p className="text-xs font-medium">{issue.componentName}</p>}
            <p className="whitespace-pre-wrap break-words">{issue.message}</p>
            {issue.affectedPath && <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">{issue.affectedPath}</p>}
            {issue.solution && <p className="mt-1 text-xs text-muted-foreground">{issue.solution}</p>}
          </div>
        ))}
      </div>
      <div className="mt-5 flex items-center gap-2">
        {!report.success && (
          <Button variant="ghost" onClick={() => firstIssue.current?.scrollIntoView({ behavior: "smooth", block: "center" })}>
            {t("check.firstIssue")}
          </Button>
        )}
        <Button variant="ghost" onClick={copy}>
          {copied ? <Check className="text-[hsl(var(--success))]" /> : <ClipboardCopy />}
          {copied ? t("activity.copied") : t("check.copy")}
        </Button>
        <Button className="ml-auto" onClick={onClose}>{t("common.close")}</Button>
      </div>
    </Dialog>
  );
}
