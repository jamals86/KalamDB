import { FileCode2, Plus, X } from "lucide-react";
import { cn } from "@/lib/utils";
import type { QueryTab } from "./types";

interface QueryTabStripProps {
  tabs: QueryTab[];
  activeTabId: string;
  onTabSelect: (tabId: string) => void;
  onAddTab: () => void;
  onCloseTab: (tabId: string) => void;
}

export function QueryTabStrip({
  tabs,
  activeTabId,
  onTabSelect,
  onAddTab,
  onCloseTab,
}: QueryTabStripProps) {
  return (
    <div className="flex items-center border-b border-border bg-background">
      <div className="flex min-w-0 flex-1 overflow-x-auto">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => onTabSelect(tab.id)}
              className={cn(
                "group flex h-11 min-w-[168px] max-w-[260px] items-center gap-2 border-r border-border px-3 text-sm ",
                isActive
                  ? "border-t-2 border-t-sky-500 bg-background text-foreground "
                  : "text-muted-foreground hover:bg-muted :bg-accent",
              )}
            >
              <FileCode2 className={cn("h-3.5 w-3.5 shrink-0", isActive ? "text-primary" : "text-muted-foreground")} />
              <span className="truncate text-left">{tab.title}</span>
              {tab.liveStatus === "connected" && (
                <span className="relative ml-1 flex h-2.5 w-2.5 shrink-0">
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                  <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-emerald-500" />
                </span>
              )}
              {tab.isLive && tab.liveStatus !== "connected" && (
                <span className="ml-1 h-2 w-2 shrink-0 rounded-full bg-sky-400/80" />
              )}
              {tab.isDirty && <span className="ml-auto h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />}
              {tabs.length > 1 && (
                <span
                  onClick={(event) => {
                    event.stopPropagation();
                    onCloseTab(tab.id);
                  }}
                  className="ml-auto rounded p-0.5 opacity-0 transition group-hover:opacity-100 hover:bg-border :bg-muted"
                >
                  <X className="h-3.5 w-3.5" />
                </span>
              )}
            </button>
          );
        })}

        <button
          type="button"
          onClick={onAddTab}
          className="flex h-11 w-11 shrink-0 items-center justify-center border-r border-border text-muted-foreground transition hover:bg-muted hover:text-foreground :bg-accent :text-foreground"
          title="New query tab"
        >
          <Plus className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
