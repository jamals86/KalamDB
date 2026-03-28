import { Loader2, Wifi, WifiOff } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { useBackendStatus } from "@/lib/backend-status";
import { cn } from "@/lib/utils";

function formatTarget(origin: string): string {
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}

export default function BackendStatusFooter() {
  const { state, message, targetOrigin, usingWebSocket } = useBackendStatus();
  const targetLabel = formatTarget(targetOrigin);
  const isOnline = state === "online";
  const isChecking = state === "checking";

  return (
    <footer className="border-t border-border bg-card/90 px-4 py-2 text-xs text-card-foreground backdrop-blur md:px-6">
      <div className="flex items-center gap-3">
        <div className="flex items-center gap-2 font-medium">
          <span
            className={cn(
              "h-2.5 w-2.5 rounded-full",
              isOnline && "bg-emerald-500",
              isChecking && "bg-amber-400",
              state === "offline" && "bg-destructive",
            )}
          />
          <span>{isOnline ? "Online" : isChecking ? "Connecting" : "Offline"}</span>
        </div>
        <Badge variant="outline" className="hidden sm:inline-flex">
          {usingWebSocket ? "WebSocket" : "HTTP probe"}
        </Badge>
        <span className="hidden truncate text-muted-foreground sm:inline">{targetLabel}</span>
        <span className="ml-auto flex min-w-0 items-center gap-1.5 text-muted-foreground">
          {isChecking ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : isOnline ? (
            <Wifi className="h-3.5 w-3.5" />
          ) : (
            <WifiOff className="h-3.5 w-3.5" />
          )}
          <span className="truncate">{message}</span>
        </span>
      </div>
    </footer>
  );
}