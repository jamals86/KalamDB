import { Button } from "@/components/ui/button";
import { Clock, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";

interface LiveQueryStatusBarProps {
  status: 'idle' | 'connecting' | 'connected' | 'error';
  onShowLog: () => void;
  onClearResults: () => void;
  rowCount: number;
}

export function LiveQueryStatusBar({ status, onShowLog, onClearResults, rowCount }: LiveQueryStatusBarProps) {
  return (
    <div className={cn(
      "px-4 py-2 border-b flex items-center justify-between shrink-0 transition-colors",
      status === "connected" && "bg-green-50 dark:bg-green-950/30",
      status === "connecting" && "bg-yellow-50 dark:bg-yellow-950/30",
      status === "error" && "bg-red-50 dark:bg-red-950/30",
    )}>
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2">
          {status === "connecting" && (
            <>
              <div className="animate-spin h-4 w-4 border-2 border-yellow-500 border-t-transparent rounded-full" />
              <span className="text-sm text-yellow-600 dark:text-yellow-400 font-medium">Connecting...</span>
            </>
          )}
          {status === "connected" && (
            <>
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
              </span>
              <span className="text-sm text-green-600 dark:text-green-400 font-medium">Live - Subscribed</span>
            </>
          )}
          {status === "error" && (
            <>
              <span className="h-2 w-2 rounded-full bg-red-500"></span>
              <span className="text-sm text-red-600 dark:text-red-400 font-medium">Disconnected</span>
            </>
          )}
        </div>
        
        <span className="text-sm text-muted-foreground">
          {rowCount} {rowCount === 1 ? "row" : "rows"} received
        </span>
      </div>
      
      <div className="flex items-center gap-2">
        <Button
          size="sm"
          variant="ghost"
          onClick={onShowLog}
          className="h-7 gap-1.5"
        >
          <Clock className="h-3.5 w-3.5" />
          View Log
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={onClearResults}
          className="h-7 gap-1.5"
        >
          <Trash2 className="h-3.5 w-3.5" />
          Clear
        </Button>
      </div>
    </div>
  );
}
