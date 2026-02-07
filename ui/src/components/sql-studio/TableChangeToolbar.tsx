/**
 * TableChangeToolbar - A floating toolbar that appears above the data table
 * whenever there are pending cell edits or row deletions.
 *
 * Shows:
 *   - A status summary (e.g. "3 changes: 2 edits, 1 delete")
 *   - A "Save" button that opens the SQL Preview Dialog
 *   - A "Discard" button that reverts all pending changes
 *
 * Usage:
 *   <TableChangeToolbar
 *     changeCount={3}
 *     editCount={2}
 *     deleteCount={1}
 *     onSave={() => openSqlPreview(...)}
 *     onDiscard={() => changes.discardAll()}
 *   />
 */

import { Save, Undo2, AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';

interface TableChangeToolbarProps {
  /** Total number of rows with changes. */
  changeCount: number;
  /** Number of edited rows. */
  editCount: number;
  /** Number of rows marked for deletion. */
  deleteCount: number;
  /** Called when user clicks Save to review SQL. */
  onSave: () => void;
  /** Called when user clicks Discard to revert all changes. */
  onDiscard: () => void;
}

export function TableChangeToolbar({
  changeCount,
  editCount,
  deleteCount,
  onSave,
  onDiscard,
}: TableChangeToolbarProps) {
  if (changeCount === 0) return null;

  // Build a human-readable summary
  const parts: string[] = [];
  if (editCount > 0) parts.push(`${editCount} edit${editCount !== 1 ? 's' : ''}`);
  if (deleteCount > 0) parts.push(`${deleteCount} delete${deleteCount !== 1 ? 's' : ''}`);
  const summary = `${changeCount} change${changeCount !== 1 ? 's' : ''}: ${parts.join(', ')}`;

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-amber-50 dark:bg-amber-950/30 border-b border-amber-200 dark:border-amber-800/50">
      {/* Left: status */}
      <div className="flex items-center gap-2 text-sm text-amber-800 dark:text-amber-300">
        <AlertTriangle className="h-4 w-4 shrink-0" />
        <span className="font-medium">{summary}</span>
        <span className="text-amber-600 dark:text-amber-400 text-xs">
          (unsaved)
        </span>
      </div>

      {/* Right: actions */}
      <div className="flex items-center gap-2">
        <Button
          size="sm"
          variant="ghost"
          onClick={onDiscard}
          className="gap-1.5 text-amber-700 dark:text-amber-300 hover:text-amber-900 hover:bg-amber-100 dark:hover:bg-amber-900/50"
        >
          <Undo2 className="h-3.5 w-3.5" />
          Discard
        </Button>
        <Button
          size="sm"
          onClick={onSave}
          className="gap-1.5 bg-amber-600 hover:bg-amber-700 text-white"
        >
          <Save className="h-3.5 w-3.5" />
          Review & Commit
        </Button>
      </div>
    </div>
  );
}
