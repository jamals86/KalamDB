import { Clock3, Columns3 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { QueryRunSummary, StudioTable } from "./types";

interface StudioInspectorPanelProps {
  selectedTable: StudioTable | null;
  history: QueryRunSummary[];
}

export function StudioInspectorPanel({
  selectedTable,
  history,
}: StudioInspectorPanelProps) {
  return (
    <div className="flex h-full flex-col border-l border-slate-200 bg-[#f8fafc] dark:border-[#1e293b] dark:bg-[#151e29]">
      <Tabs defaultValue="details" className="flex h-full flex-col">
        <div className="border-b border-slate-200 px-2 py-2 dark:border-[#1e293b]">
          <TabsList className="grid h-8 w-full grid-cols-2 bg-transparent">
            <TabsTrigger value="details">Details</TabsTrigger>
            <TabsTrigger value="history">History</TabsTrigger>
          </TabsList>
        </div>

        <TabsContent value="details" className="m-0 flex-1 overflow-hidden">
          <ScrollArea className="h-full p-3">
            {!selectedTable && (
              <p className="text-sm text-muted-foreground">
                Select a table from Explorer to inspect schema details.
              </p>
            )}

            {selectedTable && (
              <div className="space-y-3">
                <div>
                  <p className="text-xs uppercase tracking-[0.16em] text-slate-500 dark:text-slate-400">Table Information</p>
                  <p className="text-sm font-semibold text-slate-800 dark:text-slate-100">
                    {selectedTable.namespace}.{selectedTable.name}
                  </p>
                  <Badge variant="secondary" className="mt-1 text-[10px] uppercase">
                    {selectedTable.tableType}
                  </Badge>
                </div>

                <div className="space-y-2">
                  <p className="text-xs uppercase tracking-[0.16em] text-slate-500 dark:text-slate-400">Table Schema</p>
                  {selectedTable.columns.map((column) => (
                    <div key={column.name} className="rounded-md border border-slate-200 bg-white p-2 dark:border-[#1e293b] dark:bg-[#0d141c]">
                      <div className="flex items-center justify-between gap-2">
                        <span className="truncate text-sm font-medium text-slate-800 dark:text-slate-200">{column.name}</span>
                        <span className="text-[10px] uppercase text-slate-500 dark:text-slate-400">{column.dataType}</span>
                      </div>
                      <div className="mt-1 flex items-center gap-2 text-[10px] text-slate-500 dark:text-slate-400">
                        {column.isPrimaryKey && <span>Primary Key</span>}
                        <span>{column.isNullable ? "Nullable" : "Not Null"}</span>
                      </div>
                    </div>
                  ))}
                </div>

                <div className="space-y-1.5 rounded-md border border-slate-200 bg-white p-3 text-xs dark:border-[#1e293b] dark:bg-[#0d141c]">
                  <p className="uppercase tracking-[0.16em] text-slate-500 dark:text-slate-400">Options + Version</p>
                  <p className="text-slate-600 dark:text-slate-300">Current version: v1</p>
                  <p className="text-slate-600 dark:text-slate-300">Read only in this phase</p>
                </div>
              </div>
            )}
          </ScrollArea>
        </TabsContent>

        <TabsContent value="history" className="m-0 flex-1 overflow-hidden">
          <ScrollArea className="h-full p-3">
            <div className="space-y-2">
              {history.length === 0 && (
                <p className="text-sm text-muted-foreground">No query executions yet.</p>
              )}

              {history.map((entry) => (
                <div key={entry.id} className="rounded-md border border-slate-200 bg-white p-2 dark:border-[#1e293b] dark:bg-[#0d141c]">
                  <div className="flex items-center justify-between gap-2">
                    <p className="truncate text-xs font-semibold">{entry.tabTitle}</p>
                    <Badge variant={entry.status === "success" ? "secondary" : "outline"}>
                      {entry.status}
                    </Badge>
                  </div>
                  <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{entry.sql}</p>
                  <div className="mt-2 flex items-center gap-3 text-[10px] text-muted-foreground">
                    <span className="inline-flex items-center gap-1">
                      <Clock3 className="h-3 w-3" />
                      {new Date(entry.executedAt).toLocaleTimeString()}
                    </span>
                    <span>{entry.durationMs} ms</span>
                    <span>{entry.rowCount} rows</span>
                  </div>
                  {entry.errorMessage && (
                    <p className="mt-2 text-xs text-destructive">{entry.errorMessage}</p>
                  )}
                </div>
              ))}
            </div>
          </ScrollArea>
        </TabsContent>
      </Tabs>

      <div className="space-y-2 border-t border-slate-200 bg-white px-3 py-3 text-xs dark:border-[#1e293b] dark:bg-[#0d141c]">
        <span className="inline-flex items-center gap-1 text-slate-500 dark:text-slate-400">
          <Columns3 className="h-3 w-3" />
          1 pending edit
        </span>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" className="h-7 flex-1 text-xs">Discard</Button>
          <Button size="sm" className="h-7 flex-1 bg-[#137fec] text-xs text-white hover:bg-[#0f6cbd]">Commit</Button>
        </div>
      </div>
    </div>
  );
}
