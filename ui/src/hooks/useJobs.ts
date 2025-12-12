import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface Job {
  job_id: string;
  job_type: string;
  status: string;
  parameters: string | null;
  result: string | null;
  trace: string | null;
  error_message: string | null;
  memory_used: number | null;
  cpu_used: number | null;
  created_at: string;
  started_at: string | null;
  completed_at: string | null;
  node_id: string;
}

export interface JobFilters {
  status?: string;
  job_type?: string;
  limit?: number;
}

export function useJobs() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchJobs = useCallback(async (filters?: JobFilters) => {
    setIsLoading(true);
    setError(null);
    try {
      let sql = `
        SELECT job_id, job_type, status, parameters, result, 
               trace, error_message, memory_used, cpu_used, created_at, started_at, 
               completed_at, node_id
        FROM system.jobs
      `;
      
      const conditions: string[] = [];
      
      if (filters?.status) {
        conditions.push(`status = '${filters.status}'`);
      }
      if (filters?.job_type) {
        conditions.push(`job_type = '${filters.job_type}'`);
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
      
      const jobList = rows.map((row) => ({
        job_id: String(row.job_id ?? ''),
        job_type: String(row.job_type ?? ''),
        status: String(row.status ?? ''),
        parameters: row.parameters as string | null,
        result: row.result as string | null,
        trace: row.trace as string | null,
        error_message: row.error_message as string | null,
        memory_used: row.memory_used as number | null,
        cpu_used: row.cpu_used as number | null,
        created_at: String(row.created_at ?? ''),
        started_at: row.started_at as string | null,
        completed_at: row.completed_at as string | null,
        node_id: String(row.node_id ?? ''),
      }));
      
      setJobs(jobList);
      return jobList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch jobs';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const fetchRunningJobs = useCallback(async () => {
    try {
      const sql = `
        SELECT job_id, job_type, status, parameters, created_at, started_at, node_id
        FROM system.jobs
        WHERE status = 'Running' OR status = 'Queued'
        ORDER BY created_at DESC
        LIMIT 50
      `;
      
      const rows = await executeSql(sql);
      
      return rows.map((row) => ({
        job_id: String(row.job_id ?? ''),
        job_type: String(row.job_type ?? ''),
        status: String(row.status ?? ''),
        parameters: row.parameters as string | null,
        result: null,
        trace: null,
        error_message: null,
        memory_used: null,
        cpu_used: null,
        created_at: String(row.created_at ?? ''),
        started_at: row.started_at as string | null,
        completed_at: null,
        node_id: String(row.node_id ?? ''),
      })) as Job[];
    } catch (err) {
      console.error('Failed to fetch running jobs:', err);
      return [];
    }
  }, []);

  return {
    jobs,
    isLoading,
    error,
    fetchJobs,
    fetchRunningJobs,
  };
}
