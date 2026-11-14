import { useState } from "react";
import { Check, ClipboardCopy, Download } from "lucide-react";
import { LogPanel, logsToText, type LogLine } from "@/components/LogPanel";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";

interface ActivityViewProps {
  logs: LogLine[];
  onClear: () => void;
}

export function ActivityView({ logs, onClear }: ActivityViewProps) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const empty = logs.length === 0;

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(logsToText(logs));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard may be unavailable */
    }
  };

  const exportTxt = () => {
    const blob = new Blob([logsToText(logs)], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    a.href = url;
    a.download = `kotor-mod-installer-log-${stamp}.txt`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 view-header border-b">
        <div>
          <h1 className="text-base font-semibold">{t("activity.title")}</h1>
          <p className="text-xs text-muted-foreground">{t("activity.entries", { count: logs.length })}</p>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={copy} disabled={empty}>
            {copied ? <Check className="text-[hsl(var(--success))]" /> : <ClipboardCopy />}
            {copied ? t("activity.copied") : t("activity.copy")}
          </Button>
          <Button variant="ghost" size="sm" onClick={exportTxt} disabled={empty}>
            <Download /> {t("activity.export")}
          </Button>
          <Button variant="ghost" size="sm" onClick={onClear} disabled={empty}>{t("common.clear")}</Button>
        </div>
      </header>
      <div className="min-h-0 flex-1 p-4">
        <LogPanel lines={logs} />
      </div>
    </div>
  );
}
