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

function buildAuditLogsSelect(): string {
  return `
    SELECT audit_id, timestamp, actor_user_id, actor_username, action, target, details, ip_address
    FROM system.audit_log
  `;
}

function buildAuditLogsWhereClause(filters?: AuditLogFilters): string {
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

  return conditions.length > 0 ? ` WHERE ${conditions.join(" AND ")}` : "";
}

export function buildAuditLogsSubscriptionQuery(filters?: AuditLogFilters): string {
  return `${buildAuditLogsSelect()}${buildAuditLogsWhereClause(filters)}`;
}

export function buildAuditLogsQuery(filters?: AuditLogFilters): string {
  let sql = buildAuditLogsSubscriptionQuery(filters);

  sql += " ORDER BY timestamp DESC";
  sql += ` LIMIT ${filters?.limit ?? 1000}`;
  return sql;
}
