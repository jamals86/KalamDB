import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface Storage {
  storage_id: string;
  storage_name: string;
  description: string | null;
  storage_type: string;
  base_directory: string;
  shared_tables_template: string | null;
  user_tables_template: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateStorageInput {
  storage_id: string;
  storage_type: 'filesystem' | 's3' | 'gcs' | 'azure';
  storage_name: string;
  description?: string;
  base_directory: string;
  shared_tables_template?: string;
  user_tables_template?: string;
}

export interface UpdateStorageInput {
  storage_name?: string;
  description?: string;
  shared_tables_template?: string;
  user_tables_template?: string;
}

export interface StorageHealthResult {
  storage_id: string;
  status: 'healthy' | 'degraded' | 'unreachable';
  readable: boolean;
  writable: boolean;
  listable: boolean;
  deletable: boolean;
  latency_ms: number;
  total_bytes: number | null;
  used_bytes: number | null;
  error: string | null;
  tested_at: number | null;
}

export function useStorages() {
  const [storages, setStorages] = useState<Storage[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchStorages = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await executeSql(`
        SELECT storage_id, storage_name, description, storage_type, base_directory, 
               shared_tables_template, user_tables_template, created_at, updated_at
        FROM system.storages
        ORDER BY storage_name
      `);
      
      const storageList = rows.map((row) => ({
        storage_id: String(row.storage_id ?? ''),
        storage_name: String(row.storage_name ?? ''),
        description: row.description as string | null,
        storage_type: String(row.storage_type ?? ''),
        base_directory: String(row.base_directory ?? ''),
        shared_tables_template: row.shared_tables_template as string | null,
        user_tables_template: row.user_tables_template as string | null,
        created_at: String(row.created_at ?? ''),
        updated_at: String(row.updated_at ?? ''),
      }));
      
      setStorages(storageList);
      return storageList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch storages';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const createStorage = useCallback(async (input: CreateStorageInput) => {
    setError(null);
    try {
      // Build CREATE STORAGE SQL
      let sql = `CREATE STORAGE ${input.storage_id} TYPE ${input.storage_type}`;
      sql += ` NAME '${input.storage_name}'`;
      
      if (input.description) {
        sql += ` DESCRIPTION '${input.description}'`;
      }
      
      // Use appropriate keyword based on storage type
      if (input.storage_type === 'filesystem') {
        sql += ` PATH '${input.base_directory}'`;
      } else {
        sql += ` BUCKET '${input.base_directory}'`;
      }
      
      if (input.shared_tables_template) {
        sql += ` SHARED_TABLES_TEMPLATE '${input.shared_tables_template}'`;
      }
      
      if (input.user_tables_template) {
        sql += ` USER_TABLES_TEMPLATE '${input.user_tables_template}'`;
      }
      
      await executeSql(sql);
      await fetchStorages(); // Refresh the list
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to create storage';
      setError(errorMessage);
      throw err;
    }
  }, [fetchStorages]);

  const updateStorage = useCallback(async (storageId: string, input: UpdateStorageInput) => {
    setError(null);
    try {
      // Build ALTER STORAGE SQL with SET clauses
      const setClauses: string[] = [];
      
      if (input.storage_name) {
        setClauses.push(`SET NAME '${input.storage_name}'`);
      }
      if (input.description !== undefined) {
        setClauses.push(`SET DESCRIPTION '${input.description || ''}'`);
      }
      if (input.shared_tables_template !== undefined) {
        setClauses.push(`SET SHARED_TABLES_TEMPLATE '${input.shared_tables_template || ''}'`);
      }
      if (input.user_tables_template !== undefined) {
        setClauses.push(`SET USER_TABLES_TEMPLATE '${input.user_tables_template || ''}'`);
      }
      
      if (setClauses.length === 0) {
        return; // Nothing to update
      }
      
      const sql = `ALTER STORAGE ${storageId} ${setClauses.join(' ')}`;
      await executeSql(sql);
      await fetchStorages(); // Refresh the list
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to update storage';
      setError(errorMessage);
      throw err;
    }
  }, [fetchStorages]);

  const checkStorageHealth = useCallback(async (storageId: string, extended = true) => {
    setError(null);
    try {
      const sql = extended
        ? `STORAGE CHECK ${storageId} EXTENDED`
        : `STORAGE CHECK ${storageId}`;
      const rows = await executeSql(sql);
      if (!rows || rows.length === 0) {
        throw new Error('No health check result returned');
      }

      const row = rows[0];
      const toBool = (value: unknown): boolean =>
        value === true || value === 'true' || value === 1 || value === '1';
      const toNumber = (value: unknown): number | null =>
        value === null || value === undefined ? null : Number(value);

      return {
        storage_id: String(row.storage_id ?? storageId),
        status: String(row.status ?? 'unreachable') as StorageHealthResult['status'],
        readable: toBool(row.readable),
        writable: toBool(row.writable),
        listable: toBool(row.listable),
        deletable: toBool(row.deletable),
        latency_ms: Number(row.latency_ms ?? 0),
        total_bytes: toNumber(row.total_bytes),
        used_bytes: toNumber(row.used_bytes),
        error: row.error ? String(row.error) : null,
        tested_at: toNumber(row.tested_at),
      } as StorageHealthResult;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to check storage health';
      setError(errorMessage);
      throw err;
    }
  }, []);

  return {
    storages,
    isLoading,
    error,
    fetchStorages,
    createStorage,
    updateStorage,
    checkStorageHealth,
  };
}
