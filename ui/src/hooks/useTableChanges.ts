/**
 * useTableChanges - Hook for tracking cell edits and row deletions in a data table.
 *
 * Tracks two kinds of changes:
 *   1. Cell edits: individual cell value modifications (marked yellow)
 *   2. Row deletions: entire rows marked for deletion (marked red)
 *
 * Provides helpers to check change status per-row/cell, apply/discard all
 * changes, and generate the list of raw changes for SQL generation.
 *
 * Usage:
 *   const changes = useTableChanges();
 *   changes.editCell(rowIndex, columnName, oldValue, newValue);
 *   changes.deleteRow(rowIndex, primaryKeyValues);
 *   changes.hasChanges; // boolean
 *   changes.getRowStatus(rowIndex); // 'deleted' | 'edited' | null
 *   changes.discardAll();
 */

import { useState, useCallback, useMemo } from 'react';

// ─── Types ───────────────────────────────────────────────────────────────────

/** Represents a single cell edit within a row. */
export interface CellEdit {
  columnName: string;
  oldValue: unknown;
  newValue: unknown;
}

/** Represents all edits for one row, keyed by column name. */
export interface RowEdit {
  rowIndex: number;
  /** The primary key column values used to build the WHERE clause. */
  primaryKeyValues: Record<string, unknown>;
  /** Cell-level edits, keyed by column name. */
  cellEdits: Record<string, CellEdit>;
}

/** Represents a row marked for deletion. */
export interface RowDeletion {
  rowIndex: number;
  /** The primary key column values used to build the WHERE clause. */
  primaryKeyValues: Record<string, unknown>;
  /** Optionally store the full row data for display purposes. */
  rowData?: Record<string, unknown>;
}

/** The full change-set tracked by this hook. */
export interface TableChangeSet {
  edits: Map<number, RowEdit>;       // keyed by rowIndex
  deletions: Map<number, RowDeletion>; // keyed by rowIndex
}

// ─── Hook ────────────────────────────────────────────────────────────────────

export function useTableChanges() {
  const [edits, setEdits] = useState<Map<number, RowEdit>>(new Map());
  const [deletions, setDeletions] = useState<Map<number, RowDeletion>>(new Map());

  // ── Mutations ────────────────────────────────────────────────────────────

  /** Mark a single cell as edited. */
  const editCell = useCallback(
    (
      rowIndex: number,
      columnName: string,
      oldValue: unknown,
      newValue: unknown,
      primaryKeyValues: Record<string, unknown>,
    ) => {
      setEdits((prev) => {
        const next = new Map(prev);
        const existing = next.get(rowIndex) ?? {
          rowIndex,
          primaryKeyValues,
          cellEdits: {},
        };
        // If the new value matches the original, remove the edit for that cell
        if (newValue === oldValue) {
          const { [columnName]: _, ...rest } = existing.cellEdits;
          if (Object.keys(rest).length === 0) {
            next.delete(rowIndex);
          } else {
            next.set(rowIndex, { ...existing, cellEdits: rest });
          }
        } else {
          next.set(rowIndex, {
            ...existing,
            cellEdits: {
              ...existing.cellEdits,
              [columnName]: { columnName, oldValue, newValue },
            },
          });
        }
        return next;
      });
    },
    [],
  );

  /** Mark (or unmark) an entire row for deletion. */
  const deleteRow = useCallback(
    (rowIndex: number, primaryKeyValues: Record<string, unknown>, rowData?: Record<string, unknown>) => {
      setDeletions((prev) => {
        const next = new Map(prev);
        if (next.has(rowIndex)) {
          // Toggle off
          next.delete(rowIndex);
        } else {
          next.set(rowIndex, { rowIndex, primaryKeyValues, rowData });
        }
        return next;
      });
      // Also remove any cell edits for a deleted row (they're irrelevant)
      setEdits((prev) => {
        const next = new Map(prev);
        next.delete(rowIndex);
        return next;
      });
    },
    [],
  );

  /** Undo deletion for a specific row. */
  const undeleteRow = useCallback((rowIndex: number) => {
    setDeletions((prev) => {
      const next = new Map(prev);
      next.delete(rowIndex);
      return next;
    });
  }, []);

  /** Undo edits for a specific row. */
  const undoRowEdits = useCallback((rowIndex: number) => {
    setEdits((prev) => {
      const next = new Map(prev);
      next.delete(rowIndex);
      return next;
    });
  }, []);

  /** Discard ALL changes. */
  const discardAll = useCallback(() => {
    setEdits(new Map());
    setDeletions(new Map());
  }, []);

  // ── Queries ──────────────────────────────────────────────────────────────

  /** Whether there are any pending changes. */
  const hasChanges = useMemo(() => edits.size > 0 || deletions.size > 0, [edits, deletions]);

  /** Total number of changed rows (edits + deletions, unique). */
  const changeCount = useMemo(() => {
    const allRows = new Set([...edits.keys(), ...deletions.keys()]);
    return allRows.size;
  }, [edits, deletions]);

  /** Return the status of a specific row. */
  const getRowStatus = useCallback(
    (rowIndex: number): 'deleted' | 'edited' | null => {
      if (deletions.has(rowIndex)) return 'deleted';
      if (edits.has(rowIndex)) return 'edited';
      return null;
    },
    [edits, deletions],
  );

  /** Check if a specific cell has been edited. */
  const isCellEdited = useCallback(
    (rowIndex: number, columnName: string): boolean => {
      return !!edits.get(rowIndex)?.cellEdits[columnName];
    },
    [edits],
  );

  /** Get the edited value for a cell, or undefined if not edited. */
  const getCellEditedValue = useCallback(
    (rowIndex: number, columnName: string): unknown | undefined => {
      return edits.get(rowIndex)?.cellEdits[columnName]?.newValue;
    },
    [edits],
  );

  return {
    // State
    edits,
    deletions,
    hasChanges,
    changeCount,

    // Mutations
    editCell,
    deleteRow,
    undeleteRow,
    undoRowEdits,
    discardAll,

    // Queries
    getRowStatus,
    isCellEdited,
    getCellEditedValue,
  };
}

export type TableChangesReturn = ReturnType<typeof useTableChanges>;
