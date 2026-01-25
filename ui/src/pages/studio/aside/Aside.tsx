import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { AsideHeader } from "./Header";
import { SchemaTree } from "./Tree";
import type { SchemaNode } from "../types";

interface AsideProps {
  schema: SchemaNode[];
  schemaFilter: string;
  schemaLoading: boolean;
  onFilterChange: (filter: string) => void;
  onRefreshSchema: () => void;
  onCreateTable: () => void;
  onToggleNode: (path: string[]) => void;
  onInsertText: (text: string) => void;
  onShowTableProperties: (namespace: string, tableName: string, columns: SchemaNode[]) => void;
  onTableContextMenu: (e: React.MouseEvent, namespace: string, tableName: string, columns: SchemaNode[]) => void;
}

export function Aside({
  schema,
  schemaFilter,
  schemaLoading,
  onFilterChange,
  onRefreshSchema,
  onCreateTable,
  onToggleNode,
  onInsertText,
  onShowTableProperties,
  onTableContextMenu,
}: AsideProps) {
  return (
    <div className="w-56 border-r flex flex-col shrink-0">
      <AsideHeader
        onCreateTable={onCreateTable}
        onRefresh={onRefreshSchema}
        isLoading={schemaLoading}
      />
      
      <div className="p-2 border-b">
        <div className="relative">
          <Search className="absolute left-2 top-2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Filter..."
            value={schemaFilter}
            onChange={(e) => onFilterChange(e.target.value)}
            className="pl-8 h-8 text-sm"
          />
        </div>
      </div>

      <ScrollArea className="flex-1 p-2">
        <div className="min-w-max">
          {schemaLoading ? (
            <div className="text-sm text-muted-foreground p-2">Loading schema...</div>
          ) : schema.length === 0 ? (
            <div className="text-sm text-muted-foreground p-2">
              {schemaFilter ? "No matches found" : "No schemas found"}
            </div>
          ) : (
            <SchemaTree
              nodes={schema}
              onToggle={onToggleNode}
              onInsert={onInsertText}
              onShowProperties={onShowTableProperties}
              onContextMenu={onTableContextMenu}
            />
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
