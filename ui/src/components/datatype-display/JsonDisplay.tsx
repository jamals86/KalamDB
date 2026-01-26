import { useState } from 'react';
import { ChevronRight, ChevronDown } from 'lucide-react';

interface JsonDisplayProps {
  value: any;
  expanded?: boolean;
}

export function JsonDisplay({ value, expanded = false }: JsonDisplayProps) {
  const [isExpanded, setIsExpanded] = useState(expanded);

  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  if (typeof value !== 'object') {
    return <span className="font-mono text-sm">{JSON.stringify(value)}</span>;
  }

  const jsonString = JSON.stringify(value, null, 2);
  const preview = JSON.stringify(value).slice(0, 50);

  if (isExpanded) {
    return (
      <div className="max-w-full">
        <button
          onClick={(e) => {
            e.stopPropagation();
            setIsExpanded(false);
          }}
          className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground mb-1"
        >
          <ChevronDown className="h-3 w-3" />
          Collapse
        </button>
        <pre className="text-xs bg-muted p-2 rounded overflow-x-auto max-h-[200px] overflow-y-auto">
          {jsonString}
        </pre>
      </div>
    );
  }

  return (
    <button
      onClick={(e) => {
        e.stopPropagation();
        setIsExpanded(true);
      }}
      className="flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline max-w-full"
      title={jsonString}
    >
      <ChevronRight className="h-3 w-3 shrink-0" />
      <span className="font-mono truncate">{preview}{preview.length >= 50 ? '...' : ''}</span>
    </button>
  );
}
