import type { StudioTable } from "@/components/sql-studio-v2/types";

interface ExplorerTableContextMenuProps {
  contextMenu: { x: number; y: number; table: StudioTable } | null;
  onClose: () => void;
  onOpenQueryInNewTab: (table: StudioTable) => void;
  onSelectFromTable: (table: StudioTable) => void;
  onInsertSelectQuery: (table: StudioTable) => void;
  onViewProperties: (table: StudioTable) => void;
  onCopyQualifiedName: (table: StudioTable) => void;
}

export function ExplorerTableContextMenu({
  contextMenu,
  onClose,
  onOpenQueryInNewTab,
  onSelectFromTable,
  onInsertSelectQuery,
  onViewProperties,
  onCopyQualifiedName,
}: ExplorerTableContextMenuProps) {
  if (!contextMenu) {
    return null;
  }

  const table = contextMenu.table;

  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} />
      <div
        className="fixed z-50 min-w-[210px] rounded-md border border-border bg-background py-1 shadow-xl"
        style={{ left: contextMenu.x, top: contextMenu.y }}
      >
        <div className="border-b border-border px-3 py-1.5 text-[11px] text-muted-foreground">
          {table.namespace}.{table.name}
        </div>
        <button
          type="button"
          className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-[#133253]"
          onClick={() => onOpenQueryInNewTab(table)}
        >
          Open Query In New Tab
        </button>
        <button
          type="button"
          className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-[#133253]"
          onClick={() => onSelectFromTable(table)}
        >
          Select * From Table
        </button>
        <button
          type="button"
          className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-[#133253]"
          onClick={() => onInsertSelectQuery(table)}
        >
          Insert SELECT Query
        </button>
        <div className="my-1 border-t border-border" />
        <button
          type="button"
          className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-[#133253]"
          onClick={() => onViewProperties(table)}
        >
          View Properties
        </button>
        <button
          type="button"
          className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-[#133253]"
          onClick={() => onCopyQualifiedName(table)}
        >
          Copy Qualified Table Name
        </button>
      </div>
    </>
  );
}
