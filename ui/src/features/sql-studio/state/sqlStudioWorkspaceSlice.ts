import { createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type {
  QueryLogEntry,
  QueryResultData,
  QueryResultSchemaField,
  QueryRunSummary,
  QueryTab,
  SavedQuery,
} from "@/components/sql-studio-v2/types";

interface SqlStudioWorkspaceState {
  tabs: QueryTab[];
  savedQueries: SavedQuery[];
  activeTabId: string | null;
  isRunning: boolean;
  tabResults: Record<string, QueryResultData | null>;
  history: QueryRunSummary[];
}

const initialState: SqlStudioWorkspaceState = {
  tabs: [],
  savedQueries: [],
  activeTabId: null,
  isRunning: false,
  tabResults: {},
  history: [],
};

function appendLiveRowsToResult(
  previous: QueryResultData | null,
  rows: Record<string, unknown>[],
  changeType: string,
  incomingSchema?: QueryResultSchemaField[],
): QueryResultData {
  const receivedAt = new Date().toLocaleTimeString();
  const liveRows = rows.map((row) => ({
    _received_at: receivedAt,
    _change_type: changeType,
    ...row,
  }));
  const existingRows = previous?.rows ?? [];
  const mergedRows = [...existingRows, ...liveRows];
  const fallbackSchema = (() => {
    const orderedKeys = mergedRows.length > 0 ? Object.keys(mergedRows[0]) : [];
    return orderedKeys.map((name, index) => ({
      name,
      dataType: name === "_received_at" ? "timestamp" : "text",
      index,
      isPrimaryKey: false,
    }));
  })();
  const orderedIncomingSchema = incomingSchema
    ? [...incomingSchema].sort((left, right) => left.index - right.index)
    : undefined;
  const sourceSchema =
    orderedIncomingSchema && orderedIncomingSchema.length > 0
      ? orderedIncomingSchema
      : (previous?.schema.length ?? 0) > 0
        ? previous?.schema
        : fallbackSchema;
  const hasReceivedAt = sourceSchema?.some((field) => field.name === "_received_at");
  const hasChangeType = sourceSchema?.some((field) => field.name === "_change_type");
  const baseSchema = sourceSchema ?? [];
  const schemaWithMeta: QueryResultSchemaField[] = [
    ...(hasReceivedAt ? [] : [{
      name: "_received_at",
      dataType: "timestamp",
      index: -1,
      isPrimaryKey: false,
    }]),
    ...(hasChangeType ? [] : [{
      name: "_change_type",
      dataType: "text",
      index: -1,
      isPrimaryKey: false,
    }]),
    ...baseSchema,
  ];
  const schema = schemaWithMeta.map((field, index) => ({
    ...field,
    index,
    isPrimaryKey: field.isPrimaryKey ?? false,
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

function applyLiveSchemaToResult(
  previous: QueryResultData | null,
  schema: QueryResultSchemaField[],
): QueryResultData {
  const base: QueryResultData = previous ?? {
    status: "success",
    rows: [],
    schema: [],
    tookMs: 0,
    rowCount: 0,
    logs: [],
  };

  if (schema.length === 0) {
    return base;
  }

  const normalizedSchema = [...schema]
    .sort((left, right) => left.index - right.index)
    .map((field, index) => ({
      ...field,
      index,
      isPrimaryKey: field.isPrimaryKey ?? false,
    }));

  return {
    ...base,
    status: base.status === "error" ? "success" : base.status,
    schema: normalizedSchema,
  };
}

const sqlStudioWorkspaceSlice = createSlice({
  name: "sqlStudioWorkspace",
  initialState,
  reducers: {
    hydrateSqlStudioWorkspace(state, action: PayloadAction<{
      tabs: QueryTab[];
      savedQueries: SavedQuery[];
      activeTabId: string;
    }>) {
      state.tabs = action.payload.tabs;
      state.savedQueries = action.payload.savedQueries;
      state.activeTabId = action.payload.activeTabId;
    },
    setWorkspaceTabs(state, action: PayloadAction<QueryTab[]>) {
      state.tabs = action.payload;
    },
    addWorkspaceTab(state, action: PayloadAction<QueryTab>) {
      state.tabs.push(action.payload);
    },
    updateWorkspaceTab(
      state,
      action: PayloadAction<{ tabId: string; updates: Partial<QueryTab> }>,
    ) {
      const tab = state.tabs.find((item) => item.id === action.payload.tabId);
      if (!tab) {
        return;
      }
      Object.assign(tab, action.payload.updates);
    },
    closeWorkspaceTab(state, action: PayloadAction<string>) {
      if (state.tabs.length <= 1) {
        return;
      }

      const tabId = action.payload;
      const index = state.tabs.findIndex((item) => item.id === tabId);
      if (index < 0) {
        return;
      }

      const nextTabs = state.tabs.filter((item) => item.id !== tabId);
      state.tabs = nextTabs;
      delete state.tabResults[tabId];

      if (state.activeTabId === tabId) {
        const fallback = nextTabs[Math.max(0, index - 1)] ?? nextTabs[0] ?? null;
        state.activeTabId = fallback?.id ?? null;
      }
    },
    setWorkspaceActiveTabId(state, action: PayloadAction<string>) {
      state.activeTabId = action.payload;
    },
    setWorkspaceSavedQueries(state, action: PayloadAction<SavedQuery[]>) {
      state.savedQueries = action.payload;
    },
    setWorkspaceRunning(state, action: PayloadAction<boolean>) {
      state.isRunning = action.payload;
    },
    setWorkspaceTabResult(
      state,
      action: PayloadAction<{ tabId: string; result: QueryResultData | null }>,
    ) {
      state.tabResults[action.payload.tabId] = action.payload.result;
    },
    appendWorkspaceLiveRows(
      state,
      action: PayloadAction<{
        tabId: string;
        rows: Record<string, unknown>[];
        changeType: string;
        schema?: QueryResultSchemaField[];
      }>,
    ) {
      const {
        tabId,
        rows,
        changeType,
        schema,
      } = action.payload;
      state.tabResults[tabId] = appendLiveRowsToResult(
        state.tabResults[tabId] ?? null,
        rows,
        changeType,
        schema,
      );
    },
    appendWorkspaceResultLog(
      state,
      action: PayloadAction<{
        tabId: string;
        entry: QueryLogEntry;
        statusOverride?: QueryResultData["status"];
      }>,
    ) {
      const { tabId, entry, statusOverride } = action.payload;
      state.tabResults[tabId] = appendLogToResult(state.tabResults[tabId] ?? null, entry, statusOverride);
    },
    setWorkspaceLiveSchema(
      state,
      action: PayloadAction<{ tabId: string; schema: QueryResultSchemaField[] }>,
    ) {
      const { tabId, schema } = action.payload;
      state.tabResults[tabId] = applyLiveSchemaToResult(state.tabResults[tabId] ?? null, schema);
    },
    prependWorkspaceHistory(state, action: PayloadAction<QueryRunSummary>) {
      state.history = [action.payload, ...state.history].slice(0, 50);
    },
  },
});

export const {
  hydrateSqlStudioWorkspace,
  setWorkspaceTabs,
  addWorkspaceTab,
  updateWorkspaceTab,
  closeWorkspaceTab,
  setWorkspaceActiveTabId,
  setWorkspaceSavedQueries,
  setWorkspaceRunning,
  setWorkspaceTabResult,
  appendWorkspaceLiveRows,
  appendWorkspaceResultLog,
  setWorkspaceLiveSchema,
  prependWorkspaceHistory,
} = sqlStudioWorkspaceSlice.actions;

export default sqlStudioWorkspaceSlice.reducer;
