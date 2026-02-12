import { executeSql } from "@/lib/kalam-client";
import {
  buildServerLogsQuery,
  type ServerLogFilters,
} from "@/services/sql/queries/serverLogQueries";

export interface ServerLog {
  timestamp: number | string;
  level: string;
  thread: string | null;
  target: string | null;
  line: number | null;
  message: string;
}

export type { ServerLogFilters };

export async function fetchServerLogs(filters?: ServerLogFilters): Promise<ServerLog[]> {
  const rows = await executeSql(buildServerLogsQuery(filters));
  return rows.map((row) => ({
    timestamp: row.timestamp as number | string,
    level: String(row.level ?? ""),
    thread: row.thread as string | null,
    target: row.target as string | null,
    line: row.line as number | null,
    message: String(row.message ?? ""),
  }));
}
