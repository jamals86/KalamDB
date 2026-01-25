import { Play, X, Clock, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

interface ToolbarButtonsProps {
  isLive: boolean;
  isLoading: boolean;
  subscriptionStatus: 'idle' | 'connecting' | 'connected' | 'error';
  hasQuery: boolean;
  onExecute: () => void;
  onToggleLive: () => void;
  onLiveChange: (value: boolean) => void;
  onShowHistory: () => void;
  onClearResults: () => void;
}

export function ToolbarButtons({
  isLive,
  isLoading,
  subscriptionStatus,
  hasQuery,
  onExecute,
  onToggleLive,
  onLiveChange,
  onShowHistory,
  onClearResults,
}: ToolbarButtonsProps) {
  return (
    <>
      <Button
        size="sm"
        onClick={isLive ? onToggleLive : onExecute}
        disabled={isLoading || !hasQuery || subscriptionStatus === "connecting"}
        className={cn(
          "gap-1.5 h-8",
          isLive && subscriptionStatus === "connected"
            ? "bg-red-600 hover:bg-red-700 text-white"
            : "bg-blue-600 hover:bg-blue-700 text-white"
        )}
      >
        {subscriptionStatus === "connecting" ? (
          <>
            <div className="animate-spin h-3.5 w-3.5 border-2 border-white border-t-transparent rounded-full" />
            Connecting...
          </>
        ) : isLive && subscriptionStatus === "connected" ? (
          <>
            <X className="h-3.5 w-3.5" />
            Stop
          </>
        ) : isLive ? (
          <>
            <Play className="h-3.5 w-3.5" />
            Subscribe
          </>
        ) : (
          <>
            <Play className="h-3.5 w-3.5" />
            Run
          </>
        )}
      </Button>

      <div className="flex items-center gap-2 ml-2">
        <label className="text-sm text-muted-foreground cursor-pointer flex items-center gap-2">
          <Switch
            checked={isLive}
            onCheckedChange={onLiveChange}
            disabled={subscriptionStatus === "connecting" || subscriptionStatus === "connected"}
          />
          Live Query
        </label>
      </div>

      <div className="ml-auto flex items-center gap-2">
        <Button
          size="sm"
          variant="ghost"
          onClick={onShowHistory}
          className="gap-1.5 h-8"
        >
          <Clock className="h-3.5 w-3.5" />
          History
        </Button>
        <Button
          size="sm"
          variant="ghost"
          onClick={onClearResults}
          className="gap-1.5 h-8"
        >
          <Trash2 className="h-3.5 w-3.5" />
          Clear
        </Button>
      </div>
    </>
  );
}
