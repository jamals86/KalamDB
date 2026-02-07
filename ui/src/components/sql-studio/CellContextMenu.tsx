/**
 * CellContextMenu - Right-click context menu for data table cells.
 *
 * Shows options to:
 *   - Edit cell value (opens inline edit)
 *   - Delete the entire row (marks for deletion)
 *   - Undo edit / Undo delete for already-changed rows
 *   - View / Copy cell value (existing behavior preserved)
 *
 * This component renders as a positioned overlay triggered by onContextMenu.
 * It should be rendered once in the parent and controlled via state.
 *
 * Usage:
 *   <CellContextMenu
 *     context={cellContextMenuState}
 *     onEdit={(rowIndex, columnName, currentValue) => { ... }}
 *     onDelete={(rowIndex) => { ... }}
 *     onUndoEdit={(rowIndex) => { ... }}
 *     onUndoDelete={(rowIndex) => { ... }}
 *     onCopyValue={(value) => { ... }}
 *     onViewData={(value) => { ... }}
 *     onClose={() => setCellContextMenuState(null)}
 *   />
 */

import { useEffect, useRef } from 'react';
import { Pencil, Trash2, Undo2, Eye, Copy } from 'lucide-react';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface CellContextMenuState {
  /** Screen X position for the menu. */
  x: number;
  /** Screen Y position for the menu. */
  y: number;
  /** Row index in the data array. */
  rowIndex: number;
  /** Column id / name. */
  columnName: string;
  /** Current cell value. */
  value: unknown;
  /** Current change status of this row. */
  rowStatus: 'deleted' | 'edited' | null;
  /** Whether this specific cell has been edited. */
  cellEdited: boolean;
}

interface CellContextMenuProps {
  context: CellContextMenuState | null;
  onEdit: (rowIndex: number, columnName: string, currentValue: unknown) => void;
  onDelete: (rowIndex: number) => void;
  onUndoEdit: (rowIndex: number) => void;
  onUndoDelete: (rowIndex: number) => void;
  onCopyValue: (value: unknown) => void;
  onViewData: (value: unknown) => void;
  onClose: () => void;
}

// ─── Component ───────────────────────────────────────────────────────────────

export function CellContextMenu({
  context,
  onEdit,
  onDelete,
  onUndoEdit,
  onUndoDelete,
  onCopyValue,
  onViewData,
  onClose,
}: CellContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  // Close on Escape key
  useEffect(() => {
    if (!context) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [context, onClose]);

  // Adjust menu position to stay within viewport
  useEffect(() => {
    if (!context || !menuRef.current) return;
    const rect = menuRef.current.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    if (rect.right > vw) {
      menuRef.current.style.left = `${context.x - rect.width}px`;
    }
    if (rect.bottom > vh) {
      menuRef.current.style.top = `${context.y - rect.height}px`;
    }
  }, [context]);

  if (!context) return null;

  const isDeleted = context.rowStatus === 'deleted';
  const isEdited = context.rowStatus === 'edited';
  const hasViewableData =
    context.value !== null &&
    (typeof context.value === 'object' ||
      (typeof context.value === 'string' && context.value.length > 40));

  return (
    <>
      {/* Backdrop */}
      <div className="fixed inset-0 z-40" onClick={onClose} />

      {/* Menu */}
      <div
        ref={menuRef}
        className="fixed z-50 bg-background border border-border rounded-md shadow-lg py-1 min-w-[180px] text-sm"
        style={{ left: context.x, top: context.y }}
      >
        {/* ── Header showing column name ─────────────────────────── */}
        <div className="px-3 py-1.5 text-xs text-muted-foreground border-b mb-1 font-mono">
          {context.columnName} · Row {context.rowIndex + 1}
        </div>

        {/* ── Edit cell ──────────────────────────────────────────── */}
        {!isDeleted && (
          <button
            className="w-full px-3 py-2 text-left hover:bg-muted flex items-center gap-2"
            onClick={() => {
              onEdit(context.rowIndex, context.columnName, context.value);
              onClose();
            }}
          >
            <Pencil className="h-3.5 w-3.5 text-amber-600" />
            Edit Cell
          </button>
        )}

        {/* ── Delete row ─────────────────────────────────────────── */}
        {!isDeleted && (
          <button
            className="w-full px-3 py-2 text-left hover:bg-muted flex items-center gap-2 text-red-600"
            onClick={() => {
              onDelete(context.rowIndex);
              onClose();
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
            Delete Row
          </button>
        )}

        {/* ── Undo delete ────────────────────────────────────────── */}
        {isDeleted && (
          <button
            className="w-full px-3 py-2 text-left hover:bg-muted flex items-center gap-2 text-blue-600"
            onClick={() => {
              onUndoDelete(context.rowIndex);
              onClose();
            }}
          >
            <Undo2 className="h-3.5 w-3.5" />
            Undo Delete
          </button>
        )}

        {/* ── Undo cell edits ────────────────────────────────────── */}
        {isEdited && !isDeleted && (
          <button
            className="w-full px-3 py-2 text-left hover:bg-muted flex items-center gap-2 text-blue-600"
            onClick={() => {
              onUndoEdit(context.rowIndex);
              onClose();
            }}
          >
            <Undo2 className="h-3.5 w-3.5" />
            Undo Row Edits
          </button>
        )}

        <div className="border-t my-1" />

        {/* ── View data (for large/complex values) ───────────────── */}
        {hasViewableData && (
          <button
            className="w-full px-3 py-2 text-left hover:bg-muted flex items-center gap-2"
            onClick={() => {
              onViewData(context.value);
              onClose();
            }}
          >
            <Eye className="h-3.5 w-3.5" />
            View Data
          </button>
        )}

        {/* ── Copy value ─────────────────────────────────────────── */}
        <button
          className="w-full px-3 py-2 text-left hover:bg-muted flex items-center gap-2"
          onClick={() => {
            onCopyValue(context.value);
            onClose();
          }}
        >
          <Copy className="h-3.5 w-3.5" />
          Copy Value
        </button>
      </div>
    </>
  );
}
