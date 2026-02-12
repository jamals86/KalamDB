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
    <div className="flex items-center border-b border-slate-200 bg-slate-50 dark:border-[#1e293b] dark:bg-[#151e29]">
      <div className="flex min-w-0 flex-1 overflow-x-auto">
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => onTabSelect(tab.id)}
              className={cn(
                "group flex h-11 min-w-[168px] max-w-[260px] items-center gap-2 border-r border-slate-200 px-3 text-sm dark:border-[#1e293b]",
                isActive
                  ? "border-t-2 border-t-sky-500 bg-white text-slate-900 dark:bg-[#101922] dark:text-slate-100"
                  : "text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-[#1a2533]",
              )}
            >
              <FileCode2 className={cn("h-3.5 w-3.5 shrink-0", isActive ? "text-sky-500" : "text-slate-400")} />
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
                  className="ml-auto rounded p-0.5 opacity-0 transition group-hover:opacity-100 hover:bg-slate-200 dark:hover:bg-slate-700"
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
          className="flex h-11 w-11 shrink-0 items-center justify-center border-r border-slate-200 text-slate-500 transition hover:bg-slate-100 hover:text-slate-900 dark:border-[#1e293b] dark:text-slate-400 dark:hover:bg-[#1a2533] dark:hover:text-slate-100"
          title="New query tab"
        >
          <Plus className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
