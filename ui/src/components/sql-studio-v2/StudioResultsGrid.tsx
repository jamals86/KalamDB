import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronsLeft,
  ChevronsRight,
  ChevronLeft,
  ChevronRight,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useTableChanges } from "@/hooks/useTableChanges";
import { CellContextMenu, type CellContextMenuState } from "@/components/sql-studio/CellContextMenu";
import { InlineCellEditor, type InlineEditContext } from "@/components/sql-studio/InlineCellEditor";
import { generateSqlStatements } from "@/components/sql-studio/utils/sqlGenerator";
import { extractTableContext } from "@/components/sql-studio/utils/sqlParser";
import { useSqlPreview } from "@/components/sql-preview";
import { executeSql } from "@/lib/kalam-client";
import { cn } from "@/lib/utils";
import { StudioExecutionLog } from "./StudioExecutionLog";
import type { QueryResultData, SqlStudioResultView, StudioTable } from "./types";

interface StudioResultsGridProps {
  result: QueryResultData | null;
  isRunning: boolean;
  activeSql: string;
  selectedTable: StudioTable | null;
  currentUsername: string;
  resultView: SqlStudioResultView;
  onResultViewChange: (view: SqlStudioResultView) => void;
  onRefreshAfterCommit: () => void;
}

type SortDirection = "asc" | "desc";
type SortState = { columnName: string; direction: SortDirection } | null;
type RowData = Record<string, unknown>;
type SelectedCell = { rowIndex: number; columnName: string } | null;

interface CellViewerState {
  open: boolean;
  title: string;
  content: string;
}

const PAGE_SIZE = 50;
const MAX_RENDERED_ROWS = 1000;
const CELL_PREVIEW_MAX_LENGTH = 120;

function previewCellValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "null";
  }

  if (typeof value === "string") {
    return value.length > CELL_PREVIEW_MAX_LENGTH
      ? `${value.slice(0, CELL_PREVIEW_MAX_LENGTH)}...`
      : value;
  }

  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }

  if (Array.isArray(value)) {
    return `[${value.length} item${value.length === 1 ? "" : "s"}]`;
  }

  if (typeof value === "object") {
    const keys = Object.keys(value as Record<string, unknown>);
    if (keys.length === 0) {
      return "{}";
    }
    const previewKeys = keys.slice(0, 3).join(", ");
    return `{${previewKeys}${keys.length > 3 ? ", ..." : ""}}`;
  }

  return String(value);
}

function stringifyCellValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "null";
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return String(value);
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function compareValues(left: unknown, right: unknown): number {
  if (left === right) {
    return 0;
  }
  if (left === null || left === undefined) {
    return 1;
  }
  if (right === null || right === undefined) {
    return -1;
  }

  const leftType = typeof left;
  const rightType = typeof right;

  if (leftType === "number" && rightType === "number") {
    return (left as number) - (right as number);
  }

  if (leftType === "boolean" && rightType === "boolean") {
    return Number(left) - Number(right);
  }

  return String(left).localeCompare(String(right), undefined, { numeric: true, sensitivity: "base" });
}

function formatValueForViewer(value: unknown, dataType?: string): string {
  const normalizedType = dataType?.toLowerCase() ?? "";
  const isJsonType = normalizedType.includes("json");

  if (value === null || value === undefined) {
    return "null";
  }

  if (isJsonType) {
    if (typeof value === "string") {
      try {
        return JSON.stringify(JSON.parse(value), null, 2);
      } catch {
        return value;
      }
    }
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return stringifyCellValue(value);
    }
  }

  if (typeof value === "object") {
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return stringifyCellValue(value);
    }
  }

  return stringifyCellValue(value);
}

