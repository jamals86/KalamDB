import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { PanelRightClose, PanelRightOpen } from "lucide-react";
import { QueryTabStrip } from "@/components/sql-studio-v2/QueryTabStrip";
import { StudioEditorPanel } from "@/components/sql-studio-v2/StudioEditorPanel";
import { StudioExplorerPanel } from "@/components/sql-studio-v2/StudioExplorerPanel";
import { StudioInspectorPanel } from "@/components/sql-studio-v2/StudioInspectorPanel";
import { StudioResultsGrid } from "@/components/sql-studio-v2/StudioResultsGrid";
import { Button } from "@/components/ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { useAuth } from "@/lib/auth";
import { subscribe, type Unsubscribe } from "@/lib/kalam-client";
import type {
  QueryLogEntry,
  QueryResultData,
  QueryRunSummary,
  SqlStudioResultView,
  QueryTab,
  SavedQuery,
  SqlStudioPanelLayout,
  StudioNamespace,
  StudioTable,
} from "@/components/sql-studio-v2/types";
import {
  loadSqlStudioWorkspaceState,
  saveSqlStudioWorkspaceState,
  type SqlStudioPersistedQueryTab,
} from "@/components/sql-studio-v2/workspaceState";
import {
  executeSqlStudioQuery,
  fetchSqlStudioSchemaTree,
} from "@/services/sqlStudioService";

const DEFAULT_SQL = "SELECT * FROM system.namespaces LIMIT 100;";
const FAVORITE_QUERIES = [
  "Top active users (24h)",
  "Jobs failed in last hour",
  "Namespace growth trend",
];

