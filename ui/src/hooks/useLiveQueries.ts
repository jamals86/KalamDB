import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

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

export interface LiveQueryFilters {
  user_id?: string;
  namespace_id?: string;
  table_name?: string;
  status?: string;
  limit?: number;
}

export function useLiveQueries() {
  const [liveQueries, setLiveQueries] = useState<LiveQuery[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchLiveQueries = useCallback(async (filters?: LiveQueryFilters) => {
    setIsLoading(true);
    setError(null);
    try {
      let sql = `
        SELECT live_id, connection_id, subscription_id, namespace_id, 
               table_name, user_id, query, options, status, 
               created_at, last_update, changes, node_id
        FROM system.live_queries
      `;
      
      const conditions: string[] = [];
      
      if (filters?.user_id) {
        conditions.push(`user_id = '${filters.user_id}'`);
      }
      if (filters?.namespace_id) {
        conditions.push(`namespace_id = '${filters.namespace_id}'`);
      }
      if (filters?.table_name) {
        conditions.push(`table_name = '${filters.table_name}'`);
      }
      if (filters?.status) {
        conditions.push(`status = '${filters.status}'`);
      }
      
      if (conditions.length > 0) {
        sql += ` WHERE ${conditions.join(' AND ')}`;
      }
      
      sql += ` ORDER BY created_at DESC`;
      
      if (filters?.limit) {
        sql += ` LIMIT ${filters.limit}`;
      } else {
        sql += ` LIMIT 1000`;
      }
      
      const rows = await executeSql(sql);
      
      const queryList = rows.map((row) => ({
        live_id: String(row.live_id ?? ''),
        connection_id: String(row.connection_id ?? ''),
        subscription_id: String(row.subscription_id ?? ''),
        namespace_id: String(row.namespace_id ?? ''),
        table_name: String(row.table_name ?? ''),
        user_id: String(row.user_id ?? ''),
        query: String(row.query ?? ''),
        options: row.options as string | null,
        status: String(row.status ?? ''),
        created_at: Number(row.created_at ?? 0),
        last_update: Number(row.last_update ?? 0),
        changes: Number(row.changes ?? 0),
        node: String(row.node_id ?? ''),
      }));
      
      setLiveQueries(queryList);
      return queryList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch live queries';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const killLiveQuery = useCallback(async (liveId: string) => {
    try {
      // Use SQL to execute a KILL LIVE QUERY command
      // Note: This might need to be implemented on the backend
      const sql = `KILL LIVE QUERY '${liveId}'`;
      await executeSql(sql);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to kill live query';
      throw new Error(errorMessage);
    }
  }, []);

  const getActiveCount = useCallback(async (): Promise<number> => {
    try {
      const sql = `
        SELECT COUNT(*) as count
        FROM system.live_queries
        WHERE status = 'Active'
      `;
      
      const rows = await executeSql(sql);
      return Number(rows[0]?.count ?? 0);
    } catch (err) {
      console.error('Failed to get active live query count:', err);
      return 0;
    }
  }, []);

  return {
    liveQueries,
    isLoading,
    error,
    fetchLiveQueries,
    killLiveQuery,
    getActiveCount,
  };
}
