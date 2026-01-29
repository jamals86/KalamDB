import {
  ChevronDown,
  ChevronRight,
  Database,
  Table2,
  Columns3,
  Radio,
  Users,
  User,
  Info,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { getDataTypeColor } from "@/lib/config";
import { useDataTypes } from "@/hooks/useDataTypes";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { SchemaNode } from "../types";

interface TreeItemProps {
  node: SchemaNode;
  path: string[];
  onToggle: (path: string[]) => void;
  onInsert: (text: string) => void;
  onShowProperties: (namespace: string, tableName: string, columns: SchemaNode[]) => void;
  onContextMenu: (e: React.MouseEvent, namespace: string, tableName: string, columns: SchemaNode[]) => void;
  renderChildren?: (nodes: SchemaNode[], path: string[]) => React.ReactNode;
}

export function TreeItem({
  node,
  path,
  onToggle,
  onInsert,
  onShowProperties,
  onContextMenu,
  renderChildren,
}: TreeItemProps) {
  const { toSqlType } = useDataTypes();
  const currentPath = [...path, node.name];
  const isExpanded = node.isExpanded;

  // Build tooltip content
  const getTooltipContent = () => {
    if (node.comment) {
      return node.comment;
    }
    // Default tooltip based on type
    if (node.type === "table") {
      const typeLabel = node.tableType ? `${node.tableType} table` : "table";
      const columnCount = node.children?.length || 0;
      return `${typeLabel} with ${columnCount} column${columnCount !== 1 ? "s" : ""}`;
    }
    if (node.type === "column" && node.dataType) {
      let tooltip = `Type: ${toSqlType(node.dataType)}`;
      if (node.isPrimaryKey) tooltip += " (Primary Key)";
      if (node.isNullable === false) tooltip += " NOT NULL";
      return tooltip;
    }
    if (node.type === "namespace") {
      const tableCount = node.children?.length || 0;
      return `Namespace with ${tableCount} table${tableCount !== 1 ? "s" : ""}`;
    }
    return null;
  };

  const tooltipContent = getTooltipContent();

  const itemContent = (
    <div key={currentPath.join(".")} className="select-none">
      <div
        className={cn(
          "flex items-center gap-1 px-2 py-1 hover:bg-muted/50 rounded cursor-pointer text-sm group",
          node.type === "column" && "pl-8"
        )}
        onClick={() => {
          if (node.type === "column") {
            onInsert(node.name);
          } else {
            onToggle(currentPath);
          }
        }}
        onDoubleClick={() => {
          if (node.type === "table") {
            onShowProperties(path[path.length - 1] || "", node.name, node.children || []);
          }
        }}
        onContextMenu={(e) => {
          if (node.type === "table") {
            e.preventDefault();
            e.stopPropagation();
            onContextMenu(e, path[path.length - 1] || "", node.name, node.children || []);
          }
        }}
      >
        {(node.children && node.children.length > 0) && (
          <span className="w-4">
            {isExpanded ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
          </span>
        )}
        {(!node.children || node.children.length === 0) && node.type !== "column" && (
          <span className="w-4" />
        )}
        {node.type === "namespace" && (
          <Database className="h-4 w-4 text-blue-500" />
        )}
        {node.type === "table" && node.tableType === "stream" && (
          <span title="Stream Table">
            <Radio className="h-4 w-4 text-purple-500" />
          </span>
        )}
        {node.type === "table" && node.tableType === "shared" && (
          <span title="Shared Table">
            <Users className="h-4 w-4 text-cyan-500" />
          </span>
        )}
        {node.type === "table" && node.tableType === "user" && (
          <span title="User Table">
            <User className="h-4 w-4 text-green-500" />
          </span>
        )}
        {node.type === "table" && node.tableType === "system" && (
          <span title="System Table">
            <Database className="h-4 w-4 text-orange-500" />
          </span>
        )}
        {node.type === "table" && !node.tableType && (
          <Table2 className="h-4 w-4 text-green-500" />
        )}
        {node.type === "column" && (
          <Columns3 className="h-4 w-4 text-orange-500" />
        )}
        {node.isPrimaryKey && (
          <span className="text-yellow-500 text-xs" title="Primary Key">🔑</span>
        )}
        <span className="truncate flex-1 min-w-0">{node.name}</span>
        {node.type === "table" && (
          <button
            className="p-0.5 hover:bg-muted rounded opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
            onClick={(e) => {
              e.stopPropagation();
              onShowProperties(path[path.length - 1] || "", node.name, node.children || []);
            }}
            title="View table properties"
          >
            <Info className="h-3.5 w-3.5 text-muted-foreground hover:text-foreground" />
          </button>
        )}
        {node.dataType && (
          <span 
            className={cn(
              "text-xs ml-auto flex items-center gap-1 shrink-0",
              getDataTypeColor(node.dataType)
            )}
            title={`${toSqlType(node.dataType)} (${node.dataType})`}
          >
            {toSqlType(node.dataType)}
            {node.isNullable === false && <span className="text-red-400" title="NOT NULL">*</span>}
          </span>
        )}
      </div>
      {isExpanded && node.children && renderChildren && (
        <div className="ml-4">{renderChildren(node.children, currentPath)}</div>
      )}
    </div>
  );

  // Wrap with tooltip if we have content
  if (tooltipContent) {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            {itemContent}
          </TooltipTrigger>
          <TooltipContent side="right" className="max-w-xs">
            <p className="text-xs">{tooltipContent}</p>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
  }

  return itemContent;
}