function createSavedQueryId() {
  return `saved-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
}

function createTabId(index: number) {
  return `tab-${Date.now()}-${index}`;
}

function createLogEntry(
  message: string,
  level: QueryLogEntry["level"] = "info",
  asUser?: string,
): QueryLogEntry {
  const createdAt = new Date().toISOString();
  return {
    id: `${createdAt}-${Math.floor(Math.random() * 1000)}`,
    message,
    level,
    asUser,
    createdAt,
  };
}

function resolveResultView(result: QueryResultData): SqlStudioResultView {
  const hasTableData = result.status === "success" && result.schema.length > 0;
  if (hasTableData) {
    return "results";
  }
  return result.logs.length > 0 ? "log" : "results";
}

function appendLiveRowsToResult(
  previous: QueryResultData | null,
  rows: Record<string, unknown>[],
  changeType: string,
): QueryResultData {
  const receivedAt = new Date().toLocaleTimeString();
  const liveRows = rows.map((row) => ({
    _received_at: receivedAt,
    _change_type: changeType,
    ...row,
  }));

  const existingRows = previous?.rows ?? [];
  const mergedRows = [...existingRows, ...liveRows];
  const orderedKeys = mergedRows.length > 0 ? Object.keys(mergedRows[0]) : [];
  const schema = orderedKeys.map((name, index) => ({
    name,
    dataType: name === "_received_at" ? "timestamp" : "text",
    index,
  }));

  return {
    status: "success",
    rows: mergedRows,
    schema,
    tookMs: previous?.tookMs ?? 0,
    rowCount: mergedRows.length,
    logs: previous?.logs ?? [],
  };
}

function appendLogToResult(
  previous: QueryResultData | null,
  entry: QueryLogEntry,
  statusOverride?: QueryResultData["status"],
): QueryResultData {
  const base: QueryResultData = previous ?? {
    status: "success",
    rows: [],
    schema: [],
    tookMs: 0,
    rowCount: 0,
    logs: [],
  };

  return {
    ...base,
    status: statusOverride ?? base.status,
    logs: [...base.logs, entry],
  };
}

function createQueryTab(index: number): QueryTab {
  return {
    id: createTabId(index),
    title: index === 1 ? "Untitled query" : `Query ${index}`,
    sql: DEFAULT_SQL,
    isDirty: false,
    isLive: false,
    liveStatus: "idle",
    resultView: "results",
    lastSavedAt: null,
    savedQueryId: null,
  };
}

function toPersistedTab(tab: QueryTab): SqlStudioPersistedQueryTab {
  return {
    id: tab.id,
    name: tab.title,
    query: tab.sql,
    settings: {
      isDirty: tab.isDirty,
      isLive: tab.isLive,
      liveStatus: tab.liveStatus === "connected" ? "idle" : tab.liveStatus,
      resultView: tab.resultView,
      lastSavedAt: tab.lastSavedAt,
      savedQueryId: tab.savedQueryId,
    },
  };
}

export default function SqlStudio() {
  const initialWorkspace = useMemo(() => {
    const fallbackTab = createQueryTab(1);
    return loadSqlStudioWorkspaceState(toPersistedTab(fallbackTab));
  }, []);

  const { user } = useAuth();
  const [tabs, setTabs] = useState<QueryTab[]>(
    initialWorkspace.tabs.map((tab) => ({
      id: tab.id,
      title: tab.name,
      sql: tab.query,
      isDirty: tab.settings.isDirty,
      isLive: tab.settings.isLive,
      liveStatus: tab.settings.liveStatus,
      resultView: tab.settings.resultView,
      lastSavedAt: tab.settings.lastSavedAt,
      savedQueryId: tab.settings.savedQueryId,
    })),
  );
  const [savedQueries, setSavedQueries] = useState<SavedQuery[]>(
    initialWorkspace.savedQueries.map((item) => ({
      id: item.id,
      title: item.title,
      sql: item.sql,
      lastSavedAt: item.lastSavedAt,
      isLive: item.isLive,
    })),
  );
  const [activeTabId, setActiveTabId] = useState<string>(initialWorkspace.activeTabId);
  const [schema, setSchema] = useState<StudioNamespace[]>([]);
  const [schemaFilter, setSchemaFilter] = useState(initialWorkspace.explorerTree.filter);
  const [favoritesExpanded, setFavoritesExpanded] = useState(
    initialWorkspace.explorerTree.favoritesExpanded,
  );
  const [expandedNamespaces, setExpandedNamespaces] = useState<Record<string, boolean>>(
    initialWorkspace.explorerTree.expandedNamespaces,
  );
  const [expandedTables, setExpandedTables] = useState<Record<string, boolean>>(
    initialWorkspace.explorerTree.expandedTables,
  );
  const [selectedTable, setSelectedTable] = useState<StudioTable | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [tabResults, setTabResults] = useState<Record<string, QueryResultData | null>>({});
  const [history, setHistory] = useState<QueryRunSummary[]>([]);
  const [isInspectorCollapsed, setIsInspectorCollapsed] = useState(initialWorkspace.inspectorCollapsed);
  const [horizontalLayout, setHorizontalLayout] = useState<SqlStudioPanelLayout>(
    initialWorkspace.sizes.explorerMain,
  );
  const [verticalLayout, setVerticalLayout] = useState<SqlStudioPanelLayout>(
    initialWorkspace.sizes.editorResults,
  );
  const [explorerContextMenu, setExplorerContextMenu] = useState<{
    x: number;
    y: number;
    table: StudioTable;
  } | null>(null);
  const liveUnsubscribeRef = useRef<Record<string, Unsubscribe>>({});

  const activeTab = useMemo(
    () => tabs.find((item) => item.id === activeTabId) ?? tabs[0]!,
    [tabs, activeTabId],
  );
  const activeResult = activeTab ? (tabResults[activeTab.id] ?? null) : null;

  const selectedTableKey = selectedTable
    ? `${selectedTable.namespace}.${selectedTable.name}`
    : null;

  const loadSchema = async () => {
    try {
      const tree = await fetchSqlStudioSchemaTree();
      setSchema(tree);
      const persistedTableKey = initialWorkspace.selectedTableKey;
      if (persistedTableKey) {
        const [namespaceName, tableName] = persistedTableKey.split(".");
        const namespace = tree.find((item) => item.name === namespaceName);
        const table = namespace?.tables.find((item) => item.name === tableName);
        if (table) {
          setSelectedTable(table);
          return;
        }
      }

      if (tree.length > 0 && tree[0].tables.length > 0) {
        setSelectedTable(tree[0].tables[0]);
      }
    } catch (error) {
      console.error("Failed to load SQL Studio schema tree", error);
    }
  };

  useEffect(() => {
    loadSchema().catch(console.error);
  }, []);

  useEffect(() => {
    if (schema.length === 0 || Object.keys(expandedNamespaces).length > 0) {
      return;
    }

    const initialExpanded: Record<string, boolean> = {};
    schema.slice(0, 3).forEach((namespace) => {
      initialExpanded[namespace.name] = true;
    });
    setExpandedNamespaces(initialExpanded);
  }, [schema, expandedNamespaces]);

  useEffect(() => {
    if (!selectedTableKey) {
      return;
    }

    const [namespaceName] = selectedTableKey.split(".");
    setExpandedNamespaces((prev) => ({ ...prev, [namespaceName]: true }));
    setExpandedTables((prev) => ({ ...prev, [selectedTableKey]: true }));
  }, [selectedTableKey]);

  const updateTab = useCallback((tabId: string, updates: Partial<QueryTab>) => {
    setTabs((previous) =>
      previous.map((tab) => (tab.id === tabId ? { ...tab, ...updates } : tab)),
    );
  }, []);

  const updateActiveTab = useCallback((updates: Partial<QueryTab>) => {
    if (!activeTab) {
      return;
    }
    updateTab(activeTab.id, updates);
  }, [activeTab, updateTab]);

  const addTab = () => {
    const nextIndex = tabs.length + 1;
    const tab = createQueryTab(nextIndex);
    setTabs((previous) => [...previous, tab]);
    setActiveTabId(tab.id);
  };

  const closeTab = useCallback((tabId: string) => {
    if (tabs.length === 1) {
      return;
    }

    const unsubscribe = liveUnsubscribeRef.current[tabId];
    if (unsubscribe) {
      unsubscribe();
      delete liveUnsubscribeRef.current[tabId];
    }

    const index = tabs.findIndex((tab) => tab.id === tabId);
    const nextTabs = tabs.filter((tab) => tab.id !== tabId);

    setTabs(nextTabs);
    setTabResults((previous) => {
      const next = { ...previous };
      delete next[tabId];
      return next;
    });

    if (activeTabId === tabId) {
      const fallback = nextTabs[Math.max(0, index - 1)] ?? nextTabs[0];
      setActiveTabId(fallback.id);
    }
  }, [tabs, activeTabId]);

  const openQueryInNewTab = (query: string, title: string) => {
    const tab: QueryTab = {
      id: createTabId(tabs.length + 1),
      title,
      sql: query,
      isDirty: true,
      isLive: false,
      liveStatus: "idle",
      resultView: "results",
      lastSavedAt: null,
      savedQueryId: null,
    };
    setTabs((previous) => [...previous, tab]);
    setActiveTabId(tab.id);
  };

  const executeQueryForTab = async (tabId: string, sql: string, tabTitle: string) => {
    if (!sql.trim()) {
      return;
    }

    setIsRunning(true);
    const startedAt = Date.now();

    try {
      const queryResult = await executeSqlStudioQuery(sql);
      const durationMs = Math.round(Date.now() - startedAt);
      const historyEntry: QueryRunSummary = {
        id: `${tabId}-${startedAt}`,
        tabTitle,
        sql,
        status: queryResult.status,
        executedAt: new Date().toISOString(),
        durationMs,
        rowCount: queryResult.rowCount,
        errorMessage: queryResult.errorMessage,
      };

      startTransition(() => {
        setTabResults((previous) => ({ ...previous, [tabId]: queryResult }));
        setHistory((previous) => [historyEntry, ...previous].slice(0, 50));
        updateTab(tabId, {
          isDirty: false,
          resultView: resolveResultView(queryResult),
        });
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : "Query execution failed";
      setTabResults((previous) => ({ ...previous, [tabId]: {
        status: "error",
        rows: [],
        schema: [],
        tookMs: 0,
        rowCount: 0,
        logs: [createLogEntry(message, "error", user?.username)],
        errorMessage: message,
      } }));
      updateTab(tabId, { resultView: "log" });
    } finally {
      setIsRunning(false);
    }
  };

  const stopLiveQuery = useCallback((tabId: string) => {
    const unsubscribe = liveUnsubscribeRef.current[tabId];
    if (unsubscribe) {
      unsubscribe();
      delete liveUnsubscribeRef.current[tabId];
    }
    updateTab(tabId, { liveStatus: "idle" });
  }, [updateTab]);

  const startLiveQuery = useCallback(async (tab: QueryTab) => {
    if (!tab.sql.trim()) {
      return;
    }

    updateTab(tab.id, { liveStatus: "connecting", isLive: true });
    try {
      const unsubscribe = await subscribe(tab.sql, (event) => {
        const serverMessage = event as {
          type?: string;
          rows?: Record<string, unknown>[];
          change_type?: string;
          message?: string;
          code?: string;
        };

        if (serverMessage.type === "error") {
          setTabResults((previous) => ({
            ...previous,
            [tab.id]: appendLogToResult(
              previous[tab.id] ?? null,
              createLogEntry(serverMessage.message ?? "Live query error", "error", user?.username),
              "error",
            ),
          }));
          updateTab(tab.id, { liveStatus: "error" });
          return;
        }

        if (serverMessage.type === "subscription_ack") {
          setTabResults((previous) => ({
            ...previous,
            [tab.id]: appendLogToResult(
              previous[tab.id] ?? null,
              createLogEntry("Live subscription connected.", "info", user?.username),
            ),
          }));
          updateTab(tab.id, { liveStatus: "connected", resultView: "results" });
          return;
        }

        if (Array.isArray(serverMessage.rows)) {
          const receivedRows = serverMessage.rows;
          const changeType = serverMessage.type === "change"
            ? (serverMessage.change_type ?? "change")
            : "initial";

          setTabResults((previous) => ({
            ...previous,
            [tab.id]: appendLogToResult(
              appendLiveRowsToResult(previous[tab.id] ?? null, receivedRows, changeType),
              createLogEntry(
                `Received ${receivedRows.length} row${receivedRows.length === 1 ? "" : "s"} (${changeType}).`,
                "info",
                user?.username,
              ),
            ),
          }));
          updateTab(tab.id, { liveStatus: "connected", resultView: "results" });
          return;
        }

        if (Array.isArray(event)) {
          setTabResults((previous) => ({
            ...previous,
            [tab.id]: appendLogToResult(
              appendLiveRowsToResult(previous[tab.id] ?? null, event as unknown as Record<string, unknown>[], "data"),
              createLogEntry(
                `Received ${event.length} row${event.length === 1 ? "" : "s"} (data).`,
                "info",
                user?.username,
              ),
            ),
          }));
          updateTab(tab.id, { liveStatus: "connected", resultView: "results" });
        }
      });

      liveUnsubscribeRef.current[tab.id] = unsubscribe;
    } catch (error) {
      console.error("Failed to subscribe to live query", error);
      setTabResults((previous) => ({
        ...previous,
        [tab.id]: appendLogToResult(
          previous[tab.id] ?? null,
          createLogEntry(
            error instanceof Error ? error.message : "Failed to subscribe to live query",
            "error",
            user?.username,
          ),
          "error",
        ),
      }));
      updateTab(tab.id, { liveStatus: "error" });
    }
  }, [updateTab, user?.username]);

  const runActiveQuery = async () => {
    if (!activeTab) {
      return;
    }

    if (activeTab.isLive) {
      if (activeTab.liveStatus === "connected") {
        stopLiveQuery(activeTab.id);
      } else {
        await startLiveQuery(activeTab);
      }
      return;
    }

    await executeQueryForTab(activeTab.id, activeTab.sql, activeTab.title);
  };

  const saveTab = useCallback((tabId: string, openAsCopy: boolean) => {
    const tab = tabs.find((item) => item.id === tabId);
    if (!tab) {
      return;
    }

    const nowIso = new Date().toISOString();
    const saveId = openAsCopy || !tab.savedQueryId ? createSavedQueryId() : tab.savedQueryId;
    const saveTitle = openAsCopy ? `${tab.title} Copy` : tab.title;

    setSavedQueries((previous) => {
      const existing = previous.find((item) => item.id === saveId);
      if (existing) {
        return previous.map((item) =>
          item.id === saveId
            ? { ...item, title: saveTitle, sql: tab.sql, isLive: tab.isLive, lastSavedAt: nowIso }
            : item,
        );
      }
      return [
        {
          id: saveId,
          title: saveTitle,
          sql: tab.sql,
          isLive: tab.isLive,
          lastSavedAt: nowIso,
        },
        ...previous,
      ];
    });

    if (openAsCopy) {
      const copiedTab: QueryTab = {
        ...tab,
        id: createTabId(tabs.length + 1),
        title: saveTitle,
        isDirty: false,
        savedQueryId: saveId,
        lastSavedAt: nowIso,
        liveStatus: "idle",
        resultView: tab.resultView,
      };
      setTabs((previous) => [...previous, copiedTab]);
      setActiveTabId(copiedTab.id);
      if (tabResults[tab.id]) {
        setTabResults((previous) => ({ ...previous, [copiedTab.id]: previous[tab.id] ?? null }));
      }
      return;
    }

    updateTab(tab.id, {
      savedQueryId: saveId,
      lastSavedAt: nowIso,
      isDirty: false,
    });
  }, [tabs, tabResults, updateTab]);

  const renameActiveTab = useCallback((title: string) => {
    if (!activeTab) {
      return;
    }

    updateTab(activeTab.id, { title });
    if (activeTab.savedQueryId) {
      setSavedQueries((previous) =>
        previous.map((item) =>
          item.id === activeTab.savedQueryId ? { ...item, title } : item,
        ),
      );
    }
  }, [activeTab, updateTab]);

  const deleteActiveTab = useCallback(() => {
    if (!activeTab) {
      return;
    }

    const deletedSavedQueryId = activeTab.savedQueryId;
    closeTab(activeTab.id);
    if (deletedSavedQueryId) {
      setSavedQueries((previous) =>
        previous.filter((item) => item.id !== deletedSavedQueryId),
      );
      setTabs((previous) =>
        previous.map((item) =>
          item.savedQueryId === deletedSavedQueryId
            ? { ...item, savedQueryId: null }
            : item,
        ),
      );
    }
  }, [activeTab, closeTab]);

  const openSavedQuery = useCallback((queryId: string) => {
    const savedQuery = savedQueries.find((item) => item.id === queryId);
    if (!savedQuery) {
      return;
    }

    const existingTab = tabs.find((item) => item.savedQueryId === queryId);
    if (existingTab) {
      setActiveTabId(existingTab.id);
      return;
    }

    const tab: QueryTab = {
      id: createTabId(tabs.length + 1),
      title: savedQuery.title,
      sql: savedQuery.sql,
      isDirty: false,
      isLive: savedQuery.isLive,
      liveStatus: "idle",
      resultView: "results",
      lastSavedAt: savedQuery.lastSavedAt,
      savedQueryId: savedQuery.id,
    };
    setTabs((previous) => [...previous, tab]);
    setActiveTabId(tab.id);
  }, [savedQueries, tabs]);

  const toggleInspector = () => {
    if (isInspectorCollapsed) {
      setIsInspectorCollapsed(false);
      return;
    }
    setIsInspectorCollapsed(true);
  };

  useEffect(() => {
    if (!explorerContextMenu) {
      return;
    }

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setExplorerContextMenu(null);
      }
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [explorerContextMenu]);

  useEffect(() => {
    return () => {
      Object.values(liveUnsubscribeRef.current).forEach((unsubscribe) => unsubscribe());
      liveUnsubscribeRef.current = {};
    };
  }, []);

  useEffect(() => {
    if (tabs.length === 0) {
      return;
    }

    saveSqlStudioWorkspaceState({
      version: 1,
      tabs: tabs.map(toPersistedTab),
      savedQueries: savedQueries.map((item) => ({
        id: item.id,
        title: item.title,
        sql: item.sql,
        lastSavedAt: item.lastSavedAt,
        isLive: item.isLive,
      })),
      activeTabId: tabs.some((tab) => tab.id === activeTabId) ? activeTabId : tabs[0].id,
      selectedTableKey,
      inspectorCollapsed: isInspectorCollapsed,
      sizes: {
        explorerMain: horizontalLayout,
        editorResults: verticalLayout,
      },
      explorerTree: {
        favoritesExpanded,
        expandedNamespaces,
        expandedTables,
        filter: schemaFilter,
      },
    });
  }, [
    tabs,
    activeTabId,
    selectedTableKey,
    isInspectorCollapsed,
    horizontalLayout,
    verticalLayout,
    favoritesExpanded,
    savedQueries,
    expandedNamespaces,
    expandedTables,
    schemaFilter,
  ]);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-[#101922] text-slate-200">
      <div className="flex min-h-0 flex-1 overflow-hidden">
        <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
          <ResizablePanel
            defaultSize={horizontalLayout[0]}
            minSize="12%"
            maxSize="36%"
            className="min-h-0"
            onResize={(size) => {
              const numericSize = Number(size);
              if (!Number.isFinite(numericSize)) {
                return;
              }
              const safeSize = Math.max(12, Math.min(36, numericSize));
              setHorizontalLayout([safeSize, 100 - safeSize]);
            }}
          >
            <StudioExplorerPanel
              schema={schema}
              filter={schemaFilter}
              favoriteQueries={FAVORITE_QUERIES}
              savedQueries={savedQueries}
              favoritesExpanded={favoritesExpanded}
              expandedNamespaces={expandedNamespaces}
              expandedTables={expandedTables}
              selectedTableKey={selectedTableKey}
              onFilterChange={setSchemaFilter}
              onToggleFavorites={() => setFavoritesExpanded((prev) => !prev)}
              onToggleNamespace={(namespaceName) =>
                setExpandedNamespaces((prev) => ({
                  ...prev,
                  [namespaceName]: !prev[namespaceName],
                }))
              }
              onToggleTable={(tableKey) =>
                setExpandedTables((prev) => ({
                  ...prev,
                  [tableKey]: !prev[tableKey],
                }))
              }
              onOpenSavedQuery={openSavedQuery}
              onSelectTable={setSelectedTable}
              onTableContextMenu={(table, position) => {
                setExplorerContextMenu({
                  x: position.x,
                  y: position.y,
                  table,
                });
              }}
            />
          </ResizablePanel>

          <ResizableHandle withHandle />

          <ResizablePanel defaultSize={horizontalLayout[1]} minSize="40%" className="min-h-0">
            <div className="relative flex h-full min-h-0 flex-col overflow-hidden bg-white dark:bg-[#101922]">
              <div className="absolute right-2 top-1.5 z-20">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 text-slate-500 hover:text-slate-200"
                  onClick={toggleInspector}
                  title={isInspectorCollapsed ? "Expand details panel" : "Collapse details panel"}
                >
                  {isInspectorCollapsed ? (
                    <PanelRightOpen className="h-4 w-4" />
                  ) : (
                    <PanelRightClose className="h-4 w-4" />
                  )}
                </Button>
              </div>

              <QueryTabStrip
                tabs={tabs}
                activeTabId={activeTab.id}
                onTabSelect={setActiveTabId}
                onAddTab={addTab}
                onCloseTab={closeTab}
              />

              <ResizablePanelGroup orientation="vertical" className="min-h-0 flex-1">
                <ResizablePanel
                  defaultSize={verticalLayout[0]}
                  minSize="20%"
                  className="min-h-0"
                  onResize={(size) => {
                    const numericSize = Number(size);
                    if (!Number.isFinite(numericSize)) {
                      return;
                    }
                    const safeSize = Math.max(20, Math.min(80, numericSize));
                    setVerticalLayout([safeSize, 100 - safeSize]);
                  }}
                >
                  <StudioEditorPanel
                    tabTitle={activeTab.title}
                    lastSavedAt={activeTab.lastSavedAt}
                    isLive={activeTab.isLive}
                    liveStatus={activeTab.liveStatus}
                    sql={activeTab.sql}
                    isRunning={isRunning}
                    onSqlChange={(value) => updateActiveTab({ sql: value, isDirty: true })}
                    onRun={runActiveQuery}
                    onToggleLive={(checked) => {
                      updateActiveTab({ isLive: checked, liveStatus: "idle" });
                      if (!checked && activeTab.liveStatus === "connected") {
                        stopLiveQuery(activeTab.id);
                      }
                    }}
                    onRename={renameActiveTab}
                    onSave={() => saveTab(activeTab.id, false)}
                    onSaveCopy={() => saveTab(activeTab.id, true)}
                    onDelete={deleteActiveTab}
                  />
                </ResizablePanel>

                <ResizableHandle withHandle />

                <ResizablePanel defaultSize={verticalLayout[1]} minSize="20%" className="min-h-0">
                  <StudioResultsGrid
                    result={activeResult}
                    isRunning={isRunning}
                    activeSql={activeTab.sql}
                    selectedTable={selectedTable}
                    currentUsername={user?.username ?? "admin"}
                    resultView={activeTab.resultView}
                    onResultViewChange={(view) => updateActiveTab({ resultView: view })}
                    onRefreshAfterCommit={() => executeQueryForTab(activeTab.id, activeTab.sql, activeTab.title)}
                  />
                </ResizablePanel>
              </ResizablePanelGroup>
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>

        {!isInspectorCollapsed && (
          <div className="min-h-0 w-[320px] min-w-[240px] max-w-[420px] border-l border-[#1b2a40]">
            <StudioInspectorPanel selectedTable={selectedTable} history={history} />
          </div>
        )}
      </div>

      {explorerContextMenu && (
        <>
          <div
            className="fixed inset-0 z-40"
            onClick={() => setExplorerContextMenu(null)}
          />
          <div
            className="fixed z-50 min-w-[210px] rounded-md border border-[#1f334d] bg-[#0f1a2a] py-1 shadow-xl"
            style={{ left: explorerContextMenu.x, top: explorerContextMenu.y }}
          >
            <div className="border-b border-[#1f334d] px-3 py-1.5 text-[11px] text-slate-400">
              {explorerContextMenu.table.namespace}.{explorerContextMenu.table.name}
            </div>
            <button
              type="button"
              className="w-full px-3 py-2 text-left text-sm text-slate-200 hover:bg-[#133253]"
              onClick={() => {
                const sql = `SELECT * FROM ${explorerContextMenu.table.namespace}.${explorerContextMenu.table.name} LIMIT 100;`;
                openQueryInNewTab(sql, explorerContextMenu.table.name);
                setSelectedTable(explorerContextMenu.table);
                setExplorerContextMenu(null);
              }}
            >
              Open Query In New Tab
            </button>
            <button
              type="button"
              className="w-full px-3 py-2 text-left text-sm text-slate-200 hover:bg-[#133253]"
              onClick={() => {
                const sql = `SELECT * FROM ${explorerContextMenu.table.namespace}.${explorerContextMenu.table.name} LIMIT 100;`;
                updateActiveTab({ sql, isDirty: true });
                setSelectedTable(explorerContextMenu.table);
                setExplorerContextMenu(null);
                executeQueryForTab(activeTab.id, sql, activeTab.title).catch(console.error);
              }}
            >
              Select * From Table
            </button>
            <button
              type="button"
              className="w-full px-3 py-2 text-left text-sm text-slate-200 hover:bg-[#133253]"
              onClick={() => {
                const sql = `SELECT * FROM ${explorerContextMenu.table.namespace}.${explorerContextMenu.table.name};`;
                updateActiveTab({ sql, isDirty: true });
                setSelectedTable(explorerContextMenu.table);
                setExplorerContextMenu(null);
              }}
            >
              Insert SELECT Query
            </button>
            <div className="my-1 border-t border-[#1f334d]" />
            <button
              type="button"
              className="w-full px-3 py-2 text-left text-sm text-slate-200 hover:bg-[#133253]"
              onClick={() => {
                setSelectedTable(explorerContextMenu.table);
                setIsInspectorCollapsed(false);
                setExplorerContextMenu(null);
              }}
            >
              View Properties
            </button>
            <button
              type="button"
              className="w-full px-3 py-2 text-left text-sm text-slate-200 hover:bg-[#133253]"
              onClick={() => {
                const qualifiedName = `${explorerContextMenu.table.namespace}.${explorerContextMenu.table.name}`;
                navigator.clipboard.writeText(qualifiedName).catch(console.error);
                setExplorerContextMenu(null);
              }}
            >
              Copy Qualified Table Name
            </button>
          </div>
        </>
      )}
    </div>
  );
}
