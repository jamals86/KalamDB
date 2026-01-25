import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface ColumnDescription {
  column_name: string;
  ordinal_position: number;
  column_id: number;
  data_type: string;
  is_nullable: boolean;
  is_primary_key: boolean;
  column_default: string | null;
  column_comment: string | null;
  schema_version: number;
}

export function useDescribeTable() {
  const [columns, setColumns] = useState<ColumnDescription[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const describeTable = useCallback(async (namespace: string, tableName: string) => {
    setIsLoading(true);
    setError(null);
    try {
      const sql = `DESCRIBE TABLE ${namespace}.${tableName}`;
      const rows = await executeSql(sql);
      
      const columnList = rows.map((row) => ({
        column_name: String(row.column_name ?? ''),
        ordinal_position: Number(row.ordinal_position ?? 0),
        column_id: Number(row.column_id ?? 0),
        data_type: String(row.data_type ?? ''),
        is_nullable: Boolean(row.is_nullable ?? false),
        is_primary_key: Boolean(row.is_primary_key ?? false),
        column_default: row.column_default as string | null,
        column_comment: row.column_comment as string | null,
        schema_version: Number(row.schema_version ?? 1),
      }));
      
      setColumns(columnList);
      return columnList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to describe table';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    columns,
    isLoading,
    error,
    describeTable,
  };
}
