import { useState, useMemo } from "react";
import Editor from "@monaco-editor/react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { executeQuery, type QueryResponse } from "@/lib/kalam-client";
import { Play, Loader2, AlertCircle, CheckCircle2 } from "lucide-react";
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  flexRender,
  type ColumnDef,
  type SortingState,
} from "@tanstack/react-table";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Input } from "@/components/ui/input";

interface QueryState {
  result: QueryResponse | null;
  error: string | null;
  isLoading: boolean;
  executionTime: number | null;
}

export default function SqlStudio() {
  const [sql, setSql] = useState("SELECT * FROM system.users LIMIT 10;");
  const [queryState, setQueryState] = useState<QueryState>({
    result: null,
    error: null,
    isLoading: false,
    executionTime: null,
  });
  const [sorting, setSorting] = useState<SortingState>([]);
  const [globalFilter, setGlobalFilter] = useState("");

  const handleExecute = async () => {
    setQueryState({
      result: null,
      error: null,
      isLoading: true,
      executionTime: null,
    });

    const startTime = performance.now();

    try {
      const response = await executeQuery(sql);
      const endTime = performance.now();
      
      if (response.status === 'error' && response.error) {
        setQueryState({
          result: null,
          error: response.error.message,
          isLoading: false,
          executionTime: endTime - startTime,
        });
      } else {
        setQueryState({
          result: response,
          error: null,
          isLoading: false,
          executionTime: response.took ?? (endTime - startTime),
        });
      }
    } catch (err) {
      const endTime = performance.now();
      setQueryState({
        result: null,
        error: err instanceof Error ? err.message : "Query execution failed",
        isLoading: false,
        executionTime: endTime - startTime,
      });
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      handleExecute();
    }
  };

  // Get first result set from response
  const firstResult = queryState.result?.results?.[0] ?? null;

  // SDK returns rows as Record<string, any>[]
  const tableData = useMemo(() => {
    return (firstResult?.rows as Record<string, unknown>[]) ?? [];
  }, [firstResult?.rows]);

  // Generate columns dynamically from result
  const columns = useMemo<ColumnDef<Record<string, unknown>>[]>(() => {
    if (!firstResult?.columns) return [];
    
    return firstResult.columns.map((colName) => ({
      accessorKey: colName,
      header: colName,
      cell: ({ getValue }) => {
        const value = getValue();
        if (value === null || value === undefined) {
          return <span className="text-muted-foreground italic">NULL</span>;
        }
        if (typeof value === 'object') {
          return <span className="font-mono text-xs">{JSON.stringify(value)}</span>;
        }
        return String(value);
      },
    }));
  }, [firstResult?.columns]);

  const table = useReactTable({
    data: tableData,
    columns,
    state: {
      sorting,
      globalFilter,
    },
    onSortingChange: setSorting,
    onGlobalFilterChange: setGlobalFilter,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    initialState: {
      pagination: {
        pageSize: 50,
      },
    },
  });

  const totalRows = firstResult?.row_count ?? 0;
  const displayedRows = table.getFilteredRowModel().rows.length;

  return (
    <div className="flex h-full flex-col gap-4">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">SQL Studio</h1>
        <Button onClick={handleExecute} disabled={queryState.isLoading}>
          {queryState.isLoading ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <Play className="mr-2 h-4 w-4" />
          )}
          Execute (Ctrl+Enter)
        </Button>
      </div>

      <Card className="flex-shrink-0">
        <CardHeader className="py-3">
          <CardTitle className="text-sm">Query Editor</CardTitle>
        </CardHeader>
        <CardContent className="p-0">
          <div className="h-64 border-t" onKeyDown={handleKeyDown}>
            <Editor
              height="100%"
              language="sql"
              value={sql}
              onChange={(value) => setSql(value || "")}
              options={{
                minimap: { enabled: false },
                fontSize: 14,
                lineNumbers: "on",
                scrollBeyondLastLine: false,
                automaticLayout: true,
              }}
              theme="vs-dark"
            />
          </div>
        </CardContent>
      </Card>

      {queryState.error && (
        <Card className="border-destructive bg-destructive/5">
          <CardContent className="flex items-start gap-3 py-4">
            <AlertCircle className="h-5 w-5 text-destructive flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-medium text-destructive">Query Error</p>
              <p className="text-sm text-destructive/80 mt-1">{queryState.error}</p>
              {queryState.executionTime && (
                <p className="text-xs text-muted-foreground mt-2">
                  Failed after {queryState.executionTime.toFixed(2)}ms
                </p>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {firstResult && (
        <Card className="flex-1 flex flex-col min-h-0">
          <CardHeader className="py-3 flex-shrink-0">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <CheckCircle2 className="h-4 w-4 text-green-500" />
                <CardTitle className="text-sm">
                  Results: {displayedRows === totalRows ? totalRows : `${displayedRows} of ${totalRows}`} rows
                  {queryState.executionTime && ` • ${queryState.executionTime.toFixed(2)}ms`}
                </CardTitle>
              </div>
              <Input
                placeholder="Filter results..."
                value={globalFilter}
                onChange={(e) => setGlobalFilter(e.target.value)}
                className="max-w-xs h-8"
              />
            </div>
          </CardHeader>
          <CardContent className="p-0 flex-1 overflow-hidden flex flex-col">
            <div className="flex-1 overflow-auto border-t">
              <Table>
                <TableHeader className="sticky top-0 bg-muted z-10">
                  {table.getHeaderGroups().map((headerGroup) => (
                    <TableRow key={headerGroup.id}>
                      {headerGroup.headers.map((header) => (
                        <TableHead
                          key={header.id}
                          className="cursor-pointer select-none hover:bg-muted/80 whitespace-nowrap"
                          onClick={header.column.getToggleSortingHandler()}
                        >
                          <div className="flex items-center gap-1">
                            {flexRender(
                              header.column.columnDef.header,
                              header.getContext()
                            )}
                            {{
                              asc: " ↑",
                              desc: " ↓",
                            }[header.column.getIsSorted() as string] ?? null}
                          </div>
                        </TableHead>
                      ))}
                    </TableRow>
                  ))}
                </TableHeader>
                <TableBody>
                  {table.getRowModel().rows.length > 0 ? (
                    table.getRowModel().rows.map((row) => (
                      <TableRow key={row.id} className="hover:bg-muted/50">
                        {row.getVisibleCells().map((cell) => (
                          <TableCell key={cell.id} className="py-2 whitespace-nowrap">
                            {flexRender(
                              cell.column.columnDef.cell,
                              cell.getContext()
                            )}
                          </TableCell>
                        ))}
                      </TableRow>
                    ))
                  ) : (
                    <TableRow>
                      <TableCell
                        colSpan={columns.length}
                        className="h-24 text-center text-muted-foreground"
                      >
                        No results
                      </TableCell>
                    </TableRow>
                  )}
                </TableBody>
              </Table>
            </div>
            
            {/* Pagination */}
            {table.getPageCount() > 1 && (
              <div className="flex items-center justify-between px-4 py-2 border-t bg-muted/30">
                <div className="text-sm text-muted-foreground">
                  Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()}
                </div>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => table.previousPage()}
                    disabled={!table.getCanPreviousPage()}
                  >
                    Previous
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => table.nextPage()}
                    disabled={!table.getCanNextPage()}
                  >
                    Next
                  </Button>
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
