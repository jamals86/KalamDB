import { useState } from "react";
import {
  flexRender,
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  type ColumnDef,
  type SortingState,
} from "@tanstack/react-table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { CellDisplay } from "@/components/datatype-display";
import {
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
} from "lucide-react";
import { QueryResultsBar } from "./QueryResultsBar";
import { LiveQueryStatusBar } from "./LiveQueryStatusBar";
import { cn } from "@/lib/utils";

interface ResultsPanelProps {
  results: Record<string, unknown>[] | null;
  columns: ColumnDef<Record<string, unknown>>[];
  schema: { name: string; data_type: string; index: number }[] | null;
  error: string | null;
  isLoading: boolean;
  isLive: boolean;
  subscriptionStatus: 'idle' | 'connecting' | 'connected' | 'error';
  executionTime: number | null;
  rowCount: number | null;
  message: string | null;
  onExport: () => void;
  onShowLog: () => void;
  onClearResults: () => void;
}

export function ResultsPanel({
  results,
  columns,
  schema,
  error,
  isLoading,
  isLive,
  subscriptionStatus,
  executionTime,
  rowCount,
  message,
  onExport,
  onShowLog,
  onClearResults,
}: ResultsPanelProps) {
  const [sorting, setSorting] = useState<SortingState>([]);

  const table = useReactTable({
    data: results ?? [],
    columns: columns.length > 0 ? columns : [],
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: {
      pagination: {
        pageSize: 100,
      },
    },
  });

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Status Bars */}
      {!isLive && (results !== null || error) && (
        <QueryResultsBar
          success={!error}
          rowCount={rowCount}
          executionTime={executionTime}
          error={error}
          onExport={onExport}
        />
      )}

      {isLive && subscriptionStatus !== 'idle' && (
        <LiveQueryStatusBar
          status={subscriptionStatus}
          onShowLog={onShowLog}
          onClearResults={onClearResults}
          rowCount={rowCount ?? 0}
        />
      )}

      {/* Results Content */}
      <div className="flex-1 overflow-hidden">
        {isLoading && (
          <div className="flex items-center justify-center h-full">
            <div className="animate-spin h-8 w-8 border-4 border-primary border-t-transparent rounded-full" />
          </div>
        )}

        {!isLoading && error && (
          <div className="flex items-center justify-center h-full p-4">
            <div className="max-w-2xl w-full bg-destructive/10 text-destructive rounded-lg p-4">
              <h3 className="font-semibold mb-2">Query Error</h3>
              <pre className="text-sm whitespace-pre-wrap font-mono">{error}</pre>
            </div>
          </div>
        )}

        {!isLoading && !error && message && !results?.length && (
          <div className="flex items-center justify-center h-full p-4">
            <div className="max-w-2xl w-full bg-primary/10 text-primary rounded-lg p-4">
              <h3 className="font-semibold mb-2">Success</h3>
              <p className="text-sm">{message}</p>
            </div>
          </div>
        )}

        {!isLoading && !error && results && results.length > 0 && (
          <div className="h-full flex flex-col">
            {/* Table */}
            <ScrollArea className="flex-1">
              <div className="min-w-full">
                <table className="w-full border-collapse">
                  <thead className="bg-muted/50 sticky top-0 z-10">
                    {table.getHeaderGroups().map((headerGroup) => (
                      <tr key={headerGroup.id}>
                        {headerGroup.headers.map((header) => (
                          <th
                            key={header.id}
                            className="border-b border-r px-3 py-2 text-left text-xs font-semibold"
                          >
                            {header.isPlaceholder
                              ? null
                              : flexRender(
                                  header.column.columnDef.header,
                                  header.getContext()
                                )}
                          </th>
                        ))}
                      </tr>
                    ))}
                  </thead>
                  <tbody>
                    {table.getRowModel().rows.map((row, idx) => (
                      <tr
                        key={row.id}
                        className={cn(
                          "hover:bg-muted/50",
                          idx % 2 === 0 ? "bg-background" : "bg-muted/20"
                        )}
                      >
                        {row.getVisibleCells().map((cell) => {
                          const columnName = cell.column.id;
                          const schemaField = schema?.find((s) => s.name === columnName);
                          const value = cell.getValue();

                          return (
                            <td
                              key={cell.id}
                              className="border-b border-r px-3 py-2 text-sm"
                            >
                              {schemaField?.data_type ? (
                                <CellDisplay
                                  value={value}
                                  dataType={schemaField.data_type}
                                />
                              ) : (
                                flexRender(
                                  cell.column.columnDef.cell,
                                  cell.getContext()
                                )
                              )}
                            </td>
                          );
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </ScrollArea>

            {/* Pagination */}
            {results.length > 0 && (
              <div className="border-t px-4 py-2 flex items-center justify-between shrink-0 bg-muted/30">
                <div className="flex items-center gap-2">
                  <span className="text-sm text-muted-foreground">
                    Rows per page:
                  </span>
                  <Select
                    value={String(table.getState().pagination.pageSize)}
                    onValueChange={(value) => table.setPageSize(Number(value))}
                  >
                    <SelectTrigger className="h-8 w-20">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {[50, 100, 200, 500].map((size) => (
                        <SelectItem key={size} value={String(size)}>
                          {size}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-sm text-muted-foreground">
                    Page {table.getState().pagination.pageIndex + 1} of{" "}
                    {table.getPageCount()}
                  </span>
                  <div className="flex gap-1">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => table.setPageIndex(0)}
                      disabled={!table.getCanPreviousPage()}
                      className="h-8 w-8 p-0"
                    >
                      <ChevronsLeft className="h-4 w-4" />
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => table.previousPage()}
                      disabled={!table.getCanPreviousPage()}
                      className="h-8 w-8 p-0"
                    >
                      <ChevronLeft className="h-4 w-4" />
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => table.nextPage()}
                      disabled={!table.getCanNextPage()}
                      className="h-8 w-8 p-0"
                    >
                      <ChevronRight className="h-4 w-4" />
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => table.setPageIndex(table.getPageCount() - 1)}
                      disabled={!table.getCanNextPage()}
                      className="h-8 w-8 p-0"
                    >
                      <ChevronsRight className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </div>
        )}

        {!isLoading && !error && !message && results?.length === 0 && (
          <div className="flex items-center justify-center h-full text-muted-foreground">
            <p>No results to display</p>
          </div>
        )}

        {!isLoading && !error && !results && (
          <div className="flex items-center justify-center h-full text-muted-foreground">
            <p>Run a query to see results</p>
          </div>
        )}
      </div>
    </div>
  );
}
