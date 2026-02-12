import { useMemo, useState } from "react";
import { Braces, Expand } from "lucide-react";
import { CodeBlock } from "@/components/ui/code-block";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";

interface JsonDisplayProps {
  value: unknown;
  maxPreviewLength?: number;
}

function normalizeJsonValue(value: unknown): unknown {
  if (typeof value !== "string") {
    return value;
  }

  const trimmed = value.trim();
  if (
    (trimmed.startsWith("{") && trimmed.endsWith("}")) ||
    (trimmed.startsWith("[") && trimmed.endsWith("]"))
  ) {
    try {
      return JSON.parse(trimmed);
    } catch {
      return value;
    }
  }

  return value;
}

function buildPreview(value: unknown, maxPreviewLength: number): string {
  try {
    const compact = typeof value === "string" ? value : JSON.stringify(value);
    if (!compact) {
      return "{}";
    }
    return compact.length > maxPreviewLength
      ? `${compact.slice(0, maxPreviewLength)}...`
      : compact;
  } catch {
    const fallback = String(value);
    return fallback.length > maxPreviewLength
      ? `${fallback.slice(0, maxPreviewLength)}...`
      : fallback;
  }
}

export function JsonDisplay({ value, maxPreviewLength = 30 }: JsonDisplayProps) {
  const [isOpen, setIsOpen] = useState(false);
  const normalizedValue = useMemo(() => normalizeJsonValue(value), [value]);
  const preview = useMemo(
    () => buildPreview(normalizedValue, maxPreviewLength),
    [normalizedValue, maxPreviewLength],
  );

  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  return (
    <>
      <button
        type="button"
        onClick={(event) => {
          event.stopPropagation();
          setIsOpen(true);
        }}
        className="inline-flex max-w-full items-center gap-1.5 rounded px-1 py-0.5 text-xs text-sky-500 hover:bg-sky-500/10 hover:text-sky-400"
        title="View JSON data"
      >
        <Braces className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate font-mono">{preview}</span>
      </button>

      <Dialog open={isOpen} onOpenChange={setIsOpen}>
        <DialogContent className="max-w-4xl overflow-hidden">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Expand className="h-4 w-4" />
              JSON Data
            </DialogTitle>
          </DialogHeader>
          <div className="max-h-[70vh] overflow-auto">
            <CodeBlock value={normalizedValue} jsonPreferred maxHeightClassName="max-h-[68vh]" />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
