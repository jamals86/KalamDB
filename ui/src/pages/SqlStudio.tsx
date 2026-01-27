import { useState, useCallback, useRef, useEffect } from "react";
import Editor, { Monaco } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import {
  Play,
  Plus,
  X,
  Table2,
  ChevronRight,
  ChevronLeft,
  ChevronsLeft,
  ChevronsRight,
  Clock,
  Trash2,
  ArrowUpDown,
  Info,
} from "lucide-react";
import { TableProperties } from "@/components/sql-studio/TableProperties";
import { QueryResultsBar } from "@/components/sql-studio/QueryResultsBar";
import { Button } from "@/components/ui/button";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { Switch } from "@/components/ui/switch";
import { CellDisplay } from "@/components/datatype-display";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import { executeSql, executeQuery as executeQueryApi, subscribe, type Unsubscribe } from "@/lib/kalam-client";
import { useDataTypes } from '@/hooks/useDataTypes';
import { extractTableContext } from "@/components/sql-studio/utils/sqlParser";
import { Sidebar } from "@/components/sql-studio/sidebar/Sidebar";
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
  schema: { name: string; data_type: string; index: number }[] | null; // Schema with data types
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
          schema: null,
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
        schema: null,
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
  // Cell selection and context menu state
  const [selectedCell, setSelectedCell] = useState<{ rowIndex: number; columnId: string; value: unknown } | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; value: unknown } | null>(null);
  // WebSocket log modal state
  const [showWsLogModal, setShowWsLogModal] = useState(false);
  // Table properties panel state
  const [showTableProperties, setShowTableProperties] = useState(false);
  const [selectedTable, setSelectedTable] = useState<{ namespace: string; tableName: string; columns: SchemaNode[]; isNewTable?: boolean } | null>(null);
  // Schema tree context menu state
  const [schemaContextMenu, setSchemaContextMenu] = useState<{
    x: number;
    y: number;
    namespace: string;
    tableName: string;
    columns: SchemaNode[];
  } | null>(null);
  const tabCounter = useRef(initialState.tabCounter);
  const monacoRef = useRef<Monaco | null>(null);
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  // Use a ref to store schema so the completion provider always has access to the latest data
  const schemaRef = useRef<SchemaNode[]>([]);
  
  // Data type mappings from system.datatypes
  const { toSqlType } = useDataTypes();

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
      // Update ref so completion provider has access to latest schema
      schemaRef.current = namespaces;
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
      
      // Get result from response - new format uses schema instead of columns
      const result = response.results?.[0] as { 
        schema?: { name: string; data_type: string; index: number }[];
        rows?: unknown[][];
        row_count?: number;
        message?: string;
      } | undefined;
      
      // Extract schema (new format) - array of {name, data_type, index}
      const schema = result?.schema ?? [];
      const columnNames = schema.map((s) => s.name);
      
      // Convert array-based rows to Record objects using schema
      const rawRows = (result?.rows ?? []) as unknown[][];
      const results: Record<string, unknown>[] = rawRows.map((row) => {
        const obj: Record<string, unknown> = {};
        schema.forEach((field) => {
          obj[field.name] = row[field.index] ?? null;
        });
        return obj;
      });
      
      const rowCount = result?.row_count ?? results.length;
      const message = result?.message ?? null;
      
      // Debug: Log what we received from server
      console.log('[SqlStudio] Query response:', { 
        results: results.length, 
        columnNames, 
        schema,
        rowCount, 
        message,
        rawResponse: result
      });

      // Build columns from the schema array (preserves order from query)
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
        schema,
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
        schema: null,
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
      // Unsubscribe by calling the function - keep isLive flag ON so user can resubscribe easily
      try {
        updateTab(activeTabId, {
          subscriptionStatus: "idle",
          subscriptionLog: [...tab.subscriptionLog, `[${new Date().toLocaleTimeString()}] Unsubscribing...`],
        });
        tab.unsubscribeFn();
        updateTab(activeTabId, {
          isLive: true, // Keep isLive ON - user can click Subscribe again
          unsubscribeFn: null,
          subscriptionStatus: "idle",
          subscriptionLog: [...tab.subscriptionLog, `[${new Date().toLocaleTimeString()}] Unsubscribed successfully`],
        });
      } catch (error) {
        console.error("Failed to unsubscribe:", error);
        updateTab(activeTabId, {
          isLive: true, // Keep isLive ON even on error
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
            
            // Helper to build columns with _received_at timestamp and _change_type as first columns
            const buildColumnsWithChangeType = (sampleRow: Record<string, unknown>) => {
              const dataKeys = Object.keys(sampleRow).filter(k => k !== '_change_type' && k !== '_received_at');
              const columns: ColumnDef<Record<string, unknown>>[] = [
                // Timestamp column first - when event was received
                {
                  accessorKey: '_received_at',
                  header: () => (
                    <span className="font-semibold text-muted-foreground" title="When this event was received (UI-only, not queryable)">
                      ⏱ Received
                    </span>
                  ),
                  cell: ({ row }) => {
                    const timestamp = row.getValue('_received_at') as string;
                    return (
                      <span className="text-xs text-muted-foreground font-mono">
                        {timestamp}
                      </span>
                    );
                  },
                },
                // Change type column - with indicator that it's not a queryable column
                {
                  accessorKey: '_change_type',
                  header: () => (
                    <span className="font-semibold text-muted-foreground" title="Event type indicator (UI-only, not a table column)">
                      📌 Type
                    </span>
                  ),
                  cell: ({ row }) => {
                    const changeType = row.getValue('_change_type') as string;
                    const colorClass = {
                      'initial': 'text-blue-600 bg-blue-50 dark:bg-blue-950/50',
                      'insert': 'text-green-600 bg-green-50 dark:bg-green-950/50',
                      'update': 'text-yellow-600 bg-yellow-50 dark:bg-yellow-950/50',
                      'delete': 'text-red-600 bg-red-50 dark:bg-red-950/50',
                    }[changeType] || 'text-muted-foreground';
                    return (
                      <span className={`px-2 py-0.5 rounded text-xs font-medium uppercase ${colorClass}`} title="Event type - not a table column">
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

            // Handle initial data batch - append with _change_type: 'initial' and _received_at timestamp
            if (serverMsg.type === 'initial_data_batch' && Array.isArray(serverMsg.rows)) {
              // SDK already transforms typed values, add change type marker and received timestamp
              const receivedAt = new Date().toLocaleTimeString();
              const newRows = serverMsg.rows.map(row => ({ _received_at: receivedAt, _change_type: 'initial', ...row }));
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

            // Handle change events (insert/update/delete) - append with change type and timestamp
            if (serverMsg.type === 'change' && Array.isArray(serverMsg.rows)) {
              const changeType = serverMsg.change_type || 'change';
              // SDK already transforms typed values, add change type marker and received timestamp
              const receivedAt = new Date().toLocaleTimeString();
              const newRows = serverMsg.rows.map(row => ({ _received_at: receivedAt, _change_type: changeType, _isNew: true, ...row }));
              
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
              // SDK already transforms typed values, add change type marker and received timestamp
              const receivedAt = new Date().toLocaleTimeString();
              const newRows = data.map(row => ({ _received_at: receivedAt, _change_type: 'data', ...row }));
              
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
      schema: null,
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

  // Export results to CSV
  const exportToCSV = useCallback(() => {
    const tab = tabs.find((t) => t.id === activeTabId);
    if (!tab?.results || tab.results.length === 0) return;

    // Get column names from the first row, excluding internal fields
    const columnKeys = Object.keys(tab.results[0]).filter(
      (key) => !key.startsWith("_")
    );

    // Build CSV content
    const csvRows: string[] = [];
    
    // Header row
    csvRows.push(columnKeys.map((key) => `"${key}"`).join(","));
    
    // Data rows
    for (const row of tab.results) {
      const values = columnKeys.map((key) => {
        const value = row[key];
        if (value === null || value === undefined) return "";
        if (typeof value === "string") {
          // Escape quotes and wrap in quotes
          return `"${value.replace(/"/g, '""')}"`;
        }
        if (typeof value === "object") {
          return `"${JSON.stringify(value).replace(/"/g, '""')}"`;
        }
        return String(value);
      });
      csvRows.push(values.join(","));
    }

    const csvContent = csvRows.join("\n");
    const blob = new Blob([csvContent], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.setAttribute("href", url);
    link.setAttribute("download", `query_results_${Date.now()}.csv`);
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  }, [activeTabId, tabs]);

  // Handle showing table properties
  const handleShowTableProperties = useCallback((namespace: string, tableName: string, columns: SchemaNode[]) => {
    setSelectedTable({ namespace, tableName, columns });
    setShowTableProperties(true);
  }, []);

  const handleEditorMount = useCallback((editorInstance: editor.IStandaloneCodeEditor, monaco: Monaco) => {
    monacoRef.current = monaco;
    editorRef.current = editorInstance;
    
    // Add Ctrl/Cmd + Enter shortcut
    editorInstance.addCommand(
      monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter,
      () => {
        executeQuery();
      }
    );

    // Helper function to extract table references from query text
    const extractTableReferences = (queryText: string): { namespace: string | null; table: string; alias: string | null }[] => {
      const tables: { namespace: string | null; table: string; alias: string | null }[] = [];
      // Match patterns like: FROM namespace.table [AS] alias, FROM table [AS] alias
      // Also matches JOIN statements
      const tablePattern = /(?:FROM|JOIN)\s+(?:(\w+)\.)?(\w+)(?:\s+(?:AS\s+)?(\w+))?/gi;
      let match;
      while ((match = tablePattern.exec(queryText)) !== null) {
        tables.push({
          namespace: match[1] || null,
          table: match[2],
          alias: match[3] || null,
        });
      }
      return tables;
    };

    // Helper function to find columns for a table reference
    const findTableColumns = (tableName: string, namespace: string | null): SchemaNode[] => {
      const currentSchema = schemaRef.current;
      for (const ns of currentSchema) {
        if (namespace && ns.name.toLowerCase() !== namespace.toLowerCase()) continue;
        for (const table of ns.children || []) {
          if (table.name.toLowerCase() === tableName.toLowerCase()) {
            return table.children || [];
          }
        }
      }
      return [];
    };

    // Register SQL autocomplete provider with schema information
    const completionProvider = monaco.languages.registerCompletionItemProvider('sql', {
      triggerCharacters: ['.', ' ', ','],
      provideCompletionItems: (model: editor.ITextModel, position: Monaco['Position']) => {
        // Use schemaRef.current to always get the latest schema
        const currentSchema = schemaRef.current;
        
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        // Get full query text and text before cursor for context detection
        const fullQuery = model.getValue();
        const textBeforeCursor = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });
        const lineBeforeCursor = model.getValueInRange({
          startLineNumber: position.lineNumber,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });

        const suggestions: Monaco['languages']['CompletionItem'][] = [];

        // Check if we're after a dot (table.column, namespace.table, or alias.column)
        const dotMatch = lineBeforeCursor.match(/(\w+)\.\s*$/);
        
        if (dotMatch) {
          const prefix = dotMatch[1].toLowerCase();
          
          // Look for matching namespace to suggest tables
          const matchingNamespace = currentSchema.find(ns => ns.name.toLowerCase() === prefix);
          if (matchingNamespace?.children) {
            matchingNamespace.children.forEach(table => {
              suggestions.push({
                label: table.name,
                kind: monaco.languages.CompletionItemKind.Class,
                insertText: table.name,
                detail: `${table.tableType || 'user'} table in ${matchingNamespace.name}`,
                documentation: `${table.children?.length || 0} columns`,
                range,
              } as Monaco['languages']['CompletionItem']);
            });
          }
          
          // Check if prefix is a table alias from the query
          const tableRefs = extractTableReferences(fullQuery);
          const aliasMatch = tableRefs.find(ref => ref.alias?.toLowerCase() === prefix);
          if (aliasMatch) {
            const columns = findTableColumns(aliasMatch.table, aliasMatch.namespace);
            columns.forEach(col => {
              suggestions.push({
                label: col.name,
                kind: monaco.languages.CompletionItemKind.Field,
                insertText: col.name,
                detail: `${col.dataType}${col.isNullable === false ? ' NOT NULL' : ''}${col.isPrimaryKey ? ' (PK)' : ''}`,
                documentation: `Column from ${aliasMatch.table}`,
                range,
              } as Monaco['languages']['CompletionItem']);
            });
          }
          
          // Look for matching table to suggest columns
          currentSchema.forEach(ns => {
            ns.children?.forEach(table => {
              if (table.name.toLowerCase() === prefix) {
                table.children?.forEach(col => {
                  suggestions.push({
                    label: col.name,
                    kind: monaco.languages.CompletionItemKind.Field,
                    insertText: col.name,
                    detail: `${col.dataType}${col.isNullable === false ? ' NOT NULL' : ''}${col.isPrimaryKey ? ' (PK)' : ''}`,
                    documentation: `Column from ${ns.name}.${table.name}`,
                    range,
                  } as Monaco['languages']['CompletionItem']);
                });
              }
            });
          });
          
          return { suggestions };
        }
        
        // Check context: are we after FROM, JOIN, UPDATE, INTO, etc.?
        const upperText = textBeforeCursor.toUpperCase();
        const isAfterTableKeyword = /(?:FROM|JOIN|UPDATE|INTO|TABLE)\s+\w*$/i.test(lineBeforeCursor);
        const isAfterSelect = /SELECT\s+(?:[\w\s,.*`"[\]]+)?\s*$/i.test(lineBeforeCursor) && !/FROM/i.test(lineBeforeCursor);
        const isAfterWhere = /WHERE\s+/i.test(upperText) && !/SELECT\s+/i.test(lineBeforeCursor.toUpperCase());
        const isAfterOn = /\bON\s+\w*$/i.test(lineBeforeCursor);
        const isAfterComma = /,\s*$/i.test(lineBeforeCursor);
        
        // Get table references from query for column suggestions
        const tableRefs = extractTableReferences(fullQuery);
        
        // After FROM, JOIN, UPDATE, INTO - suggest tables
        if (isAfterTableKeyword) {
          // First suggest namespaces
          currentSchema.forEach(ns => {
            suggestions.push({
              label: ns.name,
              kind: monaco.languages.CompletionItemKind.Module,
              insertText: ns.name,
              detail: 'Namespace',
              documentation: `${ns.children?.length || 0} tables`,
              range,
              sortText: '0' + ns.name, // Sort namespaces first
            } as Monaco['languages']['CompletionItem']);

            // Suggest fully qualified table names with higher priority for system tables
            ns.children?.forEach(table => {
              const isSystem = ns.name === 'system' || ns.name === 'information_schema';
              suggestions.push({
                label: `${ns.name}.${table.name}`,
                kind: monaco.languages.CompletionItemKind.Class,
                insertText: `${ns.name}.${table.name}`,
                detail: `${table.tableType || 'user'} table`,
                documentation: `${table.children?.length || 0} columns`,
                range,
                sortText: (isSystem ? '1' : '2') + ns.name + '.' + table.name,
              } as Monaco['languages']['CompletionItem']);
            });
          });
          
          return { suggestions };
        }
        
        // After SELECT or after comma in SELECT - suggest columns and aggregate functions
        if (isAfterSelect || (isAfterComma && !isAfterTableKeyword)) {
          // Suggest aggregate functions
          const aggregates = ['COUNT', 'SUM', 'AVG', 'MIN', 'MAX', 'COUNT(*)'];
          aggregates.forEach(fn => {
            suggestions.push({
              label: fn,
              kind: monaco.languages.CompletionItemKind.Function,
              insertText: fn.includes('(') ? fn : fn + '(',
              detail: 'Aggregate function',
              range,
              sortText: '0' + fn,
            } as Monaco['languages']['CompletionItem']);
          });
          
          // Suggest * for all columns
          suggestions.push({
            label: '*',
            kind: monaco.languages.CompletionItemKind.Operator,
            insertText: '*',
            detail: 'All columns',
            range,
            sortText: '00',
          } as Monaco['languages']['CompletionItem']);
          
          // If tables are referenced in query, suggest their columns
          if (tableRefs.length > 0) {
            tableRefs.forEach(ref => {
              const columns = findTableColumns(ref.table, ref.namespace);
              const prefix = ref.alias || (ref.namespace ? `${ref.namespace}.${ref.table}` : ref.table);
              columns.forEach(col => {
                suggestions.push({
                  label: `${prefix}.${col.name}`,
                  kind: monaco.languages.CompletionItemKind.Field,
                  insertText: `${prefix}.${col.name}`,
                  detail: col.dataType || 'unknown',
                  documentation: `Column from ${ref.namespace ? ref.namespace + '.' : ''}${ref.table}`,
                  range,
                  sortText: '1' + prefix + col.name,
                } as Monaco['languages']['CompletionItem']);
              });
            });
          }
          
          return { suggestions };
        }
        
        // After WHERE or ON - suggest columns from referenced tables
        if (isAfterWhere || isAfterOn) {
          tableRefs.forEach(ref => {
            const columns = findTableColumns(ref.table, ref.namespace);
            const prefix = ref.alias || ref.table;
            columns.forEach(col => {
              suggestions.push({
                label: `${prefix}.${col.name}`,
                kind: monaco.languages.CompletionItemKind.Field,
                insertText: `${prefix}.${col.name}`,
                detail: col.dataType || 'unknown',
                range,
                sortText: '0' + prefix + col.name,
              } as Monaco['languages']['CompletionItem']);
            });
          });
          
          // Also suggest comparison operators and keywords
          const whereKeywords = ['AND', 'OR', 'NOT', 'IN', 'LIKE', 'BETWEEN', 'IS NULL', 'IS NOT NULL', '=', '!=', '<', '>', '<=', '>='];
          whereKeywords.forEach(kw => {
            suggestions.push({
              label: kw,
              kind: monaco.languages.CompletionItemKind.Keyword,
              insertText: kw,
              range,
              sortText: '2' + kw,
            } as Monaco['languages']['CompletionItem']);
          });
          
          return { suggestions };
        }
        
        // Default: suggest SQL keywords, namespaces, and tables
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
            sortText: '1' + kw,
          } as Monaco['languages']['CompletionItem']);
        });

        // Suggest namespaces
        currentSchema.forEach(ns => {
          suggestions.push({
            label: ns.name,
            kind: monaco.languages.CompletionItemKind.Module,
            insertText: ns.name,
            detail: 'Namespace',
            documentation: `${ns.children?.length || 0} tables`,
            range,
            sortText: '2' + ns.name,
          } as Monaco['languages']['CompletionItem']);

          // Also suggest fully qualified table names
          ns.children?.forEach(table => {
            suggestions.push({
              label: `${ns.name}.${table.name}`,
              kind: monaco.languages.CompletionItemKind.Class,
              insertText: `${ns.name}.${table.name}`,
              detail: `${table.tableType || 'user'} table in ${ns.name}`,
              documentation: `${table.children?.length || 0} columns`,
              range,
              sortText: '3' + ns.name + '.' + table.name,
            } as Monaco['languages']['CompletionItem']);
          });
        });

        return { suggestions };
      },
    });

    // Cleanup on unmount
    return () => {
      completionProvider.dispose();
    };
  }, [executeQuery]);

  return (
    <div className="h-[calc(100vh-4rem)] flex">
      {/* Schema sidebar - Fixed width */}
      <div className="w-64 border-r bg-muted/30 flex-shrink-0">
        <Sidebar
          schema={filteredSchema}
          schemaFilter={schemaFilter}
          schemaLoading={schemaLoading}
          onFilterChange={setSchemaFilter}
          onRefreshSchema={loadSchema}
          onCreateTable={() => {
            setSelectedTable({
              namespace: "default",
              tableName: "new_table",
              columns: [],
              isNewTable: true,
            });
            setShowTableProperties(true);
          }}
          onToggleNode={toggleSchemaNode}
          onInsertText={insertIntoQuery}
          onShowTableProperties={handleShowTableProperties}
          onTableContextMenu={(e, namespace, tableName, columns) => {
            setSchemaContextMenu({
              x: e.clientX,
              y: e.clientY,
              namespace,
              tableName,
              columns,
            });
          }}
        />
      </div>

      {/* Main content */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Header: Tabs + Run Button | Live Query + Actions */}
        <div className="border-b flex items-center h-12 px-4 gap-2 shrink-0 bg-background">
          {/* Left side: Query Tabs + Run Button */}
          <div className="flex items-center gap-2">
            {/* Query Tabs */}
            <div className="flex items-center gap-1">
              {tabs.map((tab) => (
                <div
                  key={tab.id}
                  onClick={() => setActiveTabId(tab.id)}
                  className={cn(
                    "flex items-center gap-2 px-4 py-2 cursor-pointer text-sm border-b-2 transition-colors",
                    activeTabId === tab.id
                      ? "border-blue-600 text-foreground font-medium bg-muted/30"
                      : "border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/20"
                  )}
                >
                  {/* Green dot for live subscribed tabs */}
                  {tab.subscriptionStatus === "connected" && (
                    <span className="relative flex h-2 w-2">
                      <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                      <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
                    </span>
                  )}
                  {tab.name}
                  {tabs.length > 1 && (
                    <button
                      onClick={(e) => closeTab(tab.id, e)}
                      className="ml-1 hover:bg-muted-foreground/20 rounded p-0.5 transition-colors"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  )}
                </div>
              ))}
              <Button size="sm" variant="ghost" onClick={addTab} className="h-8 w-8 p-0 ml-2">
                <Plus className="h-4 w-4" />
              </Button>
            </div>

            <div className="w-px h-6 bg-border mx-2" />

            {/* Run Button */}
            <Button
              size="sm"
              onClick={activeTab?.isLive ? toggleLiveQuery : executeQuery}
              disabled={activeTab?.isLoading || !activeTab?.query?.trim() || activeTab?.subscriptionStatus === "connecting"}
              className={cn(
                "gap-2 h-9 px-4 font-medium",
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
          </div>

          <div className="flex-1" />

          {/* Right side: Live Query Toggle + Actions */}
          <div className="flex items-center gap-3">
            {/* Live Query Toggle */}
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium">Live Query</span>
              <div className="flex items-center gap-1.5">
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
                    <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />
                  </span>
                )}
              </div>
            </div>

            <div className="w-px h-6 bg-border" />

            {/* Actions */}
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
            </TooltipProvider>
          </div>
        </div>

        {/* Editor and Results - Resizable Vertical Split */}
        <ResizablePanelGroup orientation="vertical" className="flex-1">
          {/* Editor Panel */}
          <ResizablePanel defaultSize={35} minSize={15} maxSize={70}>
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
          </ResizablePanel>

          <ResizableHandle withHandle />

          {/* Results Panel */}
          <ResizablePanel defaultSize={65}>
            <div className="h-full flex flex-col overflow-hidden">
          {/* Query Results Status Bar - show for non-live queries with results */}
          {!activeTab?.isLive && (activeTab?.results !== null || activeTab?.error) && (
            <QueryResultsBar
              success={!activeTab?.error}
              rowCount={activeTab?.rowCount ?? null}
              executionTime={activeTab?.executionTime ?? null}
              error={activeTab?.error ?? null}
              onExport={exportToCSV}
            />
          )}

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
              {/* View WebSocket Log button - show when there are log entries */}
              {activeTab.subscriptionLog.length > 0 && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setShowWsLogModal(true)}
                  className="h-6 text-xs gap-1"
                >
                  <span>📜</span> Log ({activeTab.subscriptionLog.length})
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
                {(() => {
                  // Try to parse JSON error and extract only the message
                  try {
                    const parsed = JSON.parse(activeTab.error);
                    if (parsed.error?.message) {
                      return (
                        <div className="space-y-2">
                          <div className="font-medium text-red-600 dark:text-red-400">
                            {parsed.error.code || 'Error'}
                          </div>
                          <pre className="whitespace-pre-wrap font-mono text-sm">{parsed.error.message}</pre>
                        </div>
                      );
                    }
                  } catch {
                    // Not JSON or doesn't have error.message structure
                  }
                  // Fallback: display as-is but try to extract message if it looks like JSON
                  const jsonMatch = activeTab.error.match(/"message"\s*:\s*"([^"]+)"/)
                  if (jsonMatch) {
                    return <pre className="whitespace-pre-wrap font-mono text-sm">{jsonMatch[1]}</pre>;
                  }
                  return <pre className="whitespace-pre-wrap font-mono text-sm">{activeTab.error}</pre>;
                })()}
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
              <div className="flex-1 overflow-auto bg-background">
                <div className="overflow-x-auto w-full">
                <table className="text-sm min-w-full">
                  <thead className="sticky top-0 z-10 bg-muted/50">
                    <tr className="border-b">
                      {table.getHeaderGroups().map((headerGroup) =>
                        headerGroup.headers.map((header) => (
                          <th
                            key={header.id}
                            className="px-4 py-3 text-left font-semibold text-xs uppercase tracking-wider text-muted-foreground whitespace-nowrap min-w-[150px] border-r border-border/40 last:border-r-0"
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
                          className="px-4 py-12 text-center text-muted-foreground text-sm"
                        >
                          No results
                        </td>
                      </tr>
                    ) : (
                      table.getRowModel().rows.map((row) => {
                        const rowData = row.original as Record<string, unknown>;
                        const isNewRow = rowData._isNew === true;
                        
                        return (
                          <tr 
                            key={row.id} 
                            className={cn(
                              "border-b border-border/40 hover:bg-muted/30 transition-colors",
                              isNewRow && "animate-pulse bg-green-50 dark:bg-green-950/30"
                            )}
                            onAnimationEnd={() => {
                              // Clear the _isNew flag after animation completes
                              if (isNewRow) {
                                setTabs(prev => prev.map(t => {
                                  if (t.id !== activeTabId || !t.results) return t;
                                  return {
                                    ...t,
                                    results: t.results.map((r, idx) => 
                                      idx === row.index ? { ...r, _isNew: false } : r
                                    )
                                  };
                                }));
                              }
                            }}
                          >
                            {row.getVisibleCells().map((cell) => {
                              const value = cell.getValue();
                              const columnName = cell.column.id;
                              
                              // Get column schema for datatype information
                              const columnSchema = activeTab?.schema?.find(s => s.name === columnName);
                              
                              // Extract data type (handle both string and object types)
                              const dataType = typeof columnSchema?.data_type === 'string' 
                                ? columnSchema.data_type 
                                : typeof columnSchema?.data_type === 'object' 
                                  ? Object.keys(columnSchema.data_type)[0]
                                  : undefined;
                              
                              // Extract table context for FILE download URLs
                              const tableContext = activeTab?.query ? extractTableContext(activeTab.query) : null;
                              
                              const isLongText = typeof value === "string" && value.length > 40;
                              const isSelected = selectedCell?.rowIndex === row.index && selectedCell?.columnId === cell.column.id;

                              return (
                                <td
                                  key={cell.id}
                                  className={cn(
                                    "px-4 py-2.5 text-sm min-w-[150px] max-w-[500px] cursor-pointer select-text border-r border-border/30 last:border-r-0",
                                    isSelected && "ring-2 ring-blue-500 ring-inset bg-blue-50 dark:bg-blue-950/30"
                                  )}
                                  title={isLongText ? String(value) : undefined}
                                  onClick={() => setSelectedCell({ rowIndex: row.index, columnId: cell.column.id, value })}
                                  onContextMenu={(e) => {
                                    e.preventDefault();
                                    setSelectedCell({ rowIndex: row.index, columnId: cell.column.id, value });
                                    // Only show context menu if there's data worth viewing
                                    if (value !== null && (typeof value === "object" || (typeof value === "string" && value.length > 40))) {
                                      setContextMenu({ x: e.clientX, y: e.clientY, value });
                                    }
                                  }}
                                >
                                  <CellDisplay
                                    value={value}
                                    dataType={dataType}
                                    namespace={tableContext?.namespace}
                                    tableName={tableContext?.tableName}
                                  />
                                </td>
                              );
                            })}
                          </tr>
                        );
                      })
                    )}
                  </tbody>
                </table>
                </div>
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
          </ResizablePanel>
        </ResizablePanelGroup>
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

      {/* Table Properties Panel */}
      {showTableProperties && selectedTable && (
        <TableProperties
          tableName={selectedTable.tableName}
          namespace={selectedTable.namespace}
          columns={selectedTable.columns.map((col) => ({
            name: col.name,
            dataType: col.dataType || "unknown",
            sqlType: toSqlType(col.dataType || "unknown"),
            isNullable: col.isNullable,
            isPrimaryKey: col.isPrimaryKey,
          }))}
          metadata={{
            engine: "System",
            rowCount: activeTab?.rowCount ?? undefined,
            createdAt: new Date().toISOString().split("T")[0],
          }}
          isNewTable={selectedTable.isNewTable}
          onClose={() => setShowTableProperties(false)}
          onAlter={() => {
            // Insert ALTER TABLE statement
            updateTab(activeTabId, {
              query: `ALTER TABLE ${selectedTable.namespace}.${selectedTable.tableName} `,
            });
            setShowTableProperties(false);
          }}
          onDrop={() => {
            // Insert DROP TABLE statement
            updateTab(activeTabId, {
              query: `DROP TABLE ${selectedTable.namespace}.${selectedTable.tableName}`,
            });
            setShowTableProperties(false);
          }}
          onCopyDDL={() => {
            // Generate and copy DDL
            const columns = selectedTable.columns
              .map((col) => `  ${col.name} ${toSqlType(col.dataType || "unknown")}${col.isNullable === false ? " NOT NULL" : ""}`)
              .join(",\n");
            const ddl = `CREATE TABLE ${selectedTable.namespace}.${selectedTable.tableName} (\n${columns}\n)`;
            navigator.clipboard.writeText(ddl);
          }}
          onEditColumn={(columnName) => {
            // Insert ALTER TABLE ... MODIFY COLUMN statement
            const col = selectedTable.columns.find((c) => c.name === columnName);
            if (col) {
              updateTab(activeTabId, {
                query: `ALTER TABLE ${selectedTable.namespace}.${selectedTable.tableName} MODIFY COLUMN ${columnName} ${toSqlType(col.dataType || "unknown")}`,
              });
            }
          }}
          onAddColumn={() => {
            // Insert ALTER TABLE ... ADD COLUMN statement (using KalamDataType)
            updateTab(activeTabId, {
              query: `ALTER TABLE ${selectedTable.namespace}.${selectedTable.tableName} ADD COLUMN new_column STRING`,
            });
          }}
          onCreateTable={() => {
            // Insert CREATE TABLE statement
            const columns = selectedTable.columns.length > 0
              ? selectedTable.columns
                  .map((col) => `  ${col.name} ${toSqlType(col.dataType || "unknown")}${col.isNullable === false ? " NOT NULL" : ""}`)
                  .join(",\n")
              : "  id BIGINT NOT NULL,\n  created_at TIMESTAMP";
            const ddl = `CREATE TABLE ${selectedTable.namespace || "default"}.${selectedTable.tableName || "new_table"} (\n${columns}\n)`;
            updateTab(activeTabId, { query: ddl });
            setShowTableProperties(false);
          }}
          toSqlType={toSqlType}
        />
      )}

      {/* Context menu for viewing cell data */}
      {contextMenu && (
        <>
          {/* Backdrop to close context menu on click outside */}
          <div 
            className="fixed inset-0 z-40" 
            onClick={() => setContextMenu(null)}
          />
          <div
            className="fixed z-50 bg-background border border-border rounded-md shadow-lg py-1 min-w-[120px]"
            style={{ 
              left: contextMenu.x, 
              top: contextMenu.y,
              // Ensure menu stays within viewport
              maxHeight: 'calc(100vh - 100px)',
            }}
          >
            <button
              className="w-full px-3 py-2 text-sm text-left hover:bg-muted flex items-center gap-2"
              onClick={() => {
                // Show full data in a dialog/modal
                const value = contextMenu.value;
                const formattedValue = typeof value === "object" 
                  ? JSON.stringify(value, null, 2) 
                  : String(value);
                
                // Create a simple modal to show the data
                const modal = document.createElement('div');
                modal.className = 'fixed inset-0 z-50 flex items-center justify-center bg-black/50';
                modal.innerHTML = `
                  <div class="bg-background border rounded-lg shadow-xl max-w-2xl max-h-[80vh] overflow-auto p-4 m-4">
                    <div class="flex justify-between items-center mb-3">
                      <h3 class="font-semibold text-lg">Cell Data</h3>
                      <button class="p-1 hover:bg-muted rounded" id="close-modal">✕</button>
                    </div>
                    <pre class="font-mono text-sm whitespace-pre-wrap bg-muted/50 p-3 rounded overflow-auto max-h-[60vh]">${formattedValue.replace(/</g, '&lt;').replace(/>/g, '&gt;')}</pre>
                    <div class="mt-3 flex justify-end gap-2">
                      <button class="px-3 py-1.5 text-sm bg-blue-600 text-white rounded hover:bg-blue-700" id="copy-data">Copy</button>
                    </div>
                  </div>
                `;
                document.body.appendChild(modal);
                
                modal.addEventListener('click', (e) => {
                  if (e.target === modal) {
                    modal.remove();
                  }
                });
                modal.querySelector('#close-modal')?.addEventListener('click', () => modal.remove());
                modal.querySelector('#copy-data')?.addEventListener('click', () => {
                  navigator.clipboard.writeText(formattedValue);
                  const btn = modal.querySelector('#copy-data') as HTMLButtonElement;
                  if (btn) {
                    btn.textContent = 'Copied!';
                    setTimeout(() => btn.textContent = 'Copy', 1500);
                  }
                });
                
                setContextMenu(null);
              }}
            >
              <span>👁</span> View Data
            </button>
            <button
              className="w-full px-3 py-2 text-sm text-left hover:bg-muted flex items-center gap-2"
              onClick={() => {
                const value = contextMenu.value;
                const textValue = typeof value === "object" 
                  ? JSON.stringify(value, null, 2) 
                  : String(value);
                navigator.clipboard.writeText(textValue);
                setContextMenu(null);
              }}
            >
              <span>📋</span> Copy Value
            </button>
          </div>
        </>
      )}

      {/* WebSocket Log Modal */}
      {showWsLogModal && activeTab && (
        <div 
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          onClick={(e) => {
            if (e.target === e.currentTarget) setShowWsLogModal(false);
          }}
        >
          <div className="bg-background border rounded-lg shadow-xl w-[600px] max-w-[90vw] max-h-[80vh] flex flex-col m-4">
            <div className="flex justify-between items-center p-4 border-b">
              <h3 className="font-semibold text-lg flex items-center gap-2">
                <span>📜</span> WebSocket Messages Log
                <span className="text-sm font-normal text-muted-foreground">
                  ({activeTab.subscriptionLog.length} entries)
                </span>
              </h3>
              <button 
                className="p-1 hover:bg-muted rounded text-muted-foreground hover:text-foreground"
                onClick={() => setShowWsLogModal(false)}
              >
                ✕
              </button>
            </div>
            <div className="flex-1 overflow-auto p-4">
              {activeTab.subscriptionLog.length === 0 ? (
                <div className="text-center text-muted-foreground py-8">
                  No log entries yet
                </div>
              ) : (
                <div className="space-y-1 font-mono text-xs">
                  {activeTab.subscriptionLog.map((entry, idx) => (
                    <div 
                      key={idx} 
                      className={cn(
                        "p-2 rounded",
                        entry.includes("Error") || entry.includes("error") 
                          ? "bg-red-50 dark:bg-red-950/30 text-red-700 dark:text-red-300" 
                          : entry.includes("INSERT") || entry.includes("insert")
                          ? "bg-green-50 dark:bg-green-950/30 text-green-700 dark:text-green-300"
                          : entry.includes("UPDATE") || entry.includes("update")
                          ? "bg-yellow-50 dark:bg-yellow-950/30 text-yellow-700 dark:text-yellow-300"
                          : entry.includes("DELETE") || entry.includes("delete")
                          ? "bg-red-50 dark:bg-red-950/30 text-red-700 dark:text-red-300"
                          : entry.includes("Subscribed") || entry.includes("connected")
                          ? "bg-blue-50 dark:bg-blue-950/30 text-blue-700 dark:text-blue-300"
                          : "bg-muted/50"
                      )}
                    >
                      {entry}
                    </div>
                  ))}
                </div>
              )}
            </div>
            <div className="flex justify-between items-center p-4 border-t">
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  updateTab(activeTabId, { subscriptionLog: [] });
                }}
                className="text-xs"
              >
                Clear Log
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  navigator.clipboard.writeText(activeTab.subscriptionLog.join('\n'));
                }}
                className="text-xs"
              >
                Copy All
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Schema tree context menu for tables */}
      {schemaContextMenu && (
        <>
          {/* Backdrop to close context menu on click outside */}
          <div 
            className="fixed inset-0 z-40" 
            onClick={() => setSchemaContextMenu(null)}
          />
          <div
            className="fixed z-50 bg-background border border-border rounded-md shadow-lg py-1 min-w-[180px]"
            style={{ 
              left: schemaContextMenu.x, 
              top: schemaContextMenu.y,
              maxHeight: 'calc(100vh - 100px)',
            }}
          >
            <div className="px-3 py-1.5 text-xs text-muted-foreground border-b mb-1">
              {schemaContextMenu.namespace}.{schemaContextMenu.tableName}
            </div>
            <button
              className="w-full px-3 py-2 text-sm text-left hover:bg-muted flex items-center gap-2"
              onClick={() => {
                // Insert SELECT * FROM statement and execute
                const query = `SELECT * FROM ${schemaContextMenu.namespace}.${schemaContextMenu.tableName} LIMIT 100`;
                updateTab(activeTabId, { query });
                setSchemaContextMenu(null);
                // Execute after a brief delay to allow state to update
                setTimeout(() => executeQuery(), 50);
              }}
            >
              <Play className="h-4 w-4" />
              Select * from table
            </button>
            <button
              className="w-full px-3 py-2 text-sm text-left hover:bg-muted flex items-center gap-2"
              onClick={() => {
                // Just insert the query without executing
                const query = `SELECT * FROM ${schemaContextMenu.namespace}.${schemaContextMenu.tableName}`;
                updateTab(activeTabId, { query });
                setSchemaContextMenu(null);
              }}
            >
              <Table2 className="h-4 w-4" />
              Insert SELECT query
            </button>
            <div className="border-t my-1" />
            <button
              className="w-full px-3 py-2 text-sm text-left hover:bg-muted flex items-center gap-2"
              onClick={() => {
                handleShowTableProperties(
                  schemaContextMenu.namespace,
                  schemaContextMenu.tableName,
                  schemaContextMenu.columns
                );
                setSchemaContextMenu(null);
              }}
            >
              <Info className="h-4 w-4" />
              View Properties
            </button>
            <button
              className="w-full px-3 py-2 text-sm text-left hover:bg-muted flex items-center gap-2"
              onClick={() => {
                // Copy qualified table name
                navigator.clipboard.writeText(`${schemaContextMenu.namespace}.${schemaContextMenu.tableName}`);
                setSchemaContextMenu(null);
              }}
            >
              <span className="h-4 w-4 flex items-center justify-center">📋</span>
              Copy table name
            </button>
          </div>
        </>
      )}
    </div>
  );
}
