import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";

export interface ConfirmRequest {
  id: string;
  title: string;
  body: string;
  options: string[];
}

interface ConfirmDialogProps {
  request: ConfirmRequest | null;
  onAnswer: (id: string, choice: string) => void;
}

// A question from the installer. Closing the window counts as the last option
// (skip), so nothing is ever changed without a click on the first button.
export function ConfirmDialog({ request, onAnswer }: ConfirmDialogProps) {
  const t = useT();
  const label = (o: string) => (o === "continue" || o === "skip" ? t(`confirm.${o}`) : o);
  const decline = request ? request.options[request.options.length - 1] : "";
  const pickFromList = (request?.options.length ?? 0) > 2;

  return (
    <Dialog
      open={!!request}
      onClose={() => request && onAnswer(request.id, decline)}
      title={request?.title}
      className="max-w-lg"
    >
      <p className="whitespace-pre-line text-sm text-muted-foreground">{request?.body}</p>
      {pickFromList ? (
        // A choice between several things (which version of a mod to get):
        // one per row, none of them highlighted, so the app does not steer.
        <div className="mt-5 flex max-h-[50vh] flex-col gap-2 overflow-y-auto">
          {request?.options.map((o) => (
            <Button
              key={o}
              variant={o === decline ? "ghost" : "outline"}
              className="w-full justify-start"
              onClick={() => onAnswer(request.id, o)}
            >
              {label(o)}
            </Button>
          ))}
        </div>
      ) : (
        <div className="mt-5 flex justify-end gap-2">
          {request?.options.map((o, i) => (
            <Button
              key={o}
              variant={i === 0 ? "default" : "outline"}
              onClick={() => onAnswer(request.id, o)}
            >
              {label(o)}
            </Button>
          ))}
        </div>
      )}
    </Dialog>
  );
}
