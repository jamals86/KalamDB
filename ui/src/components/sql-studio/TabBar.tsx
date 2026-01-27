import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface QueryTab {
  id: string;
  name: string;
  query: string;
  subscriptionStatus?: 'idle' | 'connecting' | 'connected' | 'error';
  isLive?: boolean;
}

interface TabBarProps {
  tabs: QueryTab[];
  activeTabId: string;
  onTabChange: (tabId: string) => void;
  onAddTab: () => void;
  onCloseTab: (tabId: string, e?: React.MouseEvent) => void;
}

export function TabBar({ tabs, activeTabId, onTabChange, onAddTab, onCloseTab }: TabBarProps) {
  return (
    <div className="flex items-center gap-1">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          onClick={() => onTabChange(tab.id)}
          className={cn(
            "flex items-center gap-1 px-3 py-1.5 rounded cursor-pointer text-sm transition-colors",
            activeTabId === tab.id
              ? "bg-muted font-medium"
              : "hover:bg-muted/50"
          )}
        >
          {/* Green dot for live subscribed tabs */}
          {tab.subscriptionStatus === "connected" && (
            <span className="relative flex h-2 w-2 mr-1">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
            </span>
          )}
          {tab.name}
          {tabs.length > 1 && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCloseTab(tab.id, e);
              }}
              className="ml-1 hover:bg-muted-foreground/20 rounded p-0.5 transition-colors"
            >
              <X className="h-3 w-3" />
            </button>
          )}
        </div>
      ))}
      <Button size="sm" variant="ghost" onClick={onAddTab} className="h-7 w-7 p-0">
        <Plus className="h-4 w-4" />
      </Button>
    </div>
  );
}
