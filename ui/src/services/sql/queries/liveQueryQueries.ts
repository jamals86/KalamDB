export interface LiveQueryFilters {
  user_id?: string;
  namespace_id?: string;
  table_name?: string;
  status?: string;
  limit?: number;
}

function escapeSqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

export function buildKillLiveQuerySql(liveId: string): string {
  return `KILL LIVE QUERY '${escapeSqlLiteral(liveId)}'`;
}
