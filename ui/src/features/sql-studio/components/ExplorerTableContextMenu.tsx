import { Pencil, Trash2, Code2 } from "lucide-react";
import type { StudioTable } from "@/components/sql-studio-v2/shared/types";

interface ExplorerTableContextMenuProps {
  contextMenu: { x: number; y: number; table: StudioTable } | null;
  onClose: () => void;
  onOpenQueryInNewTab: (table: StudioTable) => void;
  onSelectFromTable: (table: StudioTable) => void;
  onInsertSelectQuery: (table: StudioTable) => void;
  onViewProperties: (table: StudioTable) => void;
  onCopyQualifiedName: (table: StudioTable) => void;
  onEditSchema: (table: StudioTable) => void;
  onDropTable: (table: StudioTable) => void;
  onCopyCreateSql: (table: StudioTable) => void;
}

export function ExplorerTableContextMenu({
  contextMenu,
  onClose,
  onOpenQueryInNewTab,
  onSelectFromTable,
  onInsertSelectQuery,
  onViewProperties,
  onCopyQualifiedName,
  onEditSchema,
  onDropTable,
  onCopyCreateSql,
}: ExplorerTableContextMenuProps) {
  if (!contextMenu) {
    return null;
  }

  const table = contextMenu.table;
  const itemClassName = "w-full px-3 py-2 text-left text-sm text-popover-foreground transition-colors hover:bg-accent hover:text-accent-foreground";

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} />
      <div
        className="fixed z-50 min-w-[210px] rounded-md border border-border bg-popover py-1 text-popover-foreground shadow-xl"
        style={{ left: contextMenu.x, top: contextMenu.y }}
      >
        <div className="border-b border-border px-3 py-1.5 text-[11px] text-muted-foreground">
          {table.namespace}.{table.name}
        </div>
        <button
          type="button"
          className={itemClassName}
          onClick={() => onOpenQueryInNewTab(table)}
        >
          Open Query In New Tab
        </button>
        <button
          type="button"
          className={itemClassName}
          onClick={() => onSelectFromTable(table)}
        >
          Select * From Table
        </button>
        <button
          type="button"
          className={itemClassName}
          onClick={() => onInsertSelectQuery(table)}
        >
          Insert SELECT Query
        </button>
        <div className="my-1 border-t border-border" />
        <button
          type="button"
          className={itemClassName}
          onClick={() => onViewProperties(table)}
        >
          View Properties
        </button>
        <button
          type="button"
          className={itemClassName}
          onClick={() => onCopyQualifiedName(table)}
        >
          Copy Qualified Table Name
        </button>
        <button
          type="button"
          className={`${itemClassName} flex items-center gap-2`}
          onClick={() => onCopyCreateSql(table)}
        >
          <Code2 className="h-3.5 w-3.5" />
          Copy CREATE TABLE SQL
        </button>
        <div className="my-1 border-t border-border" />
        <button
          type="button"
          className={`${itemClassName} flex items-center gap-2`}
          onClick={() => onEditSchema(table)}
        >
          <Pencil className="h-3.5 w-3.5" />
          Edit Table
        </button>
        <button
          type="button"
          className={`${itemClassName} flex items-center gap-2 text-destructive hover:bg-destructive/10 hover:text-destructive`}
          onClick={() => onDropTable(table)}
        >
          <Trash2 className="h-3.5 w-3.5" />
          Drop Table
        </button>
      </div>
    </>
  );
}
