import { cn } from "@/lib/utils";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";

interface CodeBlockProps {
  value: unknown;
  className?: string;
  maxHeightClassName?: string;
  jsonPreferred?: boolean;
}

interface NormalizedCode {
  text: string;
  isJson: boolean;
}

function normalizeCode(value: unknown, jsonPreferred: boolean): NormalizedCode {
  if (value === null || value === undefined) {
    return { text: "null", isJson: true };
  }

  if (typeof value === "string") {
    const trimmed = value.trim();
    const maybeJson =
      jsonPreferred ||
      ((trimmed.startsWith("{") && trimmed.endsWith("}")) ||
        (trimmed.startsWith("[") && trimmed.endsWith("]")));

    if (maybeJson) {
      try {
        const parsed = JSON.parse(value);
        return { text: JSON.stringify(parsed, null, 2), isJson: true };
      } catch {
        return { text: value, isJson: false };
      }
    }
    return { text: value, isJson: false };
  }

  if (typeof value === "object") {
    try {
      return { text: JSON.stringify(value, null, 2), isJson: true };
    } catch {
      return { text: String(value), isJson: false };
    }
  }

  return { text: String(value), isJson: false };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function highlightJson(json: string): string {
  const escaped = escapeHtml(json);
  return escaped.replace(
    /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(\.\d+)?([eE][+\-]?\d+)?)/g,
    (match) => {
      if (/^".*":$/.test(match)) {
        return `<span class="text-sky-400">${match}</span>`;
      }
      if (/^"/.test(match)) {
        return `<span class="text-emerald-400">${match}</span>`;
      }
      if (/true|false/.test(match)) {
        return `<span class="text-violet-400">${match}</span>`;
      }
      if (/null/.test(match)) {
        return `<span class="text-slate-500 italic">${match}</span>`;
      }
      return `<span class="text-amber-400">${match}</span>`;
    },
  );
}

export function CodeBlock({
  value,
  className,
  maxHeightClassName = "max-h-[60vh]",
  jsonPreferred = false,
}: CodeBlockProps) {
  const normalized = normalizeCode(value, jsonPreferred);
  const highlighted = normalized.isJson ? highlightJson(normalized.text) : null;

  return (
    <div className={cn("rounded-md border border-slate-700 bg-black h-full flex flex-col", className)}>
      <ScrollArea className={cn("w-full flex-1", maxHeightClassName)}>
        {normalized.isJson && highlighted ? (
          <pre className="whitespace-pre p-3 font-mono text-xs leading-5 text-slate-200">
            <code dangerouslySetInnerHTML={{ __html: highlighted }} />
          </pre>
        ) : (
          <pre className="whitespace-pre-wrap p-3 font-mono text-xs leading-5 text-slate-200">
            {normalized.text}
          </pre>
        )}
        <ScrollBar orientation="vertical" />
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
    </div>
  );
}
