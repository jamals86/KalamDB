import { memo, type ReactNode, useMemo } from "react";
import {
  ChevronDown,
  ChevronRight,
  Database,
  FolderOpen,
  KeyRound,
  Radio,
  Search,
  Star,
  Type,
  User,
  Users,
} from "lucide-react";
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
  favoriteQueries: string[];
  savedQueries: SavedQuery[];
  favoritesExpanded: boolean;
  expandedNamespaces: Record<string, boolean>;
  expandedTables: Record<string, boolean>;
  selectedTableKey: string | null;
  onFilterChange: (value: string) => void;
  onToggleFavorites: () => void;
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
  return <Type className="h-3 w-3 text-slate-500" />;
}

function tableTypeMeta(tableType: string): { icon: ReactNode; tooltip: string } {
  const normalized = tableType.toLowerCase();
  if (normalized === "stream") {
    return {
      icon: <Radio className="h-3.5 w-3.5 text-violet-400" />,
      tooltip: "Stream table",
    };
  }
  if (normalized === "shared") {
    return {
      icon: <Users className="h-3.5 w-3.5 text-cyan-400" />,
      tooltip: "Shared table",
    };
  }
  if (normalized === "system") {
    return {
      icon: <Database className="h-3.5 w-3.5 text-amber-400" />,
      tooltip: "System table",
    };
  }
  return {
    icon: <User className="h-3.5 w-3.5 text-emerald-400" />,
    tooltip: "User table",
  };
}

