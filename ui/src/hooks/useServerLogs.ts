import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface ServerLog {
  timestamp: string;
  level: string;
  thread: string | null;
  target: string | null;
  line: number | null;
  message: string;
}

export interface ServerLogFilters {
  level?: string;
  target?: string;
  message?: string;
  limit?: number;
}

export function useServerLogs() {
  const [logs, setLogs] = useState<ServerLog[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchLogs = useCallback(async (filters?: ServerLogFilters) => {
    setIsLoading(true);
    setError(null);
    try {
      // Build SQL with optional filters
      let sql = `
        SELECT timestamp, level, thread, target, line, message
        FROM system.server_logs
      `;
      
      const conditions: string[] = [];
      
      if (filters?.level) {
        conditions.push(`level = '${filters.level}'`);
      }
      if (filters?.target) {
        conditions.push(`target LIKE '%${filters.target}%'`);
      }
      if (filters?.message) {
        conditions.push(`message LIKE '%${filters.message}%'`);
      }
      
      if (conditions.length > 0) {
        sql += ` WHERE ${conditions.join(' AND ')}`;
      }
      
      sql += ` ORDER BY timestamp DESC`;
      
      if (filters?.limit) {
        sql += ` LIMIT ${filters.limit}`;
      } else {
        sql += ` LIMIT 500`; // Default limit
      }
      
      const rows = await executeSql(sql);
      
      const logList = rows.map((row) => ({
        timestamp: String(row.timestamp ?? ''),
        level: String(row.level ?? ''),
        thread: row.thread as string | null,
        target: row.target as string | null,
        line: row.line as number | null,
        message: String(row.message ?? ''),
      }));
      
      setLogs(logList);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to fetch server logs';
      setError(message);
      setLogs([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const clearLogs = useCallback(() => {
    setLogs([]);
    setError(null);
  }, []);

  return {
    logs,
    isLoading,
    error,
    fetchLogs,
    clearLogs,
  };
}
