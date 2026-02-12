export const SQL_STUDIO_WORKSPACE_STORAGE_KEY = "kalamdb-sql-studio-v2-workspace";

type PanelLayout = [number, number];

export interface SqlStudioPersistedQueryTab {
  id: string;
  name: string;
  query: string;
  settings: {
    isDirty: boolean;
    isLive: boolean;
    liveStatus: "idle" | "connecting" | "connected" | "error";
    resultView: "results" | "log";
    lastSavedAt: string | null;
    savedQueryId: string | null;
  };
}

export interface SqlStudioPersistedSavedQuery {
  id: string;
  title: string;
  sql: string;
  lastSavedAt: string;
  isLive: boolean;
}

export interface SqlStudioExplorerTreeState {
  favoritesExpanded: boolean;
  expandedNamespaces: Record<string, boolean>;
  expandedTables: Record<string, boolean>;
  filter: string;
}

export interface SqlStudioWorkspaceSizes {
  explorerMain: PanelLayout;
  editorResults: PanelLayout;
}

export interface SqlStudioWorkspaceState {
  version: 1;
  tabs: SqlStudioPersistedQueryTab[];
  savedQueries: SqlStudioPersistedSavedQuery[];
  activeTabId: string;
  selectedTableKey: string | null;
  inspectorCollapsed: boolean;
  sizes: SqlStudioWorkspaceSizes;
  explorerTree: SqlStudioExplorerTreeState;
}

const DEFAULT_WORKSPACE_SIZES: SqlStudioWorkspaceSizes = {
  explorerMain: [21, 79],
  editorResults: [42, 58],
};

function sanitizePanelLayout(value: unknown, fallback: PanelLayout): PanelLayout {
  if (!Array.isArray(value) || value.length !== 2) {
    return fallback;
  }

  const left = Number(value[0]);
  const right = Number(value[1]);
  if (!Number.isFinite(left) || !Number.isFinite(right) || left <= 0 || right <= 0) {
    return fallback;
  }

  return [left, right];
}

function sanitizeRecord(value: unknown): Record<string, boolean> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  return Object.entries(value).reduce<Record<string, boolean>>((acc, [key, entry]) => {
    acc[key] = Boolean(entry);
    return acc;
  }, {});
}

function normalizeTabs(
  value: unknown,
  fallbackTab: SqlStudioPersistedQueryTab,
): SqlStudioPersistedQueryTab[] {
  if (!Array.isArray(value) || value.length === 0) {
    return [fallbackTab];
  }

  const normalized: SqlStudioPersistedQueryTab[] = [];
  value.forEach((item) => {
    if (!item || typeof item !== "object") {
      return;
    }

    const record = item as Partial<SqlStudioPersistedQueryTab>;
    const id = typeof record.id === "string" ? record.id : "";
    const name = typeof record.name === "string" ? record.name : "";
    const query = typeof record.query === "string" ? record.query : "";
    const isDirty = Boolean(record.settings?.isDirty);
    const isLive = Boolean(record.settings?.isLive);
    const liveStatus: SqlStudioPersistedQueryTab["settings"]["liveStatus"] =
      record.settings?.liveStatus === "connected" || record.settings?.liveStatus === "error"
        ? record.settings.liveStatus
        : "idle";
    const resultView: SqlStudioPersistedQueryTab["settings"]["resultView"] =
      record.settings?.resultView === "log" ? "log" : "results";
    const lastSavedAt = typeof record.settings?.lastSavedAt === "string"
      ? record.settings.lastSavedAt
      : null;
    const savedQueryId = typeof record.settings?.savedQueryId === "string"
      ? record.settings.savedQueryId
      : null;

    if (!id || !name) {
      return;
    }

    normalized.push({
      id,
      name,
      query,
      settings: {
        isDirty,
        isLive,
        liveStatus,
        resultView,
        lastSavedAt,
        savedQueryId,
      },
    });
  });

  return normalized.length > 0 ? normalized : [fallbackTab];
}

function normalizeSavedQueries(value: unknown): SqlStudioPersistedSavedQuery[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item) => {
      if (!item || typeof item !== "object") {
        return null;
      }
      const record = item as Partial<SqlStudioPersistedSavedQuery>;
      const id = typeof record.id === "string" ? record.id : "";
      const title = typeof record.title === "string" ? record.title : "";
      const sql = typeof record.sql === "string" ? record.sql : "";
      const lastSavedAt = typeof record.lastSavedAt === "string" ? record.lastSavedAt : "";
      const isLive = Boolean(record.isLive);
      if (!id || !title || !lastSavedAt) {
        return null;
      }

      return {
        id,
        title,
        sql,
        lastSavedAt,
        isLive,
      } satisfies SqlStudioPersistedSavedQuery;
    })
    .filter((item): item is SqlStudioPersistedSavedQuery => item !== null);
}

export function loadSqlStudioWorkspaceState(
  fallbackTab: SqlStudioPersistedQueryTab,
): SqlStudioWorkspaceState {
  if (typeof window === "undefined") {
    return {
      version: 1,
      tabs: [fallbackTab],
      savedQueries: [],
      activeTabId: fallbackTab.id,
      selectedTableKey: null,
      inspectorCollapsed: false,
      sizes: DEFAULT_WORKSPACE_SIZES,
      explorerTree: {
        favoritesExpanded: true,
        expandedNamespaces: {},
        expandedTables: {},
        filter: "",
      },
    };
  }

  try {
    const raw = window.localStorage.getItem(SQL_STUDIO_WORKSPACE_STORAGE_KEY);
    if (!raw) {
      throw new Error("missing persisted state");
    }

    const parsed = JSON.parse(raw) as Partial<SqlStudioWorkspaceState>;
    const tabs = normalizeTabs(parsed.tabs, fallbackTab);
    const savedQueries = normalizeSavedQueries(parsed.savedQueries);
    const activeTabId = tabs.some((tab) => tab.id === parsed.activeTabId)
      ? String(parsed.activeTabId)
      : tabs[0].id;

    return {
      version: 1,
      tabs,
      savedQueries,
      activeTabId,
      selectedTableKey:
        typeof parsed.selectedTableKey === "string" ? parsed.selectedTableKey : null,
      inspectorCollapsed: Boolean(parsed.inspectorCollapsed),
      sizes: {
        explorerMain: sanitizePanelLayout(parsed.sizes?.explorerMain, DEFAULT_WORKSPACE_SIZES.explorerMain),
        editorResults: sanitizePanelLayout(parsed.sizes?.editorResults, DEFAULT_WORKSPACE_SIZES.editorResults),
      },
      explorerTree: {
        favoritesExpanded: parsed.explorerTree?.favoritesExpanded !== false,
        expandedNamespaces: sanitizeRecord(parsed.explorerTree?.expandedNamespaces),
        expandedTables: sanitizeRecord(parsed.explorerTree?.expandedTables),
        filter: typeof parsed.explorerTree?.filter === "string" ? parsed.explorerTree.filter : "",
      },
    };
  } catch {
    return {
      version: 1,
      tabs: [fallbackTab],
      savedQueries: [],
      activeTabId: fallbackTab.id,
      selectedTableKey: null,
      inspectorCollapsed: false,
      sizes: DEFAULT_WORKSPACE_SIZES,
      explorerTree: {
        favoritesExpanded: true,
        expandedNamespaces: {},
        expandedTables: {},
        filter: "",
      },
    };
  }
}

export function saveSqlStudioWorkspaceState(state: SqlStudioWorkspaceState): void {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(SQL_STUDIO_WORKSPACE_STORAGE_KEY, JSON.stringify(state));
}
