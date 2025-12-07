import { useState, useCallback, useRef, useEffect } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import {
  Play,
  Plus,
  X,
  Database,
  Table2,
  Columns3,
  ChevronRight,
  ChevronDown,
  Zap,
  ZapOff,
  Clock,
  Trash2,
  Copy,
  Download,
  RotateCcw,
  Search,
  Code,
  TableIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { executeSql, executeQuery as executeQueryApi, subscribe, type Unsubscribe } from "@/lib/kalam-client";
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  getPaginationRowModel,
  useReactTable,
  type ColumnDef,
  type SortingState,
} from "@tanstack/react-table";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ArrowUpDown } from "lucide-react";

interface QueryTab {
  id: string;
  name: string;
  query: string;
  results: Record<string, unknown>[] | null;
  columns: ColumnDef<Record<string, unknown>>[];
  error: string | null;
  isLoading: boolean;
  isLive: boolean;
  unsubscribeFn: Unsubscribe | null;
  executionTime: number | null;
  rowCount: number | null;
}

interface SchemaNode {
  name: string;
  type: "namespace" | "table" | "column";
  dataType?: string;
  isNullable?: boolean;
  isPrimaryKey?: boolean;
  children?: SchemaNode[];
  isExpanded?: boolean;
}

interface QueryHistoryItem {
  id: string;
  query: string;
  timestamp: Date;
  executionTime: number;
  rowCount: number;
  success: boolean;
}

