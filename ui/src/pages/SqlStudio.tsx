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
  ChevronLeft,
  ChevronsLeft,
  ChevronsRight,
  Clock,
  Trash2,
  Search,
  MoreHorizontal,
  ArrowUpDown,
  RefreshCw,
  Radio,
  Users,
  User,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ScrollArea } from "@/components/ui/scroll-area";
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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const STORAGE_KEY = "kalamdb-sql-studio-tabs";

// Persisted tab state (subset of QueryTab that we save to localStorage)
interface PersistedTab {
  id: string;
  name: string;
  query: string;
  isLive: boolean;
}

interface PersistedState {
  tabs: PersistedTab[];
  activeTabId: string;
  tabCounter: number;
}

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
  message: string | null; // For DML statements (INSERT/UPDATE/DELETE)
  subscriptionStatus: 'idle' | 'connecting' | 'connected' | 'error'; // Subscription state
  subscriptionLog: string[]; // Log messages for subscription activity
}

interface SchemaNode {
  name: string;
  type: "namespace" | "table" | "column";
  tableType?: "user" | "shared" | "stream"; // For table nodes
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

// Load persisted state from localStorage
function loadPersistedState(): { tabs: QueryTab[]; activeTabId: string; tabCounter: number } {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed: PersistedState = JSON.parse(stored);
      if (parsed.tabs && parsed.tabs.length > 0) {
        const tabs: QueryTab[] = parsed.tabs.map((t) => ({
          id: t.id,
          name: t.name,
          query: t.query,
          results: null,
          columns: [],
          error: null,
          isLoading: false,
          isLive: t.isLive,
          unsubscribeFn: null,
          executionTime: null,
          rowCount: null,
          message: null,
          subscriptionStatus: 'idle' as const,
          subscriptionLog: [],
        }));
        return {
          tabs,
          activeTabId: parsed.activeTabId || tabs[0].id,
          tabCounter: parsed.tabCounter || tabs.length + 1,
        };
      }
    }
  } catch (e) {
    console.error("Failed to load persisted tabs:", e);
  }
  // Default state
  return {
    tabs: [
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
        message: null,
        subscriptionStatus: 'idle' as const,
        subscriptionLog: [],
      },
    ],
    activeTabId: "1",
    tabCounter: 2,
  };
}