const StudioExplorerPanelComponent = ({
  schema,
  filter,
  favoriteQueries,
  savedQueries,
  favoritesExpanded,
  expandedNamespaces,
  expandedTables,
  selectedTableKey,
  onFilterChange,
  onToggleFavorites,
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
          if (!normalizedFilter) {
            return true;
          }

          return (
            namespace.name.toLowerCase().includes(normalizedFilter) ||
            table.name.toLowerCase().includes(normalizedFilter) ||
            table.columns.some((column) => column.name.toLowerCase().includes(normalizedFilter))
          );
        });

        return {
          ...namespace,
          tables: filteredTables,
        };
      })
      .filter((namespace) => namespace.tables.length > 0 || !normalizedFilter);
  }, [schema, normalizedFilter]);

  return (
    <TooltipProvider delayDuration={250}>
      <div className="flex h-full min-h-0 flex-col overflow-hidden border-r border-[#1b2a40] bg-[#131f31] text-slate-300">
        <div className="border-b border-[#1b2a40] px-3 py-3">
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.18em] text-slate-500">Explorer</p>
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="relative">
                <Search className="pointer-events-none absolute left-2 top-2.5 h-3.5 w-3.5 text-slate-500" />
                <Input
                  value={filter}
                  onChange={(event) => onFilterChange(event.target.value)}
                  className="h-8 border-[#1f334d] bg-[#0d1726] pl-7 text-xs text-slate-200 placeholder:text-slate-500"
                  placeholder="Search tables..."
                />
              </div>
            </TooltipTrigger>
            <TooltipContent>Filter namespaces, tables, and columns</TooltipContent>
          </Tooltip>
        </div>

        <ScrollArea className="min-h-0 flex-1 overflow-hidden">
          <div className="space-y-3 p-2">
            <div>
              <button
                type="button"
                onClick={onToggleFavorites}
                className="mb-1 flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-[11px] font-semibold uppercase tracking-[0.16em] text-slate-500 hover:bg-[#12263f]"
              >
                {favoritesExpanded ? (
                  <ChevronDown className="h-3 w-3" />
                ) : (
                  <ChevronRight className="h-3 w-3" />
                )}
                Favorites
              </button>
              {favoritesExpanded && (
                <div className="space-y-2">
                  <div>
                    <p className="px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.15em] text-slate-500">
                      Quick Favorites
                    </p>
                    <div className="space-y-0.5">
                      {favoriteQueries.map((item) => (
                        <Tooltip key={item}>
                          <TooltipTrigger asChild>
                            <button
                              type="button"
                              className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-slate-300 transition-colors hover:bg-[#0f2744] hover:text-sky-300"
                            >
                              <Star className="h-3.5 w-3.5 text-amber-500" />
                              <span className="truncate">{item}</span>
                            </button>
                          </TooltipTrigger>
                          <TooltipContent>{item}</TooltipContent>
                        </Tooltip>
                      ))}
                    </div>
                  </div>

                  <div>
                    <p className="px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.15em] text-slate-500">
                      Saved Queries
                    </p>
                    <div className="space-y-0.5">
                      {savedQueries.length === 0 && (
                        <p className="px-2 py-1 text-xs text-slate-500">No saved queries yet.</p>
                      )}
                      {savedQueries.map((savedQuery) => (
                        <Tooltip key={savedQuery.id}>
                          <TooltipTrigger asChild>
                            <button
                              type="button"
                              onClick={() => onOpenSavedQuery(savedQuery.id)}
                              className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm text-slate-300 transition-colors hover:bg-[#0f2744] hover:text-sky-300"
                            >
                              <Star className="h-3.5 w-3.5 text-sky-400" />
                              <span className="truncate">{savedQuery.title}</span>
                              {savedQuery.isLive && <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />}
                            </button>
                          </TooltipTrigger>
                          <TooltipContent>
                            {savedQuery.title}
                          </TooltipContent>
                        </Tooltip>
                      ))}
                    </div>
                  </div>
                </div>
              )}
            </div>

            <div className="space-y-0.5">
              {filteredSchema.map((namespace) => {
                const namespaceOpen = expandedNamespaces[namespace.name] ?? false;
                return (
                  <div key={namespace.name}>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <button
                          type="button"
                          onClick={() => onToggleNamespace(namespace.name)}
                          className="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-[11px] text-slate-500 hover:bg-[#12263f]"
                        >
                          {namespaceOpen ? (
                            <ChevronDown className="h-3 w-3" />
                          ) : (
                            <ChevronRight className="h-3 w-3" />
                          )}
                          <Database className="h-3 w-3" />
                          <span className="truncate font-semibold">{namespace.name}</span>
                        </button>
                      </TooltipTrigger>
                      <TooltipContent>Namespace: {namespace.name}</TooltipContent>
                    </Tooltip>

                    {namespaceOpen && (
                      <div className="space-y-0.5 border-l border-[#243851] pl-3">
                        {namespace.tables.map((table) => {
                          const tableKey = `${table.namespace}.${table.name}`;
                          const tableOpen = expandedTables[tableKey] ?? tableKey === selectedTableKey;
                          const isSelected = selectedTableKey === tableKey;
                          const tableMeta = tableTypeMeta(table.tableType);

                          return (
                            <div key={tableKey}>
                              <div
                                className={cn(
                                  "group flex items-center justify-between rounded px-2 py-1.5 transition-colors",
                                  isSelected
                                    ? "bg-[#0f2744] text-sky-300"
                                    : "text-slate-400 hover:bg-[#12263f] hover:text-slate-200",
                                )}
                                onContextMenu={(event) => {
                                  event.preventDefault();
                                  event.stopPropagation();
                                  onSelectTable(table);
                                  onTableContextMenu(table, { x: event.clientX, y: event.clientY });
                                }}
                              >
                                <button
                                  type="button"
                                  onClick={() => onToggleTable(tableKey)}
                                  className="mr-1 text-slate-500 hover:text-slate-300"
                                >
                                  {tableOpen ? (
                                    <ChevronDown className="h-3 w-3" />
                                  ) : (
                                    <ChevronRight className="h-3 w-3" />
                                  )}
                                </button>

                                <button
                                  type="button"
                                  onClick={() => onSelectTable(table)}
                                  className="flex min-w-0 flex-1 items-center gap-2 text-left"
                                >
                                  <Tooltip>
                                    <TooltipTrigger asChild>
                                      <span className="inline-flex items-center">{tableMeta.icon}</span>
                                    </TooltipTrigger>
                                    <TooltipContent>{tableMeta.tooltip}</TooltipContent>
                                  </Tooltip>
                                  <FolderOpen className="h-3.5 w-3.5" />
                                  <span className="truncate text-sm">{table.name}</span>
                                </button>
                              </div>

                              {tableOpen && (
                                <div className="space-y-0.5 border-l border-[#22354d] pl-4">
                                  {table.columns.map((column) => (
                                    <Tooltip key={`${tableKey}.${column.name}`}>
                                      <TooltipTrigger asChild>
                                        <div className="flex items-center gap-2 px-2 py-0.5 text-xs text-slate-500">
                                          {columnIcon(column.isPrimaryKey)}
                                          <span className="truncate">{column.name}</span>
                                          <span className="ml-auto truncate font-mono text-[10px] lowercase">{column.dataType}</span>
                                        </div>
                                      </TooltipTrigger>
                                      <TooltipContent>
                                        {column.name} ({column.dataType})
                                      </TooltipContent>
                                    </Tooltip>
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
          </div>
        </ScrollArea>

        <div className="border-t border-[#1b2a40] bg-[#0f1b2b] px-3 py-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="flex items-center gap-2 text-xs text-slate-400">
                <span className="h-2 w-2 rounded-full bg-emerald-500" />
                <span>Connected</span>
                <span className="ml-auto">14ms</span>
              </div>
            </TooltipTrigger>
            <TooltipContent>Current SQL studio connection latency</TooltipContent>
          </Tooltip>
        </div>
      </div>
    </TooltipProvider>
  );
};

export const StudioExplorerPanel = memo(StudioExplorerPanelComponent);
