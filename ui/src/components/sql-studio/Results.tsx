import { useMemo, useState } from 'react';
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  flexRender,
  ColumnDef,
  SortingState,
  ColumnFiltersState,
} from '@tanstack/react-table';
import { QueryResult } from '../../lib/api';

interface ResultsProps {
  result: QueryResult | null;
  isLoading?: boolean;
}

const MAX_DISPLAY_ROWS = 10000;

export function Results({ result, isLoading }: ResultsProps) {
  const [sorting, setSorting] = useState<SortingState>([]);
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [globalFilter, setGlobalFilter] = useState('');

  const columns = useMemo<ColumnDef<unknown[]>[]>(() => {
    if (!result?.columns) return [];
    
    return result.columns.map((col, index) => ({
      id: `col_${index}`,
      accessorFn: (row: unknown[]) => row[index],
      header: () => (
        <div className="flex flex-col">
          <span className="font-semibold">{col.name}</span>
          <span className="text-xs text-gray-400 font-normal">{col.data_type}</span>
        </div>
      ),
      cell: ({ getValue }) => {
        const value = getValue();
        if (value === null) {
          return <span className="text-gray-400 italic">NULL</span>;
        }
        if (typeof value === 'boolean') {
          return <span className={value ? 'text-green-600' : 'text-red-600'}>{String(value)}</span>;
        }
        if (typeof value === 'object') {
          return <span className="text-blue-600 font-mono text-xs">{JSON.stringify(value)}</span>;
        }
        return <span className="font-mono text-sm">{String(value)}</span>;
      },
    }));
  }, [result?.columns]);

  const data = useMemo(() => {
    if (!result?.rows) return [];
    return result.rows.slice(0, MAX_DISPLAY_ROWS);
  }, [result?.rows]);

  const table = useReactTable({
    data,
    columns,
    state: {
      sorting,
      columnFilters,
      globalFilter,
    },
    onSortingChange: setSorting,
    onColumnFiltersChange: setColumnFilters,
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

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full p-8">
        <div className="flex items-center gap-2 text-gray-500">
          <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
          <span>Executing query...</span>
        </div>
      </div>
    );
  }

  if (!result) {
    return (
      <div className="flex items-center justify-center h-full p-8 text-gray-400">
        Run a query to see results
      </div>
    );
  }

  const showTruncationWarning = result.truncated || result.rows.length > MAX_DISPLAY_ROWS;

  return (
    <div className="flex flex-col h-full border rounded-lg overflow-hidden">
      {/* Header with stats and filter */}
      <div className="flex items-center justify-between p-2 bg-gray-50 border-b">
        <div className="flex items-center gap-4">
          <span className="text-sm text-gray-600">
            {result.row_count.toLocaleString()} row{result.row_count !== 1 ? 's' : ''}
          </span>
          {showTruncationWarning && (
            <span className="text-xs text-amber-600 bg-amber-50 px-2 py-1 rounded">
              ⚠️ Results limited to {MAX_DISPLAY_ROWS.toLocaleString()} rows
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <input
            type="text"
            placeholder="Filter results..."
            value={globalFilter}
            onChange={(e) => setGlobalFilter(e.target.value)}
            className="px-2 py-1 text-sm border rounded"
          />
        </div>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-sm">
          <thead className="bg-gray-50 sticky top-0">
            {table.getHeaderGroups().map((headerGroup) => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <th
                    key={header.id}
                    className="px-3 py-2 text-left border-b cursor-pointer hover:bg-gray-100"
                    onClick={header.column.getToggleSortingHandler()}
                  >
                    <div className="flex items-center gap-1">
                      {flexRender(header.column.columnDef.header, header.getContext())}
                      {{
                        asc: ' ↑',
                        desc: ' ↓',
                      }[header.column.getIsSorted() as string] ?? null}
                    </div>
                  </th>
                ))}
              </tr>
            ))}
          </thead>
          <tbody>
            {table.getRowModel().rows.map((row) => (
              <tr key={row.id} className="hover:bg-gray-50">
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} className="px-3 py-2 border-b">
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Pagination */}
      <div className="flex items-center justify-between p-2 bg-gray-50 border-t">
        <div className="flex items-center gap-2">
          <button
            onClick={() => table.setPageIndex(0)}
            disabled={!table.getCanPreviousPage()}
            className="px-2 py-1 text-sm border rounded disabled:opacity-50"
          >
            {'<<'}
          </button>
          <button
            onClick={() => table.previousPage()}
            disabled={!table.getCanPreviousPage()}
            className="px-2 py-1 text-sm border rounded disabled:opacity-50"
          >
            {'<'}
          </button>
          <button
            onClick={() => table.nextPage()}
            disabled={!table.getCanNextPage()}
            className="px-2 py-1 text-sm border rounded disabled:opacity-50"
          >
            {'>'}
          </button>
          <button
            onClick={() => table.setPageIndex(table.getPageCount() - 1)}
            disabled={!table.getCanNextPage()}
            className="px-2 py-1 text-sm border rounded disabled:opacity-50"
          >
            {'>>'}
          </button>
        </div>
        <span className="text-sm text-gray-600">
          Page {table.getState().pagination.pageIndex + 1} of {table.getPageCount()}
        </span>
        <select
          value={table.getState().pagination.pageSize}
          onChange={(e) => table.setPageSize(Number(e.target.value))}
          className="px-2 py-1 text-sm border rounded"
        >
          {[25, 50, 100, 200].map((pageSize) => (
            <option key={pageSize} value={pageSize}>
              Show {pageSize}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