export default function SqlStudio() {
  const initialState = loadPersistedState();
  const [tabs, setTabs] = useState<QueryTab[]>(initialState.tabs);
  const [activeTabId, setActiveTabId] = useState(initialState.activeTabId);
  const [schema, setSchema] = useState<SchemaNode[]>([]);
  const [schemaLoading, setSchemaLoading] = useState(true);
  const [queryHistory, setQueryHistory] = useState<QueryHistoryItem[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const [sorting, setSorting] = useState<SortingState>([]);
  const [schemaFilter, setSchemaFilter] = useState("");
  const tabCounter = useRef(initialState.tabCounter);
  const monacoRef = useRef<Monaco | null>(null);

  const activeTab = tabs.find((t) => t.id === activeTabId) || tabs[0];

  // Persist tabs to localStorage whenever tabs or activeTabId changes
  useEffect(() => {
    const persistedState: PersistedState = {
      tabs: tabs.map((t) => ({
        id: t.id,
        name: t.name,
        query: t.query,
        isLive: t.isLive,
      })),
      activeTabId,
      tabCounter: tabCounter.current,
    };
    localStorage.setItem(STORAGE_KEY, JSON.stringify(persistedState));
  }, [tabs, activeTabId]);

  // Load schema on mount
  useEffect(() => {
    loadSchema();
  }, []);

  const loadSchema = async () => {
    setSchemaLoading(true);
    try {
      // Fetch all schema data in a single query using JOINs
      const allData = await executeSql(`
        SELECT 
          t.table_schema,
          t.table_name,
          t.table_type,
          c.column_name,
          c.data_type,
          c.is_nullable,
          c.ordinal_position
        FROM information_schema.tables t
        LEFT JOIN information_schema.columns c 
          ON t.table_schema = c.table_schema AND t.table_name = c.table_name
        ORDER BY t.table_schema, t.table_name, c.ordinal_position
      `);

      // Build the schema tree from the flat results
      const namespaceMap = new Map<string, SchemaNode>();
      const tableMap = new Map<string, SchemaNode>();

      for (const row of allData) {
        const schemaName = row.table_schema as string;
        const tableName = row.table_name as string;
        const tableType = (row.table_type as string)?.toLowerCase() as "user" | "shared" | "stream" | undefined;
        const tableKey = `${schemaName}.${tableName}`;

        // Create or get namespace node
        if (!namespaceMap.has(schemaName)) {
          namespaceMap.set(schemaName, {
            name: schemaName,
            type: "namespace",
            children: [],
            isExpanded: schemaName === "system",
          });
        }

        // Create or get table node
        if (!tableMap.has(tableKey)) {
          const tableNode: SchemaNode = {
            name: tableName,
            type: "table",
            tableType: tableType,
            children: [],
            isExpanded: false,
          };
          tableMap.set(tableKey, tableNode);
          namespaceMap.get(schemaName)!.children!.push(tableNode);
        }

        // Add column if present (LEFT JOIN may produce null columns for tables with no columns)
        if (row.column_name) {
          const columnNode: SchemaNode = {
            name: row.column_name as string,
            type: "column",
            dataType: row.data_type as string,
            isNullable: row.is_nullable === "YES",
            isPrimaryKey: row.ordinal_position === 0,
          };
          tableMap.get(tableKey)!.children!.push(columnNode);
        }
      }

      const namespaces = Array.from(namespaceMap.values());
      console.log(`Loaded schema: ${namespaces.length} namespaces, ${tableMap.size} tables`);
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
      const rowCount = response.results?.[0]?.row_count ?? results.length;
      const message = (response.results?.[0]?.message as string) ?? null;
      
      // Debug: Log what we received from server
      console.log('[SqlStudio] Query response:', { 
        results: results.length, 
        columnNames, 
        rowCount, 
        message,
        rawResponse: response.results?.[0]
      });

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
        rowCount,
        message,
        error: null,
      });

      // Add to history
      setQueryHistory((prev) => [
        {
          id: Date.now().toString(),
          query: tab.query,
          timestamp: new Date(),
          executionTime,
          rowCount,
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
        message: null,
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
        updateTab(activeTabId, {
          subscriptionStatus: "idle",
          subscriptionLog: [...tab.subscriptionLog, `[${new Date().toLocaleTimeString()}] Unsubscribing...`],
        });
        tab.unsubscribeFn();
        updateTab(activeTabId, {
          isLive: false,
          unsubscribeFn: null,
          subscriptionStatus: "idle",
          subscriptionLog: [...tab.subscriptionLog, `[${new Date().toLocaleTimeString()}] Unsubscribed successfully`],
        });
      } catch (error) {
        console.error("Failed to unsubscribe:", error);
        updateTab(activeTabId, {
          isLive: false,
          unsubscribeFn: null,
          subscriptionStatus: "error",
          subscriptionLog: [...tab.subscriptionLog, `[${new Date().toLocaleTimeString()}] Unsubscribe error: ${error instanceof Error ? error.message : "Unknown error"}`],
        });
      }
    } else {
      // Subscribe - DON'T call executeQuery first, let subscribe handle initial data
      // Capture the tab ID to avoid stale closure issues
      const subscribingTabId = activeTabId;
      
      updateTab(subscribingTabId, {
        subscriptionStatus: "connecting",
        subscriptionLog: [`[${new Date().toLocaleTimeString()}] Connecting to live query...`],
        error: null,
      });

      try {
        const unsubscribeFn = await subscribe(
          tab.query,
          (event) => {
            // Debug: Log raw event
            console.log('[SqlStudio] Subscription callback received event:', event);
            console.log('[SqlStudio] Event type:', typeof event, event?.type);
            
            // Check if event is a ServerMessage object with type field
            const serverMsg = event as { 
              type?: string; 
              subscription_id?: string;
              code?: string;
              message?: string;
              rows?: Record<string, unknown>[];
              total_rows?: number;
              batch_control?: { status?: string };
              change_type?: 'insert' | 'update' | 'delete';
            };
            
            // Helper to build columns with _change_type as first column
            const buildColumnsWithChangeType = (sampleRow: Record<string, unknown>) => {
              const dataKeys = Object.keys(sampleRow).filter(k => k !== '_change_type');
              const columns: ColumnDef<Record<string, unknown>>[] = [
                // Change type column first
                {
                  accessorKey: '_change_type',
                  header: () => <span className="font-semibold">Type</span>,
                  cell: ({ row }) => {
                    const changeType = row.getValue('_change_type') as string;
                    const colorClass = {
                      'initial': 'text-blue-600 bg-blue-50 dark:bg-blue-950/50',
                      'insert': 'text-green-600 bg-green-50 dark:bg-green-950/50',
                      'update': 'text-yellow-600 bg-yellow-50 dark:bg-yellow-950/50',
                      'delete': 'text-red-600 bg-red-50 dark:bg-red-950/50',
                    }[changeType] || 'text-muted-foreground';
                    return (
                      <span className={`px-2 py-0.5 rounded text-xs font-medium uppercase ${colorClass}`}>
                        {changeType}
                      </span>
                    );
                  },
                },
                // Data columns
                ...dataKeys.map((key) => ({
                  accessorKey: key,
                  header: ({ column }: { column: { toggleSorting: (asc: boolean) => void; getIsSorted: () => string | false } }) => (
                    <Button
                      variant="ghost"
                      onClick={() => column.toggleSorting(column.getIsSorted() === "asc")}
                      className="h-8 px-2 font-semibold"
                    >
                      {key}
                      <ArrowUpDown className="ml-2 h-4 w-4" />
                    </Button>
                  ),
                  cell: ({ row }: { row: { getValue: (key: string) => unknown } }) => {
                    const value = row.getValue(key);
                    if (value === null) return <span className="text-muted-foreground italic">null</span>;
                    if (typeof value === "object") return JSON.stringify(value);
                    return String(value);
                  },
                })),
              ];
              return columns;
            };
            
            // Handle error messages from server
            if (serverMsg.type === 'error') {
              console.error("Subscription error from server:", serverMsg);
              setTabs((prev) =>
                prev.map((t) =>
                  t.id === subscribingTabId
                    ? {
                        ...t,
                        // Keep isLive flag on - user can retry by clicking Subscribe again
                        unsubscribeFn: null,
                        subscriptionStatus: "error",
                        error: `Live query failed: ${serverMsg.message || serverMsg.code || "Unknown error"}`,
                        subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] Error: ${serverMsg.message || serverMsg.code}`],
                      }
                    : t
                )
              );
              return;
            }

            // Handle subscription acknowledgment
            if (serverMsg.type === 'subscription_ack') {
              setTabs((prev) =>
                prev.map((t) =>
                  t.id === subscribingTabId
                    ? {
                        ...t,
                        subscriptionStatus: "connected",
                        // Clear results for fresh subscription
                        results: [],
                        subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] Subscribed! Total rows: ${serverMsg.total_rows ?? 'unknown'}`],
                      }
                    : t
                )
              );
              return;
            }

            // Handle initial data batch - append with _change_type: 'initial'
            if (serverMsg.type === 'initial_data_batch' && Array.isArray(serverMsg.rows)) {
              // SDK already transforms typed values, just add change type marker
              const newRows = serverMsg.rows.map(row => ({ _change_type: 'initial', ...row }));
              const batchStatus = serverMsg.batch_control?.status;
              
              setTabs((prev) =>
                prev.map((t) => {
                  if (t.id !== subscribingTabId) return t;
                  
                  // Append to existing results
                  const existingResults = t.results || [];
                  const combinedResults = [...existingResults, ...newRows];
                  
                  // Build columns from first row if we have data
                  const columns = combinedResults.length > 0 
                    ? buildColumnsWithChangeType(combinedResults[0])
                    : t.columns;
                  
                  return {
                    ...t,
                    results: combinedResults,
                    columns,
                    rowCount: combinedResults.length,
                    subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] Initial data: +${newRows.length} rows (total: ${combinedResults.length})${batchStatus === 'ready' ? ' ✓' : '...'}`],
                  };
                })
              );
              return;
            }

            // Handle change events (insert/update/delete) - append with change type
            if (serverMsg.type === 'change' && Array.isArray(serverMsg.rows)) {
              const changeType = serverMsg.change_type || 'change';
              // SDK already transforms typed values, just add change type marker
              const newRows = serverMsg.rows.map(row => ({ _change_type: changeType, ...row }));
              
              setTabs((prev) =>
                prev.map((t) => {
                  if (t.id !== subscribingTabId) return t;
                  
                  // Append new changes to existing results
                  const existingResults = t.results || [];
                  const combinedResults = [...existingResults, ...newRows];
                  
                  // Build columns from first row if we have data
                  const columns = combinedResults.length > 0 
                    ? buildColumnsWithChangeType(combinedResults[0])
                    : t.columns;
                  
                  return {
                    ...t,
                    results: combinedResults,
                    columns,
                    rowCount: combinedResults.length,
                    subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] ${changeType.toUpperCase()}: +${newRows.length} rows (total: ${combinedResults.length})`],
                  };
                })
              );
              return;
            }

            // Fallback: Handle raw data array (legacy format)
            const data = event as unknown as Record<string, unknown>[];
            if (Array.isArray(data) && data.length > 0) {
              // SDK already transforms typed values, just add change type marker
              const newRows = data.map(row => ({ _change_type: 'data', ...row }));
              
              setTabs((prev) =>
                prev.map((t) => {
                  if (t.id !== subscribingTabId) return t;
                  
                  const existingResults = t.results || [];
                  const combinedResults = [...existingResults, ...newRows];
                  const columns = combinedResults.length > 0 
                    ? buildColumnsWithChangeType(combinedResults[0])
                    : t.columns;
                  
                  return {
                    ...t,
                    results: combinedResults,
                    columns,
                    rowCount: combinedResults.length,
                    subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] Data: +${newRows.length} rows (total: ${combinedResults.length})`],
                  };
                })
              );
            }
          }
        );

        // Only set isLive if not already in error state (error callback might have fired)
        setTabs((prev) =>
          prev.map((t) =>
            t.id === subscribingTabId && t.subscriptionStatus !== "error"
              ? {
                  ...t,
                  isLive: true,
                  unsubscribeFn,
                  subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] Subscription registered, waiting for server response...`],
                }
              : t
          )
        );

        // Set a timeout to detect if server doesn't respond
        setTimeout(() => {
          setTabs((prev) =>
            prev.map((t) =>
              t.id === subscribingTabId && t.subscriptionStatus === "connecting"
                ? {
                    ...t,
                    subscriptionStatus: "error",
                    error: "Connection timeout: Server did not respond within 10 seconds. Please check your query format (e.g., SELECT * FROM namespace.table).",
                    subscriptionLog: [...t.subscriptionLog, `[${new Date().toLocaleTimeString()}] Timeout: No response from server`],
                  }
                : t
            )
          );
        }, 10000);
      } catch (error) {
        console.error("Failed to subscribe:", error);
        updateTab(subscribingTabId, {
          error: `Live query failed: ${error instanceof Error ? error.message : "Unknown error"}`,
          subscriptionStatus: "error",
          subscriptionLog: [...tab.subscriptionLog, `[${new Date().toLocaleTimeString()}] Error: ${error instanceof Error ? error.message : "Unknown error"}`],
        });
      }
    }
  }, [activeTabId, tabs, updateTab]);

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
      message: null,
      subscriptionStatus: "idle",
      subscriptionLog: [],
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
            {node.type === "table" && !node.tableType && (
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
    <div className="h-[calc(100vh-4rem)] flex">
      {/* Schema sidebar */}
      <div className="w-56 border-r flex flex-col shrink-0">
        <div className="p-2 border-b font-medium text-sm flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Database className="h-4 w-4" />
            Schema Browser
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={loadSchema}
            disabled={schemaLoading}
            className="h-6 w-6 p-0"
            title="Refresh schema"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", schemaLoading && "animate-spin")} />
          </Button>
        </div>
        <div className="p-2 border-b">
          <div className="relative">
            <Search className="absolute left-2 top-2 h-4 w-4 text-muted-foreground" />
            <Input
              placeholder="Filter..."
              value={schemaFilter}
              onChange={(e) => setSchemaFilter(e.target.value)}
              className="pl-8 h-8 text-sm"
            />
          </div>
        </div>
        <ScrollArea className="flex-1 p-2">
          {schemaLoading ? (
            <div className="text-sm text-muted-foreground p-2">Loading schema...</div>
          ) : filteredSchema.length === 0 ? (
            <div className="text-sm text-muted-foreground p-2">
              {schemaFilter ? "No matches found" : "No schemas found"}
            </div>
          ) : (
            renderSchemaTree(filteredSchema)
          )}
        </ScrollArea>
      </div>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Header: Tabs + Run + Live Toggle + Actions */}
        <div className="border-b flex items-center h-12 px-2 gap-2 shrink-0 bg-background">
          {/* Query Tabs */}
          <div className="flex items-center gap-1">
            {tabs.map((tab) => (
              <div
                key={tab.id}
                onClick={() => setActiveTabId(tab.id)}
                className={cn(
                  "flex items-center gap-1 px-3 py-1.5 rounded cursor-pointer text-sm",
                  activeTabId === tab.id
                    ? "bg-muted font-medium"
                    : "hover:bg-muted/50"
                )}
              >
                {/* Green dot for live subscribed tabs */}
                {tab.subscriptionStatus === "connected" && (
                  <span className="relative flex h-2 w-2 mr-1">
                    <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                    <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                  </span>
                )}
                {tab.name}
                {tabs.length > 1 && (
                  <button
                    onClick={(e) => closeTab(tab.id, e)}
                    className="ml-1 hover:bg-muted-foreground/20 rounded p-0.5"
                  >
                    <X className="h-3 w-3" />
                  </button>
                )}
              </div>
            ))}
            <Button size="sm" variant="ghost" onClick={addTab} className="h-7 w-7 p-0">
              <Plus className="h-4 w-4" />
            </Button>
          </div>

          <div className="w-px h-6 bg-border mx-1" />

          {/* Run Button */}
          <Button
            size="sm"
            onClick={activeTab?.isLive ? toggleLiveQuery : executeQuery}
            disabled={activeTab?.isLoading || !activeTab?.query?.trim() || activeTab?.subscriptionStatus === "connecting"}
            className={cn(
              "gap-1.5 h-8",
              activeTab?.isLive && activeTab?.subscriptionStatus === "connected"
                ? "bg-red-600 hover:bg-red-700 text-white"
                : "bg-blue-600 hover:bg-blue-700 text-white"
            )}
          >
            {activeTab?.subscriptionStatus === "connecting" ? (
              <>
                <div className="animate-spin h-3.5 w-3.5 border-2 border-white border-t-transparent rounded-full" />
                Connecting...
              </>
            ) : activeTab?.isLive && activeTab?.subscriptionStatus === "connected" ? (
              <>
                <X className="h-3.5 w-3.5" />
                Stop
              </>
            ) : activeTab?.isLive ? (
              <>
                <Play className="h-3.5 w-3.5" />
                Subscribe
              </>
            ) : (
              <>
                <Play className="h-3.5 w-3.5" />
                Run
              </>
            )}
          </Button>

          {/* Live Query Toggle */}
          <div className="flex items-center gap-2 ml-2">
            <span className="text-sm font-medium">Live Query</span>
            <div className="flex items-center gap-1.5">
              <span className={cn("text-xs", !activeTab?.isLive && "text-muted-foreground")}>
                {activeTab?.isLive ? "ON" : "OFF"}
              </span>
              <Switch
                checked={activeTab?.isLive ?? false}
                onCheckedChange={(checked) => {
                  if (!checked && activeTab?.unsubscribeFn) {
                    // When turning off and subscribed, call toggleLiveQuery to unsubscribe properly
                    toggleLiveQuery();
                  } else if (!checked) {
                    // Just turning off the flag (not subscribed yet)
                    updateTab(activeTabId, { 
                      isLive: false, 
                      subscriptionStatus: "idle",
                    });
                  } else {
                    // Turning on - just set the flag, Run button will subscribe
                    updateTab(activeTabId, { 
                      isLive: true,
                      subscriptionStatus: "idle",
                      subscriptionLog: [],
                    });
                  }
                }}
                className="data-[state=checked]:bg-blue-600"
              />
              {activeTab?.isLive && activeTab?.subscriptionStatus === "connected" && (
                <span className="text-xs text-green-600 flex items-center gap-1">
                  (<span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />)
                </span>
              )}
            </div>
          </div>

          <div className="flex-1" />

          {/* Right side actions */}
          <div className="flex items-center gap-1">
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button size="sm" variant="outline" onClick={() => setShowHistory(!showHistory)} className="h-8 gap-1.5">
                    <Clock className="h-4 w-4" />
                    History
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Query History</TooltipContent>
              </Tooltip>
              <Button size="sm" variant="ghost" className="h-8 w-8 p-0">
                <MoreHorizontal className="h-4 w-4" />
              </Button>
            </TooltipProvider>
          </div>
        </div>

        {/* Editor */}
        <div className="h-32 shrink-0 border-b">
          <Editor
            height="100%"
            defaultLanguage="sql"
            theme="vs-dark"
            value={activeTab?.query || ""}
            onChange={(value) => updateTab(activeTabId, { query: value || "" })}
            options={{
              minimap: { enabled: false },
              fontSize: 13,
              lineNumbers: "on",
              scrollBeyondLastLine: false,
              automaticLayout: true,
              tabSize: 2,
              wordWrap: "on",
              padding: { top: 8, bottom: 8 },
              lineHeight: 20,
            }}
            onMount={handleEditorMount}
          />
        </div>

        {/* Results area */}
        <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
          {/* Live Query Status Bar */}
          {activeTab?.isLive && (
            <div className={cn(
              "px-4 py-2 border-b flex items-center gap-3 shrink-0",
              activeTab.subscriptionStatus === "connected" && "bg-green-50 dark:bg-green-950/30",
              activeTab.subscriptionStatus === "connecting" && "bg-yellow-50 dark:bg-yellow-950/30",
              activeTab.subscriptionStatus === "error" && "bg-red-50 dark:bg-red-950/30",
            )}>
              <div className="flex items-center gap-2">
                {activeTab.subscriptionStatus === "connecting" && (
                  <>
                    <div className="animate-spin h-4 w-4 border-2 border-yellow-500 border-t-transparent rounded-full" />
                    <span className="text-sm text-yellow-600 dark:text-yellow-400">Connecting...</span>
                  </>
                )}
                {activeTab.subscriptionStatus === "connected" && (
                  <>
                    <span className="relative flex h-2 w-2">
                      <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                      <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                    </span>
                    <span className="text-sm text-green-600 dark:text-green-400">Live - Subscribed</span>
                  </>
                )}
                {activeTab.subscriptionStatus === "error" && (
                  <>
                    <span className="h-2 w-2 rounded-full bg-red-500" />
                    <span className="text-sm text-red-600 dark:text-red-400">Connection Error</span>
                  </>
                )}
              </div>
              {/* Subscription Log */}
              {activeTab.subscriptionLog.length > 0 && (
                <div className="flex-1 text-xs text-muted-foreground font-mono truncate">
                  {activeTab.subscriptionLog[activeTab.subscriptionLog.length - 1]}
                </div>
              )}
              {activeTab.subscriptionStatus === "connected" && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={toggleLiveQuery}
                  className="h-6 text-xs"
                >
                  Disconnect
                </Button>
              )}
            </div>
          )}

          {activeTab?.isLoading ? (
            <div className="flex items-center justify-center h-full text-muted-foreground">
              <div className="animate-spin mr-2">⏳</div>
              Executing query...
            </div>
          ) : activeTab?.error ? (
            <div className="p-4 m-2">
              <div className="text-red-500 bg-red-50 dark:bg-red-950/30 rounded border border-red-200 dark:border-red-800 p-4">
                <pre className="whitespace-pre-wrap font-mono text-sm">{activeTab.error}</pre>
              </div>
              {activeTab?.executionTime !== null && (
                <div className="mt-2 text-sm text-muted-foreground">
                  Execution time: {activeTab.executionTime}ms
                </div>
              )}
            </div>
          ) : activeTab?.results === null && !activeTab?.isLive ? (
            <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
              {activeTab?.isLive ? "Click Run to start live query subscription" : "Run a query to see results"}
            </div>
          ) : activeTab?.results === null && activeTab?.isLive && activeTab?.subscriptionStatus !== "connected" ? (
            <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
              {activeTab?.subscriptionStatus === "connecting" ? "Establishing connection..." : "Click Run to start live query subscription"}
            </div>
          ) : activeTab?.message && (!activeTab?.columns || activeTab?.columns?.length === 0) ? (
            // DML statements (INSERT/UPDATE/DELETE) - no columns, just a message
            <div className="p-4 text-green-600 bg-green-50 dark:bg-green-950/30 m-2 rounded border border-green-200 dark:border-green-800">
              <div className="flex items-center gap-2">
                <span className="text-lg">✓</span>
                <span className="font-medium">
                  {activeTab.message}
                </span>
              </div>
              {activeTab?.executionTime !== null && (
                <div className="mt-2 text-sm text-muted-foreground">
                  Execution time: {activeTab.executionTime}ms
                </div>
              )}
            </div>
          ) : (
            // SELECT queries - always show table with headers (even if 0 rows)
            <>
              {/* Table with sheet-like cells */}
              <div className="flex-1 overflow-auto">
                <table className="w-full border-collapse text-sm">
                  <thead className="sticky top-0 z-10">
                    <tr className="bg-muted/80 backdrop-blur">
                      {table.getHeaderGroups().map((headerGroup) =>
                        headerGroup.headers.map((header) => (
                          <th
                            key={header.id}
                            className="border border-border px-3 py-2 text-left font-medium text-muted-foreground whitespace-nowrap"
                          >
                            {header.isPlaceholder ? null : (
                              <button
                                className="flex items-center gap-1 hover:text-foreground"
                                onClick={() => header.column.toggleSorting()}
                              >
                                {flexRender(header.column.columnDef.header, header.getContext())}
                                <ArrowUpDown className="h-3 w-3" />
                              </button>
                            )}
                          </th>
                        ))
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {table.getRowModel().rows.length === 0 ? (
                      <tr>
                        <td
                          colSpan={activeTab?.columns?.length || 1}
                          className="border border-border px-3 py-8 text-center text-muted-foreground"
                        >
                          No results
                        </td>
                      </tr>
                    ) : (
                      table.getRowModel().rows.map((row) => (
                        <tr key={row.id} className="hover:bg-muted/30">
                          {row.getVisibleCells().map((cell) => {
                            const value = cell.getValue();
                            const displayValue =
                              value === null ? (
                                <span className="text-muted-foreground italic">null</span>
                              ) : typeof value === "object" ? (
                                JSON.stringify(value)
                              ) : (
                                String(value)
                              );
                            const isLongText = typeof value === "string" && value.length > 40;

                            return (
                              <td
                                key={cell.id}
                                className="border border-border px-3 py-1.5 font-mono text-sm max-w-[300px] truncate"
                                title={isLongText ? String(value) : undefined}
                              >
                                {displayValue}
                              </td>
                            );
                          })}
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              </div>

              {/* Sticky Pagination Footer */}
              <div className="border-t bg-background px-4 py-2 flex items-center justify-end gap-4 shrink-0">
                <div className="flex items-center gap-2 text-sm">
                  <span className="text-muted-foreground">Rows per page:</span>
                  <Select
                    value={String(table.getState().pagination.pageSize)}
                    onValueChange={(value) => table.setPageSize(Number(value))}
                  >
                    <SelectTrigger className="h-8 w-16">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {[10, 25, 50, 100].map((size) => (
                        <SelectItem key={size} value={String(size)}>
                          {size}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>

                <span className="text-sm text-muted-foreground">
                  {table.getState().pagination.pageIndex * table.getState().pagination.pageSize + 1}-
                  {Math.min(
                    (table.getState().pagination.pageIndex + 1) * table.getState().pagination.pageSize,
                    activeTab?.rowCount ?? 0
                  )}{" "}
                  of {activeTab?.rowCount ?? 0} items
                </span>

                <div className="flex items-center gap-1">
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => table.setPageIndex(0)}
                    disabled={!table.getCanPreviousPage()}
                    className="h-8 w-8 p-0"
                  >
                    <ChevronsLeft className="h-4 w-4" />
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => table.previousPage()}
                    disabled={!table.getCanPreviousPage()}
                    className="h-8 w-8 p-0"
                  >
                    <ChevronLeft className="h-4 w-4" />
                  </Button>

                  {/* Page numbers */}
                  {Array.from({ length: Math.min(5, table.getPageCount()) }, (_, i) => {
                    const pageIndex = table.getState().pagination.pageIndex;
                    const totalPages = table.getPageCount();
                    let pageNum: number;
                    if (totalPages <= 5) {
                      pageNum = i;
                    } else if (pageIndex < 2) {
                      pageNum = i;
                    } else if (pageIndex > totalPages - 3) {
                      pageNum = totalPages - 5 + i;
                    } else {
                      pageNum = pageIndex - 2 + i;
                    }
                    return (
                      <Button
                        key={pageNum}
                        size="sm"
                        variant={pageIndex === pageNum ? "default" : "outline"}
                        onClick={() => table.setPageIndex(pageNum)}
                        className="h-8 w-8 p-0"
                      >
                        {pageNum + 1}
                      </Button>
                    );
                  })}

                  {table.getPageCount() > 5 && (
                    <span className="text-muted-foreground px-1">...</span>
                  )}

                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => table.nextPage()}
                    disabled={!table.getCanNextPage()}
                    className="h-8 w-8 p-0"
                  >
                    <ChevronRight className="h-4 w-4" />
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => table.setPageIndex(table.getPageCount() - 1)}
                    disabled={!table.getCanNextPage()}
                    className="h-8 w-8 p-0"
                  >
                    <ChevronsRight className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Query history sidebar */}
      {showHistory && (
        <div className="w-64 border-l flex flex-col shrink-0">
          <div className="p-2 border-b font-medium text-sm flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Clock className="h-4 w-4" />
              History
            </div>
            <Button size="sm" variant="ghost" onClick={() => setQueryHistory([])} className="h-6 w-6 p-0">
              <Trash2 className="h-3 w-3" />
            </Button>
          </div>
          <ScrollArea className="flex-1">
            {queryHistory.length === 0 ? (
              <div className="p-4 text-sm text-muted-foreground">No query history</div>
            ) : (
              <div className="p-2 space-y-2">
                {queryHistory.map((item) => (
                  <div
                    key={item.id}
                    className={cn(
                      "p-2 rounded border text-sm cursor-pointer hover:bg-muted/50",
                      item.success ? "border-green-200 dark:border-green-800" : "border-red-200 dark:border-red-800"
                    )}
                    onClick={() => updateTab(activeTabId, { query: item.query })}
                  >
                    <div className="font-mono text-xs truncate">
                      {item.query.slice(0, 80)}
                      {item.query.length > 80 && "..."}
                    </div>
                    <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
                      <span>{item.executionTime}ms</span>
                      <span>·</span>
                      <span>{item.rowCount} rows</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </ScrollArea>
        </div>
      )}
    </div>
  );
}
