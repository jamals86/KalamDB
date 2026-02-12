export interface AuditLogFilters {
  username?: string;
  action?: string;
  target?: string;
  startDate?: string;
  endDate?: string;
  limit?: number;
}

function escapeSqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

export function buildAuditLogsQuery(filters?: AuditLogFilters): string {
  let sql = `
    SELECT audit_id, timestamp, actor_user_id, actor_username, action, target, details, ip_address
    FROM system.audit_log
  `;

  const conditions: string[] = [];
  if (filters?.username) {
    conditions.push(`actor_username LIKE '%${escapeSqlLiteral(filters.username)}%'`);
  }
  if (filters?.action) {
    conditions.push(`action = '${escapeSqlLiteral(filters.action)}'`);
  }
  if (filters?.target) {
    conditions.push(`target LIKE '%${escapeSqlLiteral(filters.target)}%'`);
  }
  if (filters?.startDate) {
    conditions.push(`timestamp >= '${escapeSqlLiteral(filters.startDate)}'`);
  }
  if (filters?.endDate) {
    conditions.push(`timestamp <= '${escapeSqlLiteral(filters.endDate)}'`);
  }

  if (conditions.length > 0) {
    sql += ` WHERE ${conditions.join(" AND ")}`;
  }

  sql += " ORDER BY timestamp DESC";
  sql += ` LIMIT ${filters?.limit ?? 1000}`;
  return sql;
}