export function StudioResultsGrid({
  result,
  isRunning,
  activeSql,
  selectedTable,
  currentUsername,
  resultView,
  onResultViewChange,
  onRefreshAfterCommit,
}: StudioResultsGridProps) {
  const [sortState, setSortState] = useState<SortState>(null);
  const [pageIndex, setPageIndex] = useState(0);
  const [selectedRows, setSelectedRows] = useState<Set<number>>(new Set());
  const [selectedCell, setSelectedCell] = useState<SelectedCell>(null);
  const [cellContextMenu, setCellContextMenu] = useState<CellContextMenuState | null>(null);
  const [inlineEditContext, setInlineEditContext] = useState<InlineEditContext | null>(null);
  const [cellViewer, setCellViewer] = useState<CellViewerState>({
    open: false,
    title: "",
    content: "",
  });

  const {
    edits,
    deletions,
    changeCount,
    getRowStatus,
    isCellEdited,
    getCellEditedValue,
    editCell,
    deleteRow,
    undeleteRow,
    undoRowEdits,
    discardAll,
  } = useTableChanges();
  const { openSqlPreview } = useSqlPreview();

  useEffect(() => {
    discardAll();
    setSortState(null);
    setPageIndex(0);
    setSelectedRows(new Set());
    setSelectedCell(null);
    setCellContextMenu(null);
    setInlineEditContext(null);
    setCellViewer({
      open: false,
      title: "",
      content: "",
    });
  }, [result, discardAll]);

  const schema = result?.schema ?? [];
  const sourceRows =
    result?.status === "success"
      ? result.rows.slice(0, MAX_RENDERED_ROWS)
      : [];

  const sortedRowIndices = useMemo(() => {
    const indices = sourceRows.map((_, rowIndex) => rowIndex);
    if (!sortState) {
      return indices;
    }

    const { columnName, direction } = sortState;
    indices.sort((leftIndex, rightIndex) => {
      const leftValue = getCellEditedValue(leftIndex, columnName) ?? sourceRows[leftIndex]?.[columnName];
      const rightValue = getCellEditedValue(rightIndex, columnName) ?? sourceRows[rightIndex]?.[columnName];
      const comparison = compareValues(leftValue, rightValue);
      return direction === "asc" ? comparison : comparison * -1;
    });
    return indices;
  }, [sourceRows, sortState, edits, getCellEditedValue]);

  const pageCount = Math.max(1, Math.ceil(sortedRowIndices.length / PAGE_SIZE));
  const currentPageStart = pageIndex * PAGE_SIZE;
  const currentPageRows = sortedRowIndices.slice(currentPageStart, currentPageStart + PAGE_SIZE);
  const columnNames = useMemo(() => schema.map((field) => field.name), [schema]);
  const selectedCellKey = selectedCell ? `${selectedCell.rowIndex}:${selectedCell.columnName}` : null;

  useEffect(() => {
    if (pageIndex > pageCount - 1) {
      setPageIndex(Math.max(0, pageCount - 1));
    }
  }, [pageIndex, pageCount]);

  useEffect(() => {
    if (!selectedCell) {
      return;
    }

    if (!currentPageRows.includes(selectedCell.rowIndex)) {
      setSelectedCell(null);
    }
  }, [selectedCell, currentPageRows]);

  const selectedVisibleRowCount = currentPageRows.filter((rowIndex) => selectedRows.has(rowIndex)).length;
  const allVisibleRowsSelected =
    currentPageRows.length > 0 && selectedVisibleRowCount === currentPageRows.length;

  const getPrimaryKeyValues = (rowIndex: number): Record<string, unknown> => {
    const row = sourceRows[rowIndex];
    if (!row) {
      return {};
    }

    const selectedPkColumns = (selectedTable?.columns ?? [])
      .filter((column) => column.isPrimaryKey)
      .map((column) => column.name);

    const pkColumns =
      selectedPkColumns.length > 0
        ? selectedPkColumns
        : schema
            .map((field) => field.name)
            .filter((name) => name.toLowerCase() === "id" || name.endsWith("_id"));

    const fallbackColumn = schema[0]?.name;
    const keyColumns = pkColumns.length > 0 ? pkColumns : fallbackColumn ? [fallbackColumn] : [];

    return keyColumns.reduce<Record<string, unknown>>((acc, columnName) => {
      acc[columnName] = row[columnName];
      return acc;
    }, {});
  };

  const handleSortColumn = (columnName: string) => {
    setSortState((previous) => {
      if (!previous || previous.columnName !== columnName) {
        return { columnName, direction: "asc" };
      }
      if (previous.direction === "asc") {
        return { columnName, direction: "desc" };
      }
      return null;
    });
    setPageIndex(0);
  };

  const handleEditCell = (rowIndex: number, columnName: string, currentValue: unknown) => {
    const cell = document.querySelector(
      `[data-row-index="${rowIndex}"][data-column-name="${columnName}"]`,
    ) as HTMLElement | null;

    if (!cell) {
      return;
    }

    setInlineEditContext({
      rowIndex,
      columnName,
      value: currentValue,
      rect: cell.getBoundingClientRect(),
    });
  };

  const handleSaveInlineEdit = (
    rowIndex: number,
    columnName: string,
    oldValue: unknown,
    newValue: unknown,
  ) => {
    const pkValues = getPrimaryKeyValues(rowIndex);
    editCell(rowIndex, columnName, oldValue, newValue, pkValues);
    setInlineEditContext(null);
  };

  const handleDeleteRow = (rowIndex: number) => {
    const row = sourceRows[rowIndex];
    if (!row) {
      return;
    }

    const pkValues = getPrimaryKeyValues(rowIndex);
    deleteRow(rowIndex, pkValues, row);
    setSelectedRows((previous) => {
      const next = new Set(previous);
      next.delete(rowIndex);
      return next;
    });
  };

  const handleDeleteSelectedRows = () => {
    const targetRows = Array.from(selectedRows);
    targetRows.forEach((rowIndex) => {
      const row = sourceRows[rowIndex];
      if (!row) {
        return;
      }
      const pkValues = getPrimaryKeyValues(rowIndex);
      deleteRow(rowIndex, pkValues, row);
    });
    setSelectedRows(new Set());
  };

  const openCellViewer = useCallback(
    (value: unknown, columnName: string, rowIndex: number) => {
      const dataType = schema.find((field) => field.name === columnName)?.dataType;
      setCellViewer({
        open: true,
        title: `${columnName} · Row ${rowIndex + 1}${dataType ? ` (${dataType})` : ""}`,
        content: formatValueForViewer(value, dataType),
      });
    },
    [schema],
  );

  const moveSelectionByArrow = useCallback(
    (rowDelta: number, colDelta: number) => {
      if (!selectedCell || currentPageRows.length === 0 || columnNames.length === 0) {
        return;
      }

      const currentRowPosition = currentPageRows.indexOf(selectedCell.rowIndex);
      const currentColumnPosition = columnNames.findIndex((name) => name === selectedCell.columnName);

      if (currentRowPosition < 0 || currentColumnPosition < 0) {
        return;
      }

      const nextRowPosition = Math.max(0, Math.min(currentPageRows.length - 1, currentRowPosition + rowDelta));
      const nextColumnPosition = Math.max(0, Math.min(columnNames.length - 1, currentColumnPosition + colDelta));
      const nextRowIndex = currentPageRows[nextRowPosition];
      const nextColumnName = columnNames[nextColumnPosition];
      setSelectedCell({
        rowIndex: nextRowIndex,
        columnName: nextColumnName,
      });

      const nextCell = document.querySelector(
        `[data-row-index="${nextRowIndex}"][data-column-name="${nextColumnName}"]`,
      ) as HTMLElement | null;
      nextCell?.scrollIntoView({ block: "nearest", inline: "nearest" });
      nextCell?.focus();
    },
    [selectedCell, currentPageRows, columnNames],
  );

  useEffect(() => {
    if (!selectedCell) {
      return;
    }

    const handleArrowNavigation = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        return;
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        moveSelectionByArrow(-1, 0);
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        moveSelectionByArrow(1, 0);
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        moveSelectionByArrow(0, -1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        moveSelectionByArrow(0, 1);
      }
    };

    window.addEventListener("keydown", handleArrowNavigation);
    return () => window.removeEventListener("keydown", handleArrowNavigation);
  }, [selectedCell, moveSelectionByArrow]);

  const handleReviewChanges = () => {
    const parsed = extractTableContext(activeSql);
    const namespace = selectedTable?.namespace ?? parsed?.namespace;
    const tableName = selectedTable?.name ?? parsed?.tableName;

    if (!namespace || !tableName) {
      alert("Unable to determine target table for commit. Select a table and run a simple SELECT ... FROM namespace.table query.");
      return;
    }

    const generated = generateSqlStatements(namespace, tableName, edits, deletions);
    if (generated.statements.length === 0) {
      return;
    }

    openSqlPreview({
      title: "Review Changes",
      description: `${generated.updateCount} update(s), ${generated.deleteCount} delete(s)`,
      sql: generated.fullSql,
      onExecute: async (statement: string) => {
        await executeSql(statement);
      },
      onComplete: async () => {
        discardAll();
        onRefreshAfterCommit();
      },
      onDiscard: () => {
        discardAll();
      },
    });
  };

  const editCount = edits.size;
  const deleteCount = deletions.size;
  const isSuccess = !isRunning && result?.status === "success";
  const hasTabularResults = isSuccess && schema.length > 0;
  const logCount = result?.logs.length ?? 0;
  const showTableEditorBars = hasTabularResults && resultView === "results";

  return (
    <div className="flex h-full min-h-0 flex-col bg-white dark:bg-[#101922]">
      <div className="flex h-10 items-center justify-between gap-2 border-b border-slate-200 px-3 text-xs dark:border-[#1e293b]">
        <Tabs
          value={resultView}
          onValueChange={(value) => onResultViewChange(value as SqlStudioResultView)}
          className="shrink-0"
        >
          <TabsList className="h-7 rounded-md bg-slate-100 p-0.5 dark:bg-[#16283d]">
            <TabsTrigger
              value="results"
              className="h-6 rounded px-2.5 text-[11px] data-[state=active]:bg-white dark:data-[state=active]:bg-[#101922]"
            >
              Results
            </TabsTrigger>
            <TabsTrigger
              value="log"
              className="h-6 rounded px-2.5 text-[11px] data-[state=active]:bg-white dark:data-[state=active]:bg-[#101922]"
            >
              Log ({logCount})
            </TabsTrigger>
          </TabsList>
        </Tabs>
        {isSuccess && (
          <div className="ml-auto flex h-full min-w-0 flex-nowrap items-center gap-2 overflow-x-auto whitespace-nowrap text-[11px] text-slate-500 dark:text-slate-400">
            <span>{result.rowCount.toLocaleString()} rows</span>
            <span>took {Math.round(result.tookMs)} ms</span>
            <span>as user: {currentUsername}</span>
            {result.rowCount > MAX_RENDERED_ROWS && (
              <span className="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] text-amber-700 dark:text-amber-300">
                showing first {MAX_RENDERED_ROWS.toLocaleString()}
              </span>
            )}
            {resultView === "results" && selectedRows.size > 0 && (
              <span className="rounded bg-sky-500/20 px-1.5 py-0.5 text-[10px] text-sky-700 dark:text-sky-300">
                {selectedRows.size} selected
              </span>
            )}
            {resultView === "results" && sortState && (
              <span className="rounded bg-slate-200 px-1.5 py-0.5 text-[10px] text-slate-700 dark:bg-slate-700 dark:text-slate-200">
                {sortState.columnName} ({sortState.direction})
              </span>
            )}
            <Button
              size="sm"
              variant="destructive"
              className={cn("h-7 gap-1.5", (resultView !== "results" || selectedRows.size === 0) && "invisible pointer-events-none")}
              onClick={handleDeleteSelectedRows}
              disabled={resultView !== "results" || selectedRows.size === 0}
            >
              <Trash2 className="h-3.5 w-3.5" />
              Delete selected
            </Button>
          </div>
        )}
      </div>

      {isRunning && (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          Running query...
        </div>
      )}

      {!isRunning && !result && (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          Run your query to view results.
        </div>
      )}

      {!isRunning && result?.status === "error" && resultView === "results" && (
        <div className="m-3 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
          {result.errorMessage ?? "Query failed"}
        </div>
      )}

      {!isRunning && result?.status === "error" && resultView === "log" && (
        <StudioExecutionLog logs={result.logs} status={result.status} />
      )}

      {!isRunning && result?.status === "success" && resultView === "log" && (
        <StudioExecutionLog logs={result.logs} status={result.status} />
      )}

      {!isRunning && result?.status === "success" && resultView === "results" && !hasTabularResults && (
        <div className="flex flex-1 items-center justify-center px-4 text-sm text-slate-500 dark:text-slate-400">
          No tabular result set for this execution. Open the Log tab to inspect statement output.
        </div>
      )}

      {!isRunning && showTableEditorBars && (
        <>
          <div className="flex h-10 items-center justify-between border-b border-slate-200 bg-amber-50/70 px-3 dark:border-[#1e293b] dark:bg-amber-950/20">
            <div className="truncate text-xs text-amber-700 dark:text-amber-300">
              {changeCount === 0
                ? "No pending table changes"
                : `${changeCount} change${changeCount === 1 ? "" : "s"} • ${editCount} edit${editCount === 1 ? "" : "s"} • ${deleteCount} delete${deleteCount === 1 ? "" : "s"}`}
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                variant="ghost"
                onClick={discardAll}
                disabled={changeCount === 0}
                className="h-7 gap-1.5 text-amber-700 hover:bg-amber-100 hover:text-amber-900 dark:text-amber-300 dark:hover:bg-amber-900/40 dark:hover:text-amber-200"
              >
                Discard
              </Button>
              <Button
                size="sm"
                onClick={handleReviewChanges}
                disabled={changeCount === 0}
                className="h-7 gap-1.5"
              >
                Review & Commit
              </Button>
            </div>
          </div>

          <ScrollArea className="min-h-0 flex-1">
            <div className="min-w-max">
              <table className="min-w-max border-collapse">
              <thead className="sticky top-0 z-10">
                <tr>
                  <th className="w-10 border-r border-slate-200 bg-slate-50 px-2 py-2 text-left dark:border-[#1e293b] dark:bg-[#151e29]">
                    <div className="flex items-center justify-center">
                      <input
                        type="checkbox"
                        checked={allVisibleRowsSelected}
                        onChange={(event) => {
                          setSelectedRows((previous) => {
                            const next = new Set(previous);
                            if (event.target.checked) {
                              currentPageRows.forEach((rowIndex) => next.add(rowIndex));
                            } else {
                              currentPageRows.forEach((rowIndex) => next.delete(rowIndex));
                            }
                            return next;
                          });
                        }}
                        className="h-3.5 w-3.5 rounded border-slate-500 bg-transparent"
                      />
                    </div>
                  </th>
                  {schema.map((field) => {
                    const isSorted = sortState?.columnName === field.name;
                    return (
                      <th
                        key={field.name}
                        className="border-r border-slate-200 bg-slate-50 px-2 py-2 text-left dark:border-[#1e293b] dark:bg-[#151e29]"
                      >
                        <button
                          type="button"
                          className="flex items-start gap-1 text-left"
                          onClick={() => handleSortColumn(field.name)}
                        >
                          <span>
                            <span className="block text-[11px] uppercase tracking-wide">{field.name}</span>
                            <span className="block text-[10px] font-normal uppercase text-slate-500 dark:text-slate-400">
                              {field.dataType}
                            </span>
                          </span>
                          {isSorted && (
                            sortState?.direction === "asc"
                              ? <ArrowUp className="mt-0.5 h-3 w-3 text-sky-400" />
                              : <ArrowDown className="mt-0.5 h-3 w-3 text-sky-400" />
                          )}
                        </button>
                      </th>
                    );
                  })}
                </tr>
              </thead>

              <tbody>
                {currentPageRows.map((rowIndex) => {
                  const row = sourceRows[rowIndex] as RowData | undefined;
                  if (!row) {
                    return null;
                  }

                  const rowStatus = getRowStatus(rowIndex);
                  const rowSelected = selectedRows.has(rowIndex);

                  return (
                    <tr
                      key={rowIndex}
                      className={cn(
                        "border-b border-slate-200 dark:border-[#1e293b]",
                        rowSelected && "bg-sky-500/10",
                        rowStatus === "edited" && "bg-amber-500/5",
                        rowStatus === "deleted" && "bg-red-500/10 opacity-60",
                      )}
                    >
                      <td className="border-r border-slate-200 px-2 py-1 dark:border-[#1e293b]">
                        <div className="flex items-center justify-center">
                          <input
                            type="checkbox"
                            checked={rowSelected}
                            onChange={(event) => {
                              setSelectedRows((previous) => {
                                const next = new Set(previous);
                                if (event.target.checked) {
                                  next.add(rowIndex);
                                } else {
                                  next.delete(rowIndex);
                                }
                                return next;
                              });
                            }}
                            className="h-3.5 w-3.5 rounded border-slate-500 bg-transparent"
                          />
                        </div>
                      </td>

                      {schema.map((field) => {
                        const value = getCellEditedValue(rowIndex, field.name) ?? row[field.name];
                        const cellEdited = isCellEdited(rowIndex, field.name);
                        const cellKey = `${rowIndex}:${field.name}`;
                        const preview = previewCellValue(value);

                        return (
                          <td key={`${rowIndex}-${field.name}`} className="border-r border-slate-200 px-1 py-1 dark:border-[#1e293b]">
                            <div
                              data-row-index={rowIndex}
                              data-column-name={field.name}
                              onClick={() => {
                                setSelectedCell({
                                  rowIndex,
                                  columnName: field.name,
                                });
                              }}
                              onDoubleClick={() => {
                                openCellViewer(value, field.name, rowIndex);
                              }}
                              onContextMenu={(event) => {
                                event.preventDefault();
                                setSelectedCell({
                                  rowIndex,
                                  columnName: field.name,
                                });
                                setCellContextMenu({
                                  x: event.clientX,
                                  y: event.clientY,
                                  rowIndex,
                                  columnName: field.name,
                                  value,
                                  rowStatus,
                                  cellEdited,
                                });
                              }}
                              className={cn(
                                "min-h-6 truncate px-1 py-0.5 font-mono text-xs outline-none",
                                value === null && "italic text-slate-500",
                                cellEdited && "bg-amber-500/20",
                                selectedCellKey === cellKey && "ring-1 ring-sky-400",
                              )}
                              tabIndex={0}
                            >
                              {preview}
                            </div>
                          </td>
                        );
                      })}
                    </tr>
                  );
                })}
              </tbody>
              </table>
            </div>
            <ScrollBar orientation="horizontal" />
          </ScrollArea>

          <div className="flex items-center justify-between border-t border-slate-200 bg-[#151e29] px-3 py-2 text-xs text-slate-400 dark:border-[#1e293b]">
            <div className="flex items-center gap-1">
              <Button
                size="icon"
                variant="ghost"
                className="h-7 w-7"
                onClick={() => setPageIndex(0)}
                disabled={pageIndex === 0}
              >
                <ChevronsLeft className="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-7 w-7"
                onClick={() => setPageIndex((previous) => Math.max(0, previous - 1))}
                disabled={pageIndex === 0}
              >
                <ChevronLeft className="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-7 w-7"
                onClick={() => setPageIndex((previous) => Math.min(pageCount - 1, previous + 1))}
                disabled={pageIndex >= pageCount - 1}
              >
                <ChevronRight className="h-3.5 w-3.5" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-7 w-7"
                onClick={() => setPageIndex(Math.max(0, pageCount - 1))}
                disabled={pageIndex >= pageCount - 1}
              >
                <ChevronsRight className="h-3.5 w-3.5" />
              </Button>
            </div>

            <span>
              Page {pageIndex + 1} of {pageCount}
            </span>
          </div>

          <CellContextMenu
            context={cellContextMenu}
            onEdit={handleEditCell}
            onDelete={handleDeleteRow}
            onUndoEdit={undoRowEdits}
            onUndoDelete={undeleteRow}
            onViewData={(value) => {
              const columnName = cellContextMenu?.columnName ?? "value";
              const rowIndex = cellContextMenu?.rowIndex ?? 0;
              openCellViewer(value, columnName, rowIndex);
            }}
            onCopyValue={(value) => {
              navigator.clipboard.writeText(stringifyCellValue(value)).catch(console.error);
            }}
            onClose={() => setCellContextMenu(null)}
          />

          <InlineCellEditor
            context={inlineEditContext}
            onSave={handleSaveInlineEdit}
            onCancel={() => setInlineEditContext(null)}
          />

          <Dialog
            open={cellViewer.open}
            onOpenChange={(open) => setCellViewer((previous) => ({ ...previous, open }))}
          >
            <DialogContent className="max-w-3xl">
              <DialogHeader>
                <DialogTitle>{cellViewer.title}</DialogTitle>
              </DialogHeader>
              <ScrollArea className="max-h-[60vh] rounded border border-slate-200 dark:border-[#1e293b]">
                <pre className="whitespace-pre-wrap p-3 font-mono text-xs text-slate-800 dark:text-slate-100">
                  {cellViewer.content}
                </pre>
                <ScrollBar orientation="horizontal" />
              </ScrollArea>
            </DialogContent>
          </Dialog>
        </>
      )}
    </div>
  );
}
