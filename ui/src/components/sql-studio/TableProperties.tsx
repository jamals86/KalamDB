import { X, Table2, Pencil, Trash2, Copy, Plus, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useEffect } from "react";
import { useDescribeTable } from "@/hooks/useDescribeTable";

interface ColumnInfo {
  name: string;
  dataType: string;
  sqlType: string;
  isNullable?: boolean;
  isPrimaryKey?: boolean;
}

interface TableMetadata {
  engine?: string;
  rowCount?: number;
  createdAt?: string;
}

interface TablePropertiesProps {
  tableName: string | null;
  namespace: string | null;
  columns: ColumnInfo[];
  metadata: TableMetadata;
  isNewTable?: boolean;
  onClose: () => void;
  onAlter?: () => void;
  onDrop?: () => void;
  onCopyDDL?: () => void;
  onEditColumn?: (columnName: string) => void;
  onAddColumn?: () => void;
  onCreateTable?: () => void;
  toSqlType: (arrowType: string) => string;
}

export function TableProperties({
  tableName,
  namespace,
  columns,
  metadata,
  isNewTable = false,
  onClose,
  onAlter,
  onDrop,
  onCopyDDL,
  onEditColumn,
  onAddColumn,
  onCreateTable,
  toSqlType,
}: TablePropertiesProps) {
  const { columns: describeColumns, isLoading, describeTable } = useDescribeTable();

  // Load DESCRIBE TABLE data when table is selected
  useEffect(() => {
    if (!isNewTable && tableName && namespace) {
      describeTable(namespace, tableName).catch(console.error);
    }
  }, [tableName, namespace, isNewTable, describeTable]);

  if (!tableName && !isNewTable) return null;

  const fullPath = namespace && tableName ? `${namespace}.${tableName}` : tableName || "New Table";

  // Use DESCRIBE data if available, otherwise fall back to schema columns
  const displayColumns = describeColumns.length > 0 ? describeColumns : null;

  return (
    <div className="w-72 border-l flex flex-col shrink-0 bg-background h-full">
      {/* Header */}
      <div className="p-4 border-b flex items-center justify-between shrink-0">
        <div className="flex items-center gap-2 font-medium">
          {isNewTable ? "Create Table" : "Table Properties"}
        </div>
        <Button
          size="sm"
          variant="ghost"
          onClick={onClose}
          className="h-6 w-6 p-0"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Table Name Section - Fixed */}
      <div className="p-4 border-b shrink-0">
        <div className="flex items-start gap-3">
          <div className="p-2 bg-emerald-100 dark:bg-emerald-900/30 rounded">
            <Table2 className="h-5 w-5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="font-semibold text-base">{tableName || "new_table"}</div>
            <div className="text-sm text-muted-foreground truncate">{fullPath}</div>
          </div>
        </div>
      </div>

      {/* Scrollable Columns Section */}
      <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
        <div className="p-4 pb-2 shrink-0 flex items-center justify-between">
          <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
            Columns ({displayColumns?.length || columns.length})
          </div>
          <div className="flex items-center gap-1">
            {!isNewTable && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => tableName && namespace && describeTable(namespace, tableName)}
                disabled={isLoading}
                className="h-6 w-6 p-0"
                title="Refresh"
              >
                <RefreshCw className={`h-3 w-3 ${isLoading ? 'animate-spin' : ''}`} />
              </Button>
            )}
            <Button
              size="sm"
              variant="ghost"
              onClick={onAddColumn}
              className="h-6 px-2 text-xs gap-1"
              title="Add column"
            >
              <Plus className="h-3 w-3" />
              Add
            </Button>
          </div>
        </div>
        <ScrollArea className="flex-1 px-4">
          {displayColumns ? (
            // Show full DESCRIBE TABLE data in a table format
            <div className="pb-4">
              <table className="w-full text-xs border-collapse">
                <thead className="sticky top-0 bg-background">
                  <tr className="border-b">
                    <th className="text-left py-2 pr-2 font-semibold text-muted-foreground">#</th>
                    <th className="text-left py-2 pr-2 font-semibold text-muted-foreground">Column</th>
                    <th className="text-left py-2 pr-2 font-semibold text-muted-foreground">Type</th>
                    <th className="text-center py-2 px-1 font-semibold text-muted-foreground" title="Nullable">N</th>
                    <th className="text-center py-2 px-1 font-semibold text-muted-foreground" title="Primary Key">PK</th>
                  </tr>
                </thead>
                <tbody>
                  {describeColumns.map((col) => (
                    <tr 
                      key={col.column_name} 
                      className="border-b hover:bg-muted/50 group"
                    >
                      <td className="py-2 pr-2 text-muted-foreground">{col.ordinal_position}</td>
                      <td className="py-2 pr-2">
                        <div className="flex flex-col">
                          <span className="font-medium">{col.column_name}</span>
                          {col.column_comment && (
                            <span className="text-muted-foreground text-xs italic mt-0.5">{col.column_comment}</span>
                          )}
                          {col.column_default && (
                            <span className="text-muted-foreground text-xs mt-0.5">
                              default: <code className="bg-muted px-1 rounded">{col.column_default}</code>
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="py-2 pr-2">
                        <Badge variant="secondary" className="text-xs font-mono">
                          {col.data_type}
                        </Badge>
                      </td>
                      <td className="py-2 px-1 text-center">
                        {col.is_nullable ? (
                          <span className="text-green-500" title="NULL">✓</span>
                        ) : (
                          <span className="text-red-500" title="NOT NULL">✗</span>
                        )}
                      </td>
                      <td className="py-2 px-1 text-center">
                        {col.is_primary_key && (
                          <span className="text-yellow-500" title="Primary Key">🔑</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            // Fallback to simple column list
            <div className="space-y-1.5 pb-4">
              {columns.length === 0 ? (
                <div className="text-sm text-muted-foreground py-4 text-center">
                  No columns defined
                </div>
              ) : (
                columns.map((col) => (
                  <div
                    key={col.name}
                    className="flex items-center justify-between p-2 rounded bg-muted/50 hover:bg-muted/80 group"
                  >
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      {col.isPrimaryKey && (
                        <span className="text-yellow-500 text-xs shrink-0" title="Primary Key">🔑</span>
                      )}
                      <span className="text-sm font-medium truncate">{col.name}</span>
                      {col.isNullable === false && (
                        <span className="text-red-400 text-xs shrink-0" title="NOT NULL">*</span>
                      )}
                    </div>
                    <div className="flex items-center gap-1.5 shrink-0">
                      <Badge variant="secondary" className="text-xs font-mono">
                        {toSqlType(col.dataType)}
                      </Badge>
                      <button
                        className="p-1 hover:bg-background rounded opacity-0 group-hover:opacity-100 transition-opacity"
                        onClick={() => onEditColumn?.(col.name)}
                        title="Edit column"
                      >
                        <Pencil className="h-3 w-3 text-muted-foreground hover:text-foreground" />
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          )}
        </ScrollArea>
      </div>

      {/* Metadata Section - Fixed */}
      {!isNewTable && (metadata.engine || metadata.rowCount !== undefined || metadata.createdAt) && (
        <div className="p-4 border-t shrink-0">
          <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3">
            Metadata
          </div>
          <div className="space-y-2">
            {metadata.engine && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">Engine</span>
                <span className="font-medium">{metadata.engine}</span>
              </div>
            )}
            {metadata.rowCount !== undefined && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">Rows</span>
                <span className="font-medium">{metadata.rowCount.toLocaleString()}</span>
              </div>
            )}
            {metadata.createdAt && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">Created</span>
                <span className="font-medium">{metadata.createdAt}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Actions Section - Fixed at bottom */}
      <div className="p-4 border-t shrink-0">
        <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-3">
          Actions
        </div>
        {isNewTable ? (
          <Button
            variant="default"
            size="sm"
            onClick={onCreateTable}
            className="w-full gap-1.5 bg-emerald-600 hover:bg-emerald-700"
          >
            <Plus className="h-3.5 w-3.5" />
            Create Table
          </Button>
        ) : (
          <>
            <div className="grid grid-cols-2 gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={onAlter}
                className="gap-1.5"
              >
                <Pencil className="h-3.5 w-3.5" />
                Alter
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={onDrop}
                className="gap-1.5 text-red-600 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-950/30"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Drop
              </Button>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={onCopyDDL}
              className="w-full mt-2 gap-1.5"
            >
              <Copy className="h-3.5 w-3.5" />
              Copy DDL
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
