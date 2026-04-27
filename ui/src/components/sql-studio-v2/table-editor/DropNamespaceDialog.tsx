import { useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface DropNamespaceDialogProps {
  open: boolean;
  namespace: string;
  tableCount: number;
  onSubmit: (cascade: boolean) => void;
  onClose: () => void;
}

export function DropNamespaceDialog({
  open,
  namespace,
  tableCount,
  onSubmit,
  onClose,
}: DropNamespaceDialogProps) {
  const [cascade, setCascade] = useState(false);
  const [typed, setTyped] = useState("");

  useEffect(() => {
    if (open) {
      setCascade(false);
      setTyped("");
    }
  }, [open]);

  const isEmpty = tableCount === 0;
  const cascadeOk = isEmpty || cascade;
  const typeMatches = typed === namespace;
  const canDrop = cascadeOk && typeMatches;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <AlertTriangle className="h-4 w-4" />
            Drop namespace "{namespace}"
          </DialogTitle>
          <DialogDescription>
            {isEmpty ? (
              <>This namespace is empty. Deleting it cannot be undone.</>
            ) : (
              <>
                This namespace contains <strong>{tableCount}</strong>{" "}
                {tableCount === 1 ? "table" : "tables"}. Without CASCADE the drop will fail.
              </>
            )}
          </DialogDescription>
        </DialogHeader>

        {!isEmpty && (
          <label className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs">
            <input
              type="checkbox"
              checked={cascade}
              onChange={(e) => setCascade(e.target.checked)}
              className="mt-0.5 h-3.5 w-3.5 cursor-pointer rounded border-border accent-red-500"
            />
            <span className="space-y-0.5">
              <span className="block font-medium text-destructive">
                CASCADE — also delete all {tableCount} {tableCount === 1 ? "table" : "tables"} in this namespace
              </span>
              <span className="block text-muted-foreground">
                All data in those tables will be permanently lost. This cannot be undone.
              </span>
            </span>
          </label>
        )}

        <label className="flex flex-col gap-1.5 text-xs">
          <span className="text-muted-foreground">
            Type <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs text-foreground">{namespace}</code> to confirm:
          </span>
          <Input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={namespace}
            disabled={!cascadeOk}
            autoFocus
            className="font-mono"
          />
        </label>

        <DialogFooter className="gap-2 sm:gap-2">
          <Button type="button" variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={!canDrop}
            onClick={() => onSubmit(cascade)}
          >
            Drop {!isEmpty && cascade ? "with CASCADE" : "namespace"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
