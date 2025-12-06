import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface Storage {
  storage_id: string;
  storage_name: string;
  description: string | null;
  storage_type: string;
  base_directory: string;
  created_at: string;
  updated_at: string;
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
        SELECT storage_id, storage_name, description, storage_type, base_directory, created_at, updated_at
        FROM system.storages
        ORDER BY storage_name
      `);
      
      const storageList = rows.map((row) => ({
        storage_id: String(row.storage_id ?? ''),
        storage_name: String(row.storage_name ?? ''),
        description: row.description as string | null,
        storage_type: String(row.storage_type ?? ''),
        base_directory: String(row.base_directory ?? ''),
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

  return {
    storages,
    isLoading,
    error,
    fetchStorages,
  };
}
