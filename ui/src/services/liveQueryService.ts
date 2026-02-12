import { executeSql } from "@/lib/kalam-client";
import {
  buildKillLiveQuerySql,
  buildLiveQueriesQuery,
  type LiveQueryFilters,
} from "@/services/sql/queries/liveQueryQueries";

export interface LiveQuery {
  live_id: string;
  connection_id: string;
  subscription_id: string;
  namespace_id: string;
  table_name: string;
  user_id: string;
  query: string;
  options: string | null;
  status: string;
  created_at: number;
  last_update: number;
  changes: number;
  node: string;
}

export type { LiveQueryFilters };

export async function fetchLiveQueries(filters?: LiveQueryFilters): Promise<LiveQuery[]> {
  const rows = await executeSql(buildLiveQueriesQuery(filters));
  return rows.map((row) => ({
    live_id: String(row.live_id ?? ""),
    connection_id: String(row.connection_id ?? ""),
    subscription_id: String(row.subscription_id ?? ""),
    namespace_id: String(row.namespace_id ?? ""),
    table_name: String(row.table_name ?? ""),
    user_id: String(row.user_id ?? ""),
    query: String(row.query ?? ""),
    options: row.options as string | null,
    status: String(row.status ?? ""),
    created_at: Number(row.created_at ?? 0),
    last_update: Number(row.last_update ?? 0),
    changes: Number(row.changes ?? 0),
    node: String(row.node_id ?? ""),
  }));
}

export async function killLiveQuery(liveId: string): Promise<void> {
  await executeSql(buildKillLiveQuerySql(liveId));
}
