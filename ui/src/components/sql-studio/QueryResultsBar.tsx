import { Check, XCircle, Download } from "lucide-react";
import { Button } from "@/components/ui/button";

interface QueryResultsBarProps {
  success: boolean;
  rowCount: number | null;
  executionTime: number | null;
  executedAs: string | null;
  error: string | null;
  onExport: () => void;
}

export function QueryResultsBar({
  success,
  rowCount,
  executionTime,
  executedAs,
  error,
  onExport,
}: QueryResultsBarProps) {
  // Don't show if no results yet
  if (rowCount === null && executionTime === null && !error) {
    return null;
  }

  return (
    <div className="px-4 py-2 border-b flex items-center justify-between bg-muted/30 shrink-0">
      <div className="flex items-center gap-4">
        {/* Status indicator */}
        <div className="flex items-center gap-1.5">
          {success && !error ? (
            <>
              <Check className="h-4 w-4 text-emerald-600" />
              <span className="text-sm font-medium text-emerald-600">Success</span>
            </>
          ) : (
            <>
              <XCircle className="h-4 w-4 text-red-500" />
              <span className="text-sm font-medium text-red-500">Error</span>
            </>
          )}
        </div>

        {/* Row count */}
        {rowCount !== null && (
          <span className="text-sm text-muted-foreground">
            {rowCount.toLocaleString()} row{rowCount !== 1 ? "s" : ""} returned
          </span>
        )}

        {/* Execution time */}
        {executionTime !== null && (
          <span className="text-sm text-muted-foreground">
            Time: {executionTime}ms
          </span>
        )}

        {executedAs && (
          <span className="text-sm text-muted-foreground">
            As: {executedAs}
          </span>
        )}
      </div>

      {/* Export button */}
      {success && rowCount !== null && rowCount > 0 && (
        <Button
          size="sm"
          variant="outline"
          onClick={onExport}
          className="h-7 gap-1.5 text-xs"
        >
          <Download className="h-3.5 w-3.5" />
          Export
        </Button>
      )}
    </div>
  );
}
