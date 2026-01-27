import { Search } from "lucide-react";
import { Input } from "@/components/ui/input";
import { ScrollArea, ScrollBar } from "@/components/ui/scroll-area";
import { SidebarHeader } from "./SidebarHeader";
import { SchemaTree } from "./Tree";
import type { SchemaNode } from "../types";

interface SidebarProps {
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

export function Sidebar({
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
}: SidebarProps) {
  return (
    <div className="h-full w-full min-w-0 border-r flex flex-col bg-background">
      <SidebarHeader
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

      <ScrollArea className="flex-1">
        <div className="min-w-0 p-2">
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
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
    </div>
  );
}