export default function SqlStudio() {
  const [tabs, setTabs] = useState<QueryTab[]>([
    {
      id: "1",
      name: "Query 1",
      query: "SELECT * FROM system.namespaces LIMIT 100",
      results: null,
      columns: [],
      error: null,
      isLoading: false,
      isLive: false,
      unsubscribeFn: null,
      executionTime: null,
      rowCount: null,
    },
  ]);
  const [activeTabId, setActiveTabId] = useState("1");
  const [schema, setSchema] = useState<SchemaNode[]>([]);
  const [schemaLoading, setSchemaLoading] = useState(true);
  const [queryHistory, setQueryHistory] = useState<QueryHistoryItem[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const [sorting, setSorting] = useState<SortingState>([]);
  const [schemaFilter, setSchemaFilter] = useState("");
  const [resultView, setResultView] = useState<"table" | "json">("table");
  const [selectedCell, setSelectedCell] = useState<{ row: number; col: string } | null>(null);
  const [cellViewerOpen, setCellViewerOpen] = useState(false);
  const [cellViewerContent, setCellViewerContent] = useState<{ column: string; value: unknown } | null>(null);
  const tabCounter = useRef(2);
  const monacoRef = useRef<Monaco | null>(null);

  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  // Load schema on mount
  useEffect(() => {
    loadSchema();
  }, []);

  const loadSchema = async () => {
    setSchemaLoading(true);
    try {
      // Get all schemas from information_schema (includes system, information_schema, and user namespaces)
      const schemasResult = await executeSql(
        "SELECT DISTINCT table_schema FROM information_schema.tables ORDER BY table_schema"
      );
      const namespaces: SchemaNode[] = [];

      for (const schema of schemasResult) {
        const schemaName = schema.table_schema as string;
        // Get tables for this schema
        const tablesResult = await executeSql(
          `SELECT table_name FROM information_schema.tables WHERE table_schema = '${schemaName}' ORDER BY table_name`
        );

        const tables: SchemaNode[] = [];
        for (const tbl of tablesResult) {
          const tableName = tbl.table_name as string;
          // Get columns for this table with full metadata
          const columnsResult = await executeSql(
            `SELECT column_name, data_type, is_nullable, ordinal_position FROM information_schema.columns WHERE table_schema = '${schemaName}' AND table_name = '${tableName}' ORDER BY ordinal_position`
          );

          const columns: SchemaNode[] = columnsResult.map((col: Record<string, unknown>) => ({
            name: col.column_name as string,
            type: "column" as const,
            dataType: col.data_type as string,
            isNullable: col.is_nullable === "YES",
            // First column (ordinal_position 0) is typically the primary key
            isPrimaryKey: col.ordinal_position === 0,
          }));

          tables.push({
            name: tableName,
            type: "table",
            children: columns,
            isExpanded: false,
          });
          console.log(`Loaded table ${schemaName}.${tableName} with ${columns.length} columns`);
        }

        namespaces.push({
          name: schemaName,
          type: "namespace",
          children: tables,
          isExpanded: schemaName === "system",
        });
      }

      setSchema(namespaces);
    } catch (error) {
      console.error("Failed to load schema:", error);
    } finally {
      setSchemaLoading(false);
    }
  };

  const updateTab = useCallback(
    (tabId: string, updates: Partial<QueryTab>) => {
      setTabs((prev) =>
        prev.map((t) => (t.id === tabId ? { ...t, ...updates } : t))
      );
    },
    []
  );

  const executeQuery = useCallback(async () => {
    const tab = tabs.find((t) => t.id === activeTabId);
    if (!tab || !tab.query.trim()) return;

    updateTab(activeTabId, { isLoading: true, error: null });
    const startTime = performance.now();

    try {
      const response = await executeQueryApi(tab.query);
      const executionTime = Math.round(performance.now() - startTime);
      
      // Get results and column names from response
      const results = (response.results?.[0]?.rows as Record<string, unknown>[]) ?? [];
      const columnNames = (response.results?.[0]?.columns as string[]) ?? [];

      // Build columns from the columns array (preserves order from query)
      const columns: ColumnDef<Record<string, unknown>>[] = columnNames.map((key) => ({
        accessorKey: key,
        header: ({ column }) => (
          <Button
            variant="ghost"
            onClick={() =>
              column.toggleSorting(column.getIsSorted() === "asc")
            }
            className="h-8 px-2 font-semibold"
          >
            {key}
            <ArrowUpDown className="ml-2 h-4 w-4" />
          </Button>
        ),
        cell: ({ row }) => {
          const value = row.getValue(key);
          if (value === null) return <span className="text-muted-foreground italic">null</span>;
          if (typeof value === "object") return JSON.stringify(value);
          return String(value);
        },
      }));

      updateTab(activeTabId, {
        results,
        columns,
        isLoading: false,
        executionTime,
        rowCount: results.length,
        error: null,
      });

      // Add to history
      setQueryHistory((prev) => [
        {
          id: Date.now().toString(),
          query: tab.query,
          timestamp: new Date(),
          executionTime,
          rowCount: results.length,
          success: true,
        },
        ...prev.slice(0, 49), // Keep last 50
      ]);
    } catch (error) {
      const executionTime = Math.round(performance.now() - startTime);
      const errorMessage =
        error instanceof Error ? error.message : "Query failed";
      updateTab(activeTabId, {
        error: errorMessage,
        isLoading: false,
        results: null,
        columns: [],
        executionTime,
        rowCount: null,
      });

      setQueryHistory((prev) => [
        {
          id: Date.now().toString(),
          query: tab.query,
          timestamp: new Date(),
          executionTime,
          rowCount: 0,
          success: false,
        },
        ...prev.slice(0, 49),
      ]);
    }
  }, [activeTabId, tabs, updateTab]);

  const toggleLiveQuery = useCallback(async () => {
    const tab = tabs.find((t) => t.id === activeTabId);
    if (!tab) return;

    if (tab.isLive && tab.unsubscribeFn) {
      // Unsubscribe by calling the function
      try {
        tab.unsubscribeFn();
        updateTab(activeTabId, {
          isLive: false,
          unsubscribeFn: null,
        });
      } catch (error) {
        console.error("Failed to unsubscribe:", error);
      }
    } else {
      // Subscribe
      try {
        // First execute the query to get initial results
        await executeQuery();

        const unsubscribeFn = await subscribe(
          tab.query,
          (event) => {
            // Handle the server message - it contains the data
            const data = event as unknown as Record<string, unknown>[];
            // Build columns from data
            const columns: ColumnDef<Record<string, unknown>>[] =
              Array.isArray(data) && data.length > 0
                ? Object.keys(data[0]).map((key) => ({
                    accessorKey: key,
                    header: ({ column }) => (
                      <Button
                        variant="ghost"
                        onClick={() =>
                          column.toggleSorting(column.getIsSorted() === "asc")
                        }
                        className="h-8 px-2 font-semibold"
                      >
                        {key}
                        <ArrowUpDown className="ml-2 h-4 w-4" />
                      </Button>
                    ),
                    cell: ({ row }) => {
                      const value = row.getValue(key);
                      if (value === null) return <span className="text-muted-foreground italic">null</span>;
                      if (typeof value === "object") return JSON.stringify(value);
                      return String(value);
                    },
                  }))
                : [];

            if (Array.isArray(data)) {
              updateTab(activeTabId, {
                results: data,
                columns,
                rowCount: data.length,
              });
            }
          }
        );

        updateTab(activeTabId, {
          isLive: true,
          unsubscribeFn,
        });
      } catch (error) {
        console.error("Failed to subscribe:", error);
        updateTab(activeTabId, {
          error: `Live query failed: ${error instanceof Error ? error.message : "Unknown error"}`,
        });
      }
    }
  }, [activeTabId, tabs, updateTab, executeQuery]);

  const addTab = useCallback(() => {
    const newId = tabCounter.current.toString();
    tabCounter.current++;
    const newTab: QueryTab = {
      id: newId,
      name: `Query ${newId}`,
      query: "",
      results: null,
      columns: [],
      error: null,
      isLoading: false,
      isLive: false,
      unsubscribeFn: null,
      executionTime: null,
      rowCount: null,
    };
    setTabs((prev) => [...prev, newTab]);
    setActiveTabId(newId);
  }, []);

  const closeTab = useCallback(
    (tabId: string, e: React.MouseEvent) => {
      e.stopPropagation();
      const tab = tabs.find((t) => t.id === tabId);
      if (tab?.isLive && tab.unsubscribeFn) {
        tab.unsubscribeFn();
      }

      if (tabs.length === 1) return;
      const newTabs = tabs.filter((t) => t.id !== tabId);
      setTabs(newTabs);
      if (activeTabId === tabId) {
        setActiveTabId(newTabs[newTabs.length - 1].id);
      }
    },
    [tabs, activeTabId]
  );

  const toggleSchemaNode = useCallback((path: string[]) => {
    console.log('toggleSchemaNode called with path:', path);
    setSchema((prev) => {
      const toggleNode = (nodes: SchemaNode[], pathIndex: number): SchemaNode[] => {
        return nodes.map((node) => {
          if (node.name === path[pathIndex]) {
            if (pathIndex === path.length - 1) {
              // This is the target node - toggle its expansion
              console.log('Toggling node:', node.name, 'from', node.isExpanded, 'to', !node.isExpanded);
              return { ...node, isExpanded: !node.isExpanded };
            }
            // Not the target yet, continue down the path
            return {
              ...node,
              children: node.children
                ? toggleNode(node.children, pathIndex + 1)
                : undefined,
            };
          }
          return node;
        });
      };
      return toggleNode(prev, 0);
    });
  }, []);

  const insertIntoQuery = useCallback(
    (text: string) => {
      updateTab(activeTabId, {
        query: activeTab.query + (activeTab.query ? " " : "") + text,
      });
    },
    [activeTabId, activeTab?.query, updateTab]
  );

  const copyResults = useCallback(() => {
    if (activeTab?.results) {
      navigator.clipboard.writeText(JSON.stringify(activeTab.results, null, 2));
    }
  }, [activeTab?.results]);

  const downloadCsv = useCallback(() => {
    if (!activeTab?.results || activeTab.results.length === 0) return;

    const headers = Object.keys(activeTab.results[0]);
    const csvContent = [
      headers.join(","),
      ...activeTab.results.map((row) =>
        headers
          .map((h) => {
            const val = row[h];
            if (val === null) return "";
            if (typeof val === "string" && val.includes(","))
              return `"${val}"`;
            return String(val);
          })
          .join(",")
      ),
    ].join("\n");

    const blob = new Blob([csvContent], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${activeTab.name.replace(/\s+/g, "_")}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }, [activeTab]);

  const table = useReactTable({
    data: activeTab?.results || [],
    columns: activeTab?.columns || [],
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    onSortingChange: setSorting,
    state: { sorting },
    initialState: { pagination: { pageSize: 50 } },
  });

  // Filter schema nodes based on search
  const filterSchemaNodes = useCallback((nodes: SchemaNode[], filter: string): SchemaNode[] => {
    if (!filter.trim()) return nodes;
    const lowerFilter = filter.toLowerCase();
    
    return nodes.reduce<SchemaNode[]>((acc, node) => {
      const nameMatches = node.name.toLowerCase().includes(lowerFilter);
      
      if (node.children) {
        const filteredChildren = filterSchemaNodes(node.children, filter);
        if (nameMatches || filteredChildren.length > 0) {
          acc.push({
            ...node,
            children: filteredChildren.length > 0 ? filteredChildren : node.children,
            isExpanded: filteredChildren.length > 0 || nameMatches, // Auto-expand matches
          });
        }
      } else if (nameMatches) {
        acc.push(node);
      }
      
      return acc;
    }, []);
  }, []);

  const filteredSchema = filterSchemaNodes(schema, schemaFilter);

  const handleEditorMount = useCallback((editorInstance: editor.IStandaloneCodeEditor, monaco: Monaco) => {
    monacoRef.current = monaco;
    
    // Add Ctrl/Cmd + Enter shortcut
    editorInstance.addCommand(
      monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
      () => {
        executeQuery();
      }
    );

    // Register SQL autocomplete provider with schema information
    const completionProvider = monaco.languages.registerCompletionItemProvider('sql', {
      triggerCharacters: ['.', ' '],
      provideCompletionItems: (model: editor.ITextModel, position: Monaco['Position']) => {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        // Get text before cursor to detect context
        const textBeforeCursor = model.getValueInRange({
          startLineNumber: position.lineNumber,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });

        const suggestions: Monaco['languages']['CompletionItem'][] = [];

        // Check if we're after a dot (table.column or namespace.table)
        const dotMatch = textBeforeCursor.match(/(\w+)\.\s*$/);
        
        if (dotMatch) {
          const prefix = dotMatch[1].toLowerCase();
          
          // Look for matching namespace to suggest tables
          const matchingNamespace = schema.find(ns => ns.name.toLowerCase() === prefix);
          if (matchingNamespace?.children) {
            matchingNamespace.children.forEach(table => {
              suggestions.push({
                label: table.name,
                kind: monaco.languages.CompletionItemKind.Class,
                insertText: table.name,
                detail: `Table in ${matchingNamespace.name}`,
                range,
              } as Monaco['languages']['CompletionItem']);
            });
          }
          
          // Look for matching table to suggest columns
          schema.forEach(ns => {
            ns.children?.forEach(table => {
              if (table.name.toLowerCase() === prefix) {
                table.children?.forEach(col => {
                  suggestions.push({
                    label: col.name,
                    kind: monaco.languages.CompletionItemKind.Field,
                    insertText: col.name,
                    detail: `${col.dataType}${col.isNullable === false ? ' NOT NULL' : ''}${col.isPrimaryKey ? ' (PK)' : ''}`,
                    range,
                  } as Monaco['languages']['CompletionItem']);
                });
              }
            });
          });
        } else {
          // Suggest SQL keywords
          const keywords = [
            'SELECT', 'FROM', 'WHERE', 'AND', 'OR', 'NOT', 'IN', 'LIKE', 'BETWEEN',
            'ORDER BY', 'GROUP BY', 'HAVING', 'LIMIT', 'OFFSET', 'JOIN', 'LEFT JOIN',
            'RIGHT JOIN', 'INNER JOIN', 'OUTER JOIN', 'ON', 'AS', 'DISTINCT', 'COUNT',
            'SUM', 'AVG', 'MIN', 'MAX', 'INSERT INTO', 'VALUES', 'UPDATE', 'SET',
            'DELETE FROM', 'CREATE TABLE', 'CREATE NAMESPACE', 'DROP TABLE', 'ALTER TABLE',
            'NULL', 'TRUE', 'FALSE', 'ASC', 'DESC'
          ];
          
          keywords.forEach(kw => {
            suggestions.push({
              label: kw,
              kind: monaco.languages.CompletionItemKind.Keyword,
              insertText: kw,
              range,
            } as Monaco['languages']['CompletionItem']);
          });

          // Suggest namespaces
          schema.forEach(ns => {
            suggestions.push({
              label: ns.name,
              kind: monaco.languages.CompletionItemKind.Module,
              insertText: ns.name,
              detail: 'Namespace',
              range,
            } as Monaco['languages']['CompletionItem']);

            // Also suggest fully qualified table names
            ns.children?.forEach(table => {
              suggestions.push({
                label: `${ns.name}.${table.name}`,
                kind: monaco.languages.CompletionItemKind.Class,
                insertText: `${ns.name}.${table.name}`,
                detail: `Table in ${ns.name}`,
                range,
              } as Monaco['languages']['CompletionItem']);
            });
          });
        }

        return { suggestions };
      },
    });

    // Cleanup on unmount
    return () => {
      completionProvider.dispose();
    };
  }, [executeQuery, schema]);

  const renderSchemaTree = (nodes: SchemaNode[], path: string[] = []) => {
    return nodes.map((node) => {
      const currentPath = [...path, node.name];
      const isExpanded = node.isExpanded;

      return (
        <div key={currentPath.join(".")} className="select-none">
          <div
            className={cn(
              "flex items-center gap-1 px-2 py-1 hover:bg-muted/50 rounded cursor-pointer text-sm",
              node.type === "column" && "pl-8"
            )}
            onClick={() => {
              if (node.type === "column") {
                insertIntoQuery(node.name);
              } else {
                // Namespaces and tables should expand/collapse
                toggleSchemaNode(currentPath);
              }
            }}
            onDoubleClick={() => {
              // Double-click on table inserts the qualified name
              if (node.type === "table") {
                insertIntoQuery(`${path[path.length - 1]}.${node.name}`);
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
            {/* Show placeholder space for alignment when no children */}
            {(!node.children || node.children.length === 0) && node.type !== "column" && (
              <span className="w-4" />
            )}
            {node.type === "namespace" && (
              <Database className="h-4 w-4 text-blue-500" />
            )}
            {node.type === "table" && (
              <Table2 className="h-4 w-4 text-green-500" />
            )}
            {node.type === "column" && (
              <Columns3 className="h-4 w-4 text-orange-500" />
            )}
            {node.isPrimaryKey && (
              <span className="text-yellow-500 text-xs" title="Primary Key">🔑</span>
            )}
            <span className="truncate">{node.name}</span>
            {node.dataType && (
              <span className="text-xs text-muted-foreground ml-auto flex items-center gap-1">
                {node.dataType}
                {node.isNullable === false && <span className="text-red-400" title="NOT NULL">*</span>}
              </span>
            )}
          </div>
          {isExpanded && node.children && (
            <div className="ml-4">{renderSchemaTree(node.children, currentPath)}</div>
          )}
        </div>
      );
    });
  };

  return (
    <div className="h-[calc(100vh-4rem)] flex flex-col">
      {/* Toolbar - stats and utility buttons only */}
      <div className="border-b bg-muted/30 px-4 py-2 flex items-center gap-2">
        {activeTab?.executionTime !== null && (
          <Badge variant="outline" className="gap-1">
            <Clock className="h-3 w-3" />
            {activeTab.executionTime}ms
          </Badge>
        )}
        {activeTab?.rowCount !== null && (
          <Badge variant="outline">{activeTab.rowCount} rows</Badge>
        )}
        {activeTab?.isLive && (
          <Badge variant="default" className="bg-green-500 animate-pulse gap-1">
            <Zap className="h-3 w-3" />
            Live
          </Badge>
        )}

        <div className="flex-1" />

        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setShowHistory(!showHistory)}
              >
                <Clock className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Query History</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="sm"
                variant="ghost"
                onClick={copyResults}
                disabled={!activeTab?.results}
              >
                <Copy className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Copy as JSON</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                size="sm"
                variant="ghost"
                onClick={downloadCsv}
                disabled={!activeTab?.results}
              >
                <Download className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Download CSV</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button size="sm" variant="ghost" onClick={loadSchema}>
                <RotateCcw className="h-4 w-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Refresh Schema</TooltipContent>
          </Tooltip>

          {/* View toggle */}
          <div className="flex items-center border rounded-md">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  size="sm"
                  variant={resultView === "table" ? "secondary" : "ghost"}
                  onClick={() => setResultView("table")}
                  className="rounded-r-none h-8"
                >
                  <TableIcon className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Table View</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  size="sm"
                  variant={resultView === "json" ? "secondary" : "ghost"}
                  onClick={() => setResultView("json")}
                  className="rounded-l-none h-8"
                >
                  <Code className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>JSON View</TooltipContent>
            </Tooltip>
          </div>
        </TooltipProvider>
      </div>

      {/* Main content */}
      <ResizablePanelGroup direction="horizontal" className="flex-1">
        {/* Schema sidebar */}
        <ResizablePanel defaultSize={20} minSize={15} maxSize={30}>
          <div className="h-full border-r flex flex-col">
            <div className="p-3 border-b font-medium text-sm flex items-center gap-2">
              <Database className="h-4 w-4" />
              Schema Browser
            </div>
            <div className="p-2 border-b">
              <div className="relative">
                <Search className="absolute left-2 top-2.5 h-4 w-4 text-muted-foreground" />
                <Input
                  placeholder="Filter..."
                  value={schemaFilter}
                  onChange={(e) => setSchemaFilter(e.target.value)}
                  className="pl-8 h-9"
                />
              </div>
            </div>
            <ScrollArea className="flex-1 p-2">
              {schemaLoading ? (
                <div className="text-sm text-muted-foreground p-2">
                  Loading schema...
                </div>
              ) : filteredSchema.length === 0 ? (
                <div className="text-sm text-muted-foreground p-2">
                  {schemaFilter ? "No matches found" : "No schemas found"}
                </div>
              ) : (
                renderSchemaTree(filteredSchema)
              )}
            </ScrollArea>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* Editor and results */}
        <ResizablePanel defaultSize={showHistory ? 60 : 80}>
          <ResizablePanelGroup direction="vertical">
            {/* Query tabs and editor */}
            <ResizablePanel defaultSize={40} minSize={20}>
              <div className="h-full flex flex-col">
                {/* Tabs */}
                <div className="border-b flex items-center bg-muted/30">
                  <Tabs value={activeTabId} className="flex-1">
                    <TabsList className="h-9 bg-transparent rounded-none border-0 p-0">
                      {tabs.map((tab) => (
                        <TabsTrigger
                          key={tab.id}
                          value={tab.id}
                          onClick={() => setActiveTabId(tab.id)}
                          className={cn(
                            "relative h-9 rounded-none border-r px-4 data-[state=active]:bg-background",
                            tab.isLive && "text-green-600"
                          )}
                        >
                          {tab.isLive && (
                            <Zap className="h-3 w-3 mr-1 text-green-500" />
                          )}
                          {tab.name}
                          {tabs.length > 1 && (
                            <button
                              onClick={(e) => closeTab(tab.id, e)}
                              className="ml-2 hover:bg-muted rounded p-0.5"
                            >
                              <X className="h-3 w-3" />
                            </button>
                          )}
                        </TabsTrigger>
                      ))}
                    </TabsList>
                  </Tabs>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={addTab}
                    className="h-9 px-2"
                  >
                    <Plus className="h-4 w-4" />
                  </Button>
                </div>

                {/* Editor */}
                <div className="flex-1 relative">
                  <Editor
                    height="100%"
                    defaultLanguage="sql"
                    theme="vs-dark"
                    value={activeTab?.query || ""}
                    onChange={(value) =>
                      updateTab(activeTabId, { query: value || "" })
                    }
                    options={{
                      minimap: { enabled: false },
                      fontSize: 14,
                      lineNumbers: "on",
                      scrollBeyondLastLine: false,
                      automaticLayout: true,
                      tabSize: 2,
                      wordWrap: "on",
                      padding: { top: 8, bottom: 40 },
                    }}
                    onMount={handleEditorMount}
                  />
                  {/* Run/Live buttons - bottom right of editor */}
                  <div className="absolute bottom-2 right-4 flex items-center gap-2 z-10">
                    <TooltipProvider>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            size="sm"
                            onClick={executeQuery}
                            disabled={activeTab?.isLoading}
                            className="gap-2 shadow-md"
                          >
                            <Play className="h-4 w-4" />
                            Run
                          </Button>
                        </TooltipTrigger>
                        <TooltipContent>Execute query (Ctrl+Enter)</TooltipContent>
                      </Tooltip>

                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            size="sm"
                            variant={activeTab?.isLive ? "destructive" : "outline"}
                            onClick={toggleLiveQuery}
                            disabled={activeTab?.isLoading || !activeTab?.query.trim()}
                            className="gap-2 shadow-md"
                          >
                            {activeTab?.isLive ? (
                              <>
                                <ZapOff className="h-4 w-4" />
                                Stop
                              </>
                            ) : (
                              <>
                                <Zap className="h-4 w-4" />
                                Live
                              </>
                            )}
                          </Button>
                        </TooltipTrigger>
                        <TooltipContent>
                          {activeTab?.isLive
                            ? "Stop live subscription"
                            : "Subscribe to live changes"}
                        </TooltipContent>
                      </Tooltip>
                    </TooltipProvider>
                  </div>
                </div>
              </div>
            </ResizablePanel>

            <ResizableHandle withHandle />

            {/* Results */}
            <ResizablePanel defaultSize={60}>
              <div className="h-full flex flex-col">
                <div className="flex-1 overflow-auto">
                  {activeTab?.isLoading ? (
                    <div className="flex items-center justify-center h-full text-muted-foreground">
                      <div className="animate-spin mr-2">⏳</div>
                      Executing query...
                    </div>
                  ) : activeTab?.error ? (
                    <div className="p-4 text-red-500 bg-red-50 dark:bg-red-950/30 m-2 rounded border border-red-200 dark:border-red-800">
                      <pre className="whitespace-pre-wrap font-mono text-sm">
                        {activeTab.error}
                      </pre>
                    </div>
                  ) : activeTab?.results === null ? (
                    <div className="flex items-center justify-center h-full text-muted-foreground">
                      Run a query to see results
                    </div>
                  ) : resultView === "json" ? (
                    /* JSON View */
                    <div className="p-4">
                      <pre className="font-mono text-sm bg-muted/30 p-4 rounded-lg overflow-auto max-h-full">
                        {JSON.stringify(activeTab.results, null, 2)}
                      </pre>
                    </div>
                  ) : (
                    /* Table View */
                    <div className="p-2">
                      <Table>
                        <TableHeader>
                          {table.getHeaderGroups().map((headerGroup) => (
                            <TableRow key={headerGroup.id}>
                              {headerGroup.headers.map((header) => (
                                <TableHead key={header.id} className="bg-muted/50">
                                  {header.isPlaceholder
                                    ? null
                                    : flexRender(
                                        header.column.columnDef.header,
                                        header.getContext()
                                      )}
                                </TableHead>
                              ))}
                            </TableRow>
                          ))}
                        </TableHeader>
                        <TableBody>
                          {table.getRowModel().rows.length === 0 ? (
                            <TableRow>
                              <TableCell 
                                colSpan={activeTab?.columns?.length || 1} 
                                className="h-24 text-center text-muted-foreground"
                              >
                                No results
                              </TableCell>
                            </TableRow>
                          ) : (
                            table.getRowModel().rows.map((row, rowIndex) => (
                              <TableRow key={row.id} className="hover:bg-muted/30">
                                {row.getVisibleCells().map((cell) => {
                                  const colId = cell.column.id;
                                  const value = cell.getValue();
                                  const isSelected = selectedCell?.row === rowIndex && selectedCell?.col === colId;
                                  const displayValue = value === null 
                                    ? <span className="text-muted-foreground italic">null</span>
                                    : typeof value === "object" 
                                      ? JSON.stringify(value)
                                      : String(value);
                                  const isLongText = typeof value === "string" && value.length > 50;
                                  
                                  return (
                                    <ContextMenu key={cell.id}>
                                      <ContextMenuTrigger asChild>
                                        <TableCell 
                                          className={cn(
                                            "font-mono text-sm cursor-pointer max-w-[300px] truncate border",
                                            isSelected 
                                              ? "bg-primary/20 border-primary ring-2 ring-primary ring-inset" 
                                              : "border-transparent hover:bg-muted/50"
                                          )}
                                          onClick={() => setSelectedCell({ row: rowIndex, col: colId })}
                                        >
                                          {isLongText ? (
                                            <span title={value as string}>{displayValue}</span>
                                          ) : (
                                            displayValue
                                          )}
                                        </TableCell>
                                      </ContextMenuTrigger>
                                      <ContextMenuContent>
                                        <ContextMenuItem 
                                          onClick={() => {
                                            setCellViewerContent({ column: colId, value });
                                            setCellViewerOpen(true);
                                          }}
                                        >
                                          <Search className="h-4 w-4 mr-2" />
                                          View Full Content
                                        </ContextMenuItem>
                                        <ContextMenuItem 
                                          onClick={() => {
                                            const text = value === null ? "null" : typeof value === "object" ? JSON.stringify(value, null, 2) : String(value);
                                            navigator.clipboard.writeText(text);
                                          }}
                                        >
                                          <Copy className="h-4 w-4 mr-2" />
                                          Copy Value
                                        </ContextMenuItem>
                                        <ContextMenuItem 
                                          onClick={() => {
                                            const rowData = row.original;
                                            navigator.clipboard.writeText(JSON.stringify(rowData, null, 2));
                                          }}
                                        >
                                          <Copy className="h-4 w-4 mr-2" />
                                          Copy Row as JSON
                                        </ContextMenuItem>
                                      </ContextMenuContent>
                                    </ContextMenu>
                                  );
                                })}
                              </TableRow>
                            ))
                          )}
                        </TableBody>
                      </Table>

                      {/* Pagination */}
                      {table.getPageCount() > 1 && (
                        <div className="flex items-center justify-between px-2 py-4 border-t">
                          <div className="text-sm text-muted-foreground">
                            Page {table.getState().pagination.pageIndex + 1} of{" "}
                            {table.getPageCount()}
                          </div>
                          <div className="flex gap-2">
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => table.previousPage()}
                              disabled={!table.getCanPreviousPage()}
                            >
                              Previous
                            </Button>
                            <Button
                              size="sm"
                              variant="outline"
                              onClick={() => table.nextPage()}
                              disabled={!table.getCanNextPage()}
                            >
                              Next
                            </Button>
                          </div>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>

        {/* Query history sidebar */}
        {showHistory && (
          <>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={20} minSize={15} maxSize={30}>
              <div className="h-full border-l flex flex-col">
                <div className="p-3 border-b font-medium text-sm flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <Clock className="h-4 w-4" />
                    History
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setQueryHistory([])}
                    className="h-6 px-2"
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </div>
                <ScrollArea className="flex-1">
                  {queryHistory.length === 0 ? (
                    <div className="p-4 text-sm text-muted-foreground">
                      No query history
                    </div>
                  ) : (
                    <div className="p-2 space-y-2">
                      {queryHistory.map((item) => (
                        <div
                          key={item.id}
                          className={cn(
                            "p-2 rounded border text-sm cursor-pointer hover:bg-muted/50",
                            item.success
                              ? "border-green-200 dark:border-green-800"
                              : "border-red-200 dark:border-red-800"
                          )}
                          onClick={() => {
                            updateTab(activeTabId, { query: item.query });
                          }}
                        >
                          <div className="font-mono text-xs truncate">
                            {item.query.slice(0, 100)}
                            {item.query.length > 100 && "..."}
                          </div>
                          <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
                            <span>{item.executionTime}ms</span>
                            <span>·</span>
                            <span>{item.rowCount} rows</span>
                            <span>·</span>
                            <span>
                              {item.timestamp.toLocaleTimeString()}
                            </span>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </ScrollArea>
              </div>
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>

      {/* Cell Viewer Dialog */}
      <Dialog open={cellViewerOpen} onOpenChange={setCellViewerOpen}>
        <DialogContent className="max-w-2xl max-h-[80vh]">
          <DialogHeader>
            <DialogTitle className="font-mono text-sm">
              {cellViewerContent?.column}
            </DialogTitle>
          </DialogHeader>
          <ScrollArea className="max-h-[60vh]">
            <pre className="font-mono text-sm whitespace-pre-wrap break-all bg-muted/30 p-4 rounded-lg">
              {cellViewerContent?.value === null 
                ? "null"
                : typeof cellViewerContent?.value === "object"
                  ? JSON.stringify(cellViewerContent.value, null, 2)
                  : String(cellViewerContent?.value ?? "")}
            </pre>
          </ScrollArea>
          <div className="flex justify-end gap-2 pt-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                const text = cellViewerContent?.value === null 
                  ? "null" 
                  : typeof cellViewerContent?.value === "object" 
                    ? JSON.stringify(cellViewerContent.value, null, 2) 
                    : String(cellViewerContent?.value ?? "");
                navigator.clipboard.writeText(text);
              }}
            >
              <Copy className="h-4 w-4 mr-2" />
              Copy
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
