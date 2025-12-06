import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface Namespace {
  name: string;
  table_count: number;
  created_at: string;
}

export interface NamespaceTable {
  namespace: string;
  table_name: string;
  table_type: string;
  row_count: number;
  created_at: string;
}

export function useNamespaces() {
  const [namespaces, setNamespaces] = useState<Namespace[]>([]);
  const [tables, setTables] = useState<NamespaceTable[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchNamespaces = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      // Query namespaces from information_schema
      // Show all namespaces including system ones for admin visibility
      const rows = await executeSql(`
        SELECT DISTINCT table_schema as namespace
        FROM information_schema.tables
        ORDER BY table_schema
      `);
      
      const namespaceList = rows.map((row) => ({
        name: String(row.namespace ?? ''),
        table_count: 0, // Will be populated separately
        created_at: new Date().toISOString(),
      }));
      
      setNamespaces(namespaceList);
      return namespaceList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch namespaces';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const fetchTablesForNamespace = useCallback(async (namespace: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await executeSql(`
        SELECT table_schema, table_name, table_type
        FROM information_schema.tables
        WHERE table_schema = '${namespace}'
        ORDER BY table_name
      `);
      
      const tableList = rows.map((row) => ({
        namespace: String(row.table_schema ?? ''),
        table_name: String(row.table_name ?? ''),
        table_type: String(row.table_type ?? ''),
        row_count: 0,
        created_at: new Date().toISOString(),
      }));
      
      setTables(tableList);
      return tableList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch tables';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const createNamespace = useCallback(async (name: string) => {
    setError(null);
    try {
      // KalamDB uses CREATE NAMESPACE, not CREATE SCHEMA
      await executeSql(`CREATE NAMESPACE ${name}`);
      await fetchNamespaces();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to create namespace';
      setError(errorMessage);
      throw err;
    }
  }, [fetchNamespaces]);

  return {
    namespaces,
    tables,
    isLoading,
    error,
    fetchNamespaces,
    fetchTablesForNamespace,
    createNamespace,
  };
}
