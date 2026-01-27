import { useMemo } from 'react';
import { useGetSettingsQuery } from '../store/apiSlice';

export interface Setting {
  name: string;
  value: string;
  description: string;
  category: string;
}

export function useSettings() {
  const { data, isLoading, error, refetch } = useGetSettingsQuery();

  const settings = useMemo(() => {
    if (!data || data.length === 0) {
      // If system.settings doesn't exist or is empty, provide basic info
      return [
        {
          name: 'server.version',
          value: '0.1.0',
          description: 'KalamDB server version',
          category: 'Server',
        },
        {
          name: 'storage.default_backend',
          value: 'rocksdb',
          description: 'Default storage backend for write operations',
          category: 'Storage',
        },
        {
          name: 'query.max_rows',
          value: '10000',
          description: 'Maximum rows returned per query',
          category: 'Query',
        },
        {
          name: 'auth.jwt_expiry',
          value: '3600',
          description: 'JWT token expiry in seconds',
          category: 'Authentication',
        },
      ] as Setting[];
    }
    
    return data.map((row) => ({
      name: String(row.name ?? ''),
      value: String(row.value ?? ''),
      description: String(row.description ?? ''),
      category: String(row.category ?? ''),
    })) as Setting[];
  }, [data]);

  const groupedSettings = useMemo(() => {
    const groups: Record<string, Setting[]> = {};
    settings.forEach(setting => {
      if (!groups[setting.category]) {
        groups[setting.category] = [];
      }
      groups[setting.category].push(setting);
    });
    return groups;
  }, [settings]);

  return {
    settings,
    groupedSettings,
    isLoading,
    error: error ? (error as any).error : null,
    fetchSettings: refetch,
  };
}
