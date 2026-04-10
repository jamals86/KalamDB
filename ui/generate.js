const fs = require('fs');

const content = `import { memo, type ReactNode, useMemo } from "react";
import {
  ChevronDown,
  ChevronRight,
  Database,
  FolderTree,
  KeyRound,
  RefreshCw,
  Radio,
  Search,
  Star,
  Type,
  User,
  Users,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { StudioNamespace, StudioTable } from "./types";
import type { SavedQuery } from "./types";

interface StudioExplorerPanelProps {
  schema: StudioNamespace[];
  filter: string;
  savedQueries: SavedQuery[];
  favoritesExpanded: boolean;
  namespaceSectionExpanded: boolean;
  expandedNamespaces: Record<string, boolean>;
  expandedTables: Record<string, boolean>;
  selectedTableKey: string | null;
  isRefreshing: boolean;
  onFilterChange: (value: string) => void;
  onRefresh: () => void;
  onToggleFavorites: () => void;
  onToggleNamespaceSection: () => void;
  onToggleNamespace: (namespaceName: string) => void;
  onToggleTable: (tableKey: string) => void;
  onOpenSavedQuery: (queryId: string) => void;
  onSelectTable: (table: StudioTable) => void;
  onTableContextMenu: (table: StudioTable, position: { x: number; y: number }) => void;
}

function columnIcon(isPrimaryKey: boolean) {
  if (isPrimaryKey) {
    return <KeyRound className="h-3 w-3 text-amber-500" />;
  }
  return <Type className="h-3 w-3 text-muted-foreground" />;
}

function tableTypeMeta(tableType: string): { icon: ReactNode; tooltip: string } {
  const normalized = tableType.toLowerCase();
  if (normalized === "stream") {
    return <Radio className="h-3.5 w-3.5 text-violet-400" />;
  }
  if (normalized === "shared") {
    return <Users className="h-3.5 w-3.5 text-cyan-400" />;
  }
  if (normalized === "system") {
    return <Database className="h-3.5 w-3.5 text-amber-400" />;
  }
  return <User className="h-3.5 w-3.5 text-emerald-400" />;
}

const StudioExplorerPanelComponent = ({
  schema,
  filter,
  savedQueries,
  favoritesExpanded,
  namespaceSectionExpanded,
  expandedNamespaces,
  expandedTables,
  selectedTableKey,
  isRefreshing,
  onFilterChange,
  onRefresh,
  onToggleFavorites,
  onToggleNamespaceSection,
  onToggleNamespace,
  onToggleTable,
  onOpenSavedQuery,
  onSelectTable,
  onTableContextMenu,
}: StudioExplorerPanelProps) => {
  const normalizedFilter = filter.trim().toLowerCase();

  const filteredSchema = useMemo(() => {
    return schema
      .map((namespace) => {
        const filteredTables = namespace.tables.filter((table) => {
          if (!normalizedFilter) return true;
          return (
            namespace.name.toLowerCase().includes(normalizedFilter) ||
            table.name.toLowerCase().includes(normalizedFilter) ||
            table.columns.some((column) => column.name.toLowerCase().includes(normalizedFilter))
          );
        });
        return { ...namespace, tables: filteredTables };
      })
      .filter((namespace) => namespace.tables.length > 0 || !normalizedFilter);
  }, [schema, normalizedFilter]);

  const sectionButtonClassName = "flex w-full items-center gap-1.5 px-2 py-1.5 text-left text-xs font-semibold uppercase tracking-wider text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground rounded-sm";

  return (
    <TooltipProvider delayDuration={250}>
      <div className="flex h-full min-h-0 flex-col overflow-hidden border-r border-border bg-background text-foreground">
        
        {/* Header & Search */}
        <div className="shrink-0 border-b border-border pl-3 pr-2 py-2">
          <div className="mb-2 flex items-center justify-between gap-1">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">Explorer</p>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
              onClick={onRefresh}
              disabled={isRefreshing}
            >
              <RefreshCw className={cn("h-3.5 w-3.5", isRefreshing && "animate-spin")} />
            </Button>
          </div>
          <div className="relative">
            <Search className="pointer-events-none absolute left-2 top-1.5 h-3 w-3 text-muted-foreground" />
            <Input
              value={filter}
              onChange={(event) => onFilterChange(event.target.value)}
              className="h-6 border-border/60 bg-muted/30 pl-6 text-xs text-foreground placeholder:text-muted-foreground focus-visible:bg-background rounded-sm"
              placeholder="Search tables..."
            />
          </div>
        </div>

        {/* Scroll Content */}
        <ScrollArea className="h-full min-h-0 flex-1 overflow-hidden">
          <div className="py-2 space-y-1">
            
            {/* Favorites */}
            <div>
              <button type="button" onClick={onToggleFavorites} className={sectionButtonClassName}>
                <span className="flex h-4 w-4 shrink-0 items-center justify-center">
                  {favoritesExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                </span>
                <Star className="h-3.5 w-3.5 text-primary" />
                Favorites
                <span className="ml-auto text-[10px] text-muted-foreground">{savedQueries.length}</span>
              </button>
              {favoritesExpanded && (
                <div className="flex flex-col py-0.5">
                  {savedQueries.length === 0 && <p className="px-8 py-1 text-xs text-muted-foreground">No saved queries.</p>}
                  {savedQueries.map((savedQuery) => (
                    <button
                      key={savedQuery.id}
                      type="button"
                      onClick={() => onOpenSavedQuery(savedQuery.id)}
                      className="flex w-full items-center gap-2 pl-8 pr-2 py-1 text-left text-xs text-foreground hover:bg-accent cursor-pointer rounded-sm"
                    >
                      <Star className="h-3 w-3 shrink-0 text-primary/70" />
                      <span className="truncate">{savedQuery.title}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Namespaces */}
            <div>
              <button type="button" onClick={onToggleNamespaceSection} className={sectionButtonClassName}>
                <span className="flex h-4 w-4 shrink-0 items-center justify-center">
                  {namespaceSectionExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
                </span>
                <FolderTree className="h-3.5 w-3.5" />
                Namespaces
                <span className="ml-auto text-[10px] text-muted-foreground">{filteredSchema.length}</span>
              </button>
              {namespaceSectionExpanded && (
                <div className="flex flex-col py-0.5">
                  {filteredSchema.length === 0 && <div className="px-8 py-1 text-xs text-muted-foreground">No matching namespaces.</div>}
                  {filteredSchema.map((namespace) => {
                    const namespaceOpen = expandedNamespaces[namespace.name] ?? false;
                    return (
                      <div key={namespace.name} className="flex flex-col">
                        
                        <div
                          className="flex w-full items-center gap-1.5 pl-4 pr-2 py-1 text-xs text-foreground hover:bg-accent cursor-pointer rounded-sm"
                          onClick={() => onToggleNamespace(namespace.name)}
                        >
                          <span className="h-4 w-4 flex items-center justify-center shrink-0">
                            {namespaceOpen ? <ChevronDown className="h-3.5 w-3.5 text-muted-foreground/70" /> : <ChevronRight className="h-3.5 w-3.5 text-muted-foreground/70" />}
                          </span>
                          <Database className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                          <span className="truncate font-medium">{namespace.name}</span>
                        </div>

                        {namespaceOpen && (
                          <div className="flex flex-col">
                            {namespace.tables.map((table) => {
                              const tableKey = \`\${table.namespace}.\${table.name}\`;
                              const tableOpen = expandedTables[tableKey] ?? tableKey === selectedTableKey;
                              const isSelected = selectedTableKey === tableKey;
                              const tableIcon = tableTypeMeta(table.tableType);

                              return (
                                <div key={tableKey} className="flex flex-col">
                                  <div
                                    className={cn(
                                      "flex w-full items-center gap-1.5 pl-8 pr-2 py-1 text-xs cursor-pointer rounded-sm transition-colors",
                                      isSelected ? "bg-primary/10 text-primary dark:bg-primary/20" : "text-foreground hover:bg-accent"
                                    )}
                                    onClick={() => onSelectTable(table)}
                                    onContextMenu={(event) => {
                                      event.preventDefault();
                                      event.stopPropagation();
                                      onSelectTable(table);
                                      onTableContextMenu(table, { x: event.clientX, y: event.clientY });
                                    }}
                                  >
                                    <span 
                                      className="h-4 w-4 flex items-center justify-center shrink-0 cursor-pointer hover:bg-muted/50 rounded"
                                      onClick={(e) => {
                                        e.stopPropagation();
                                        onToggleTable(tableKey);
                                      }}
                                    >
                                      {tableOpen ? <ChevronDown className="h-3.5 w-3.5 opacity-70" /> : <ChevronRight className="h-3.5 w-3.5 opacity-70" />}
                                    </span>
                                    <span className="shrink-0">{tableIcon}</span>
                                    <span className={cn("truncate", isSelected && "font-medium")}>{table.name}</span>
                                  </div>

                                  {tableOpen && (
                                    <div className="flex flex-col pb-0.5">
                                      {table.columns.map((column) => (
                                        <div key={\`\${tableKey}.\${column.name}\`} className="flex items-center gap-2 pl-[3.25rem] pr-2 py-0.5 text-xs text-muted-foreground hover:bg-accent/40 rounded-sm cursor-default">
                                          <span className="shrink-0">{columnIcon(column.isPrimaryKey)}</span>
                                          <span className="truncate">{column.name}</span>
                                          <span className="ml-auto truncate font-mono text-[10px] lowercase opacity-60 bg-muted/40 px-1 rounded">{column.dataType}</span>
                                        </div>
                                      ))}
                                    </div>
                                  )}
                                </div>
                              );
                            })}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

          </div>
        </ScrollArea>
      </div>
    </TooltipProvider>
  );
};

export const StudioExplorerPanel = memo(StudioExplorerPanelComponent);
`;
fs.writeFileSync('ui/src/components/sql-studio-v2/StudioExplorerPanel.tsx', content);
