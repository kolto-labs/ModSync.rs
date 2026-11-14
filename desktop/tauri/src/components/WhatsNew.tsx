import { Fragment, useMemo, type ReactNode } from "react";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";

// Render inline **bold** segments within a line.
function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*)/g).map((part, i) =>
    part.startsWith("**") && part.endsWith("**") ? (
      <strong key={i} className="font-medium text-foreground">{part.slice(2, -2)}</strong>
    ) : (
      <Fragment key={i}>{part}</Fragment>
    )
  );
}

interface WhatsNewProps {
  open: boolean;
  version: string;
  notes: string;
  onClose: () => void;
}

type Block =
  | { kind: "heading"; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "para"; text: string };

// Tiny markdown renderer (no deps). Handles:
//   `## [..]`  → version heading (skipped; the dialog title already shows version)
//   `### Group`→ bold section subheading
//   `- item`   → bullet list
//   blank line → spacing / block break
function parseNotes(notes: string): Block[] {
  const blocks: Block[] = [];
  let list: string[] | null = null;

  const flushList = () => {
    if (list && list.length) blocks.push({ kind: "list", items: list });
    list = null;
  };

  for (const rawLine of notes.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line) { flushList(); continue; }

    if (line.startsWith("## ")) {
      // Top-level version heading - the dialog title already shows the version.
      flushList();
      continue;
    }
    if (line.startsWith("### ")) {
      flushList();
      blocks.push({ kind: "heading", text: line.slice(4).trim() });
      continue;
    }
    if (line.startsWith("- ") || line.startsWith("* ")) {
      if (!list) list = [];
      list.push(line.slice(2).trim());
      continue;
    }
    // Plain paragraph line.
    flushList();
    blocks.push({ kind: "para", text: line });
  }
  flushList();
  return blocks;
}

export function WhatsNew({ open, version, notes, onClose }: WhatsNewProps) {
  const t = useT();
  const blocks = useMemo(() => parseNotes(notes), [notes]);

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("whatsNew.title", { version })}
      className="max-w-lg"
    >
      <div className="max-h-[60vh] space-y-3 overflow-auto pr-1 text-sm">
        {blocks.map((b, i) => {
          if (b.kind === "heading") {
            return (
              <h3 key={i} className="text-sm font-semibold text-foreground first:mt-0">
                {b.text}
              </h3>
            );
          }
          if (b.kind === "list") {
            return (
              <ul key={i} className="list-disc space-y-1 pl-5 text-muted-foreground">
                {b.items.map((it, j) => (
                  <li key={j} className="leading-relaxed">{inline(it)}</li>
                ))}
              </ul>
            );
          }
          return (
            <p key={i} className="leading-relaxed text-muted-foreground">{inline(b.text)}</p>
          );
        })}
      </div>
      <div className="mt-4 flex justify-end">
        <Button onClick={onClose}>{t("whatsNew.gotIt")}</Button>
      </div>
    </Dialog>
  );
}
