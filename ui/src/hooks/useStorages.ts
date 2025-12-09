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
  storage_type: 'filesystem' | 's3';
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

  return {
    storages,
    isLoading,
    error,
    fetchStorages,
    createStorage,
    updateStorage,
  };
}
