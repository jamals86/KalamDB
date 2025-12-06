import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface Setting {
  name: string;
  value: string;
  description: string;
  category: string;
}

export function useSettings() {
  const [settings, setSettings] = useState<Setting[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchSettings = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      // Try to query settings from system.settings if it exists
      // Otherwise, return some default configuration info
      const rows = await executeSql(`
        SELECT name, value, description, category
        FROM system.settings
        ORDER BY category, name
      `);
      
      const settingsList = rows.map((row) => ({
        name: String(row.name ?? ''),
        value: String(row.value ?? ''),
        description: String(row.description ?? ''),
        category: String(row.category ?? ''),
      }));
      
      setSettings(settingsList);
      return settingsList;
    } catch {
      // If system.settings doesn't exist, provide basic info
      const defaultSettings: Setting[] = [
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
      ];
      setSettings(defaultSettings);
      return defaultSettings;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const groupedSettings = useCallback(() => {
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
    groupedSettings: groupedSettings(),
    isLoading,
    error,
    fetchSettings,
  };
}
