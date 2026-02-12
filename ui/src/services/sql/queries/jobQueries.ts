export interface JobFilters {
  status?: string;
  job_type?: string;
  limit?: number;
}

function escapeSqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

export function buildSystemJobsQuery(filters?: JobFilters): string {
  let sql = `
    SELECT job_id, job_type, status, parameters, result, 
           trace, error_message, memory_used, cpu_used, created_at, started_at, 
           completed_at, node_id
    FROM system.jobs
  `;

  const conditions: string[] = [];
  if (filters?.status) {
    conditions.push(`status = '${escapeSqlLiteral(filters.status)}'`);
  }
  if (filters?.job_type) {
    conditions.push(`job_type = '${escapeSqlLiteral(filters.job_type)}'`);
  }
  if (conditions.length > 0) {
    sql += ` WHERE ${conditions.join(" AND ")}`;
  }

  sql += " ORDER BY created_at DESC";
  sql += ` LIMIT ${filters?.limit ?? 1000}`;
  return sql;
}
