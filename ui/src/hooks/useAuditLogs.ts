import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface AuditLog {
  audit_id: string;
  timestamp: string;
  actor_user_id: string;
  actor_username: string;
  action: string;
  target: string;
  details: string | null;
  ip_address: string | null;
}

export interface AuditLogFilters {
  username?: string;
  action?: string;
  target?: string;
  startDate?: string;
  endDate?: string;
  limit?: number;
}

export function useAuditLogs() {
  const [logs, setLogs] = useState<AuditLog[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchLogs = useCallback(async (filters?: AuditLogFilters) => {
    setIsLoading(true);
    setError(null);
    try {
      // Build SQL with optional filters
      let sql = `
        SELECT audit_id, timestamp, actor_user_id, actor_username, action, target, details, ip_address
        FROM kalam.system.audit_logs
      `;
      
      const conditions: string[] = [];
      
      if (filters?.username) {
        conditions.push(`actor_username LIKE '%${filters.username}%'`);
      }
      if (filters?.action) {
        conditions.push(`action = '${filters.action}'`);
      }
      if (filters?.target) {
        conditions.push(`target LIKE '%${filters.target}%'`);
      }
      if (filters?.startDate) {
        conditions.push(`timestamp >= '${filters.startDate}'`);
      }
      if (filters?.endDate) {
        conditions.push(`timestamp <= '${filters.endDate}'`);
      }
      
      if (conditions.length > 0) {
        sql += ` WHERE ${conditions.join(' AND ')}`;
      }
      
      sql += ` ORDER BY timestamp DESC`;
      
      if (filters?.limit) {
        sql += ` LIMIT ${filters.limit}`;
      } else {
        sql += ` LIMIT 1000`; // Default limit
      }
      
      const rows = await executeSql(sql);
      
      const logList = rows.map((row) => ({
        audit_id: String(row.audit_id ?? ''),
        timestamp: String(row.timestamp ?? ''),
        actor_user_id: String(row.actor_user_id ?? ''),
        actor_username: String(row.actor_username ?? ''),
        action: String(row.action ?? ''),
        target: String(row.target ?? ''),
        details: row.details as string | null,
        ip_address: row.ip_address as string | null,
      }));
      
      setLogs(logList);
      return logList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch audit logs';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    logs,
    isLoading,
    error,
    fetchLogs,
  };
}
