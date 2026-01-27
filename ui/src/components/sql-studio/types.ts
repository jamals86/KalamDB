import type { ColumnDef } from "@tanstack/react-table";
import type { Unsubscribe } from "@/lib/kalam-client";

export const STORAGE_KEY = "kalamdb-sql-studio-tabs";

// Persisted tab state (subset of QueryTab that we save to localStorage)
export interface PersistedTab {
  id: string;
  name: string;
  query: string;
  isLive: boolean;
}

export interface PersistedState {
  tabs: PersistedTab[];
  activeTabId: string;
  tabCounter: number;
}

export interface QueryTab {
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

export interface SchemaNode {
  name: string;
  type: "namespace" | "table" | "column";
  tableType?: "user" | "shared" | "stream" | "system"; // For table nodes
  dataType?: string;
  isNullable?: boolean;
  isPrimaryKey?: boolean;
  children?: SchemaNode[];
  isExpanded?: boolean;
}

export interface QueryHistoryItem {
  id: string;
  query: string;
  timestamp: Date;
  executionTime: number;
  rowCount: number;
  success: boolean;
}
