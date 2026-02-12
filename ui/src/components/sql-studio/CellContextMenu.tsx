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
import { cn } from "@/lib/utils";

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
  const itemClassName =
    "relative flex w-full cursor-default select-none items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-left outline-none transition-colors hover:bg-accent hover:text-accent-foreground disabled:pointer-events-none disabled:opacity-50";

  return (
    <>
      {/* Backdrop */}
      <div className="fixed inset-0 z-40" onClick={onClose} />

      {/* Menu */}
      <div
        ref={menuRef}
        className="fixed z-50 min-w-[11rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground opacity-100 shadow-md antialiased backdrop-blur-none"
        style={{ left: context.x, top: context.y }}
      >
        {/* ── Header showing column name ─────────────────────────── */}
        <div className="px-2 py-1.5 text-xs font-medium text-muted-foreground">
          {context.columnName} · Row {context.rowIndex + 1}
        </div>
        <div className="-mx-1 my-1 h-px bg-muted" />

        {/* ── Edit cell ──────────────────────────────────────────── */}
        {!isDeleted && (
          <button
            className={itemClassName}
            onClick={() => {
              onEdit(context.rowIndex, context.columnName, context.value);
              onClose();
            }}
          >
            <Pencil className="h-3.5 w-3.5 text-foreground/80" />
            Edit Cell
          </button>
        )}

        {/* ── Delete row ─────────────────────────────────────────── */}
        {!isDeleted && (
          <button
            className={cn(itemClassName, "text-destructive hover:bg-destructive/10 hover:text-destructive")}
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
            className={itemClassName}
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
            className={itemClassName}
            onClick={() => {
              onUndoEdit(context.rowIndex);
              onClose();
            }}
          >
            <Undo2 className="h-3.5 w-3.5" />
            Undo Row Edits
          </button>
        )}

        <div className="-mx-1 my-1 h-px bg-muted" />

        {/* ── View data (for large/complex values) ───────────────── */}
        <button
          className={itemClassName}
          disabled={!hasViewableData}
          onClick={() => {
            if (!hasViewableData) {
              return;
            }
            onViewData(context.value);
            onClose();
          }}
        >
          <Eye className="h-3.5 w-3.5" />
          View Data
        </button>

        {/* ── Copy value ─────────────────────────────────────────── */}
        <button
          className={itemClassName}
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
