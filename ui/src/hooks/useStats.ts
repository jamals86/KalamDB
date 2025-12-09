import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface Stat {
  metric_name: string;
  metric_value: string;
}

export interface StatsMap {
  [key: string]: string;
}

export function useStats() {
  const [stats, setStats] = useState<StatsMap>({});
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchStats = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await executeSql(`
        SELECT metric_name, metric_value
        FROM system.stats
      `);
      
      // Convert array of {metric_name, metric_value} to a map
      const statsMap: StatsMap = {};
      rows.forEach((row) => {
        const name = String(row.metric_name ?? '');
        const value = String(row.metric_value ?? '');
        if (name) {
          statsMap[name] = value;
        }
      });
      
      setStats(statsMap);
      return statsMap;
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to fetch stats';
      setError(message);
      // Return empty stats on error
      setStats({});
      return {};
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    stats,
    isLoading,
    error,
    fetchStats,
  };
}
