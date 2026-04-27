import { useEffect, useState, type ReactNode } from "react";
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

interface DestructiveConfirmDialogProps {
  open: boolean;
  title: string;
  description: ReactNode;
  expected: string;
  confirmLabel: string;
  warning?: ReactNode;
  onConfirm: () => void;
  onClose: () => void;
}

export function DestructiveConfirmDialog({
  open,
  title,
  description,
  expected,
  confirmLabel,
  warning,
  onConfirm,
  onClose,
}: DestructiveConfirmDialogProps) {
  const [typed, setTyped] = useState("");

  useEffect(() => {
    if (open) setTyped("");
  }, [open]);

  const matches = typed === expected;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <AlertTriangle className="h-4 w-4" />
            {title}
          </DialogTitle>
          <DialogDescription asChild>
            <div className="text-sm text-muted-foreground">{description}</div>
          </DialogDescription>
        </DialogHeader>

        {warning}

        <label className="flex flex-col gap-1.5 text-xs">
          <span className="text-muted-foreground">
            Type <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs text-foreground">{expected}</code> to confirm:
          </span>
          <Input
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={expected}
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
            disabled={!matches}
            onClick={() => {
              if (matches) onConfirm();
            }}
          >
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
