import { executeSql } from "@/lib/kalam-client";
import {
  SYSTEM_SETTINGS_QUERY,
  SYSTEM_STATS_QUERY,
  SYSTEM_USERS_QUERY,
} from "@/services/sql/queries/systemQueries";

export async function fetchSystemSettings(): Promise<Record<string, unknown>[]> {
  return executeSql(SYSTEM_SETTINGS_QUERY);
}

export async function fetchSystemUsers(): Promise<Record<string, unknown>[]> {
  return executeSql(SYSTEM_USERS_QUERY);
}

export interface Setting {
  name: string;
  value: string;
  description: string;
  category: string;
}

export function mapSettingsRows(rows: Record<string, unknown>[]): Setting[] {
  if (rows.length === 0) {
    return [
      {
        name: "server.version",
        value: "0.1.0",
        description: "KalamDB server version",
        category: "Server",
      },
      {
        name: "storage.default_backend",
        value: "rocksdb",
        description: "Default storage backend for write operations",
        category: "Storage",
      },
      {
        name: "query.max_rows",
        value: "10000",
        description: "Maximum rows returned per query",
        category: "Query",
      },
      {
        name: "auth.jwt_expiry",
        value: "3600",
        description: "JWT token expiry in seconds",
        category: "Authentication",
      },
    ];
  }

  return rows.map((row) => ({
    name: String(row.name ?? ""),
    value: String(row.value ?? ""),
    description: String(row.description ?? ""),
    category: String(row.category ?? ""),
  }));
}

export type SystemStatsMap = Record<string, string>;

export async function fetchSystemStats(): Promise<SystemStatsMap> {
  const rows = await executeSql(SYSTEM_STATS_QUERY);
  const stats: SystemStatsMap = {};

  rows.forEach((row) => {
    const metricName = String(row.metric_name ?? "");
    if (!metricName) {
      return;
    }

    stats[metricName] = String(row.metric_value ?? "");
  });

  return stats;
}
