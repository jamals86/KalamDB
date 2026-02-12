export interface ServerLogFilters {
  level?: string;
  target?: string;
  message?: string;
  limit?: number;
}

function escapeSqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

export function buildServerLogsQuery(filters?: ServerLogFilters): string {
  let sql = `
    SELECT timestamp, level, thread, target, line, message
    FROM system.server_logs
  `;

  const conditions: string[] = [];
  if (filters?.level) {
    conditions.push(`level = '${escapeSqlLiteral(filters.level)}'`);
  }
  if (filters?.target) {
    conditions.push(`target LIKE '%${escapeSqlLiteral(filters.target)}%'`);
  }
  if (filters?.message) {
    conditions.push(`message LIKE '%${escapeSqlLiteral(filters.message)}%'`);
  }

  if (conditions.length > 0) {
    sql += ` WHERE ${conditions.join(" AND ")}`;
  }

  sql += " ORDER BY timestamp DESC";
  sql += ` LIMIT ${filters?.limit ?? 500}`;
  return sql;
}
