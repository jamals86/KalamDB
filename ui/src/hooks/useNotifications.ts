import { useState, useEffect, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';
import { Job } from './useJobs';
import { AuditLog } from './useAuditLogs';

export interface Notifications {
  runningJobs: Job[];
  recentAuditLogs: AuditLog[];
  runningJobsCount: number;
  hasUnread: boolean;
}

export function useNotifications(refreshInterval = 3000) {
  const [notifications, setNotifications] = useState<Notifications>({
    runningJobs: [],
    recentAuditLogs: [],
    runningJobsCount: 0,
    hasUnread: false,
  });
  const [isLoading, setIsLoading] = useState(false);
  const [lastFetchTime, setLastFetchTime] = useState<Date | null>(null);

  const fetchNotifications = useCallback(async () => {
    setIsLoading(true);
    try {
      // Fetch running/queued jobs
      const jobsSql = `
        SELECT job_id, job_type, status, parameters, created_at, started_at, node_id
        FROM system.jobs
        WHERE status = 'Running' OR status = 'Queued' OR status = 'New'
        ORDER BY created_at DESC
        LIMIT 10
      `;
      
      // Fetch recent audit logs (last 10)
      const auditSql = `
        SELECT audit_id, timestamp, actor_user_id, actor_username, action, target, details, ip_address
        FROM system.audit_log
        ORDER BY timestamp DESC
        LIMIT 10
      `;
      
      const [jobRows, auditRows] = await Promise.all([
        executeSql(jobsSql),
        executeSql(auditSql),
      ]);
      
      const runningJobs = jobRows.map((row) => ({
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
      
      const recentAuditLogs = auditRows.map((row) => ({
        audit_id: String(row.audit_id ?? ''),
        timestamp: String(row.timestamp ?? ''),
        actor_user_id: String(row.actor_user_id ?? ''),
        actor_username: String(row.actor_username ?? ''),
        action: String(row.action ?? ''),
        target: String(row.target ?? ''),
        details: row.details as string | null,
        ip_address: row.ip_address as string | null,
      })) as AuditLog[];
      
      const runningCount = runningJobs.filter(j => j.status === 'Running').length;
      
      setNotifications({
        runningJobs,
        recentAuditLogs,
        runningJobsCount: runningCount,
        hasUnread: runningCount > 0,
      });
      setLastFetchTime(new Date());
    } catch (err) {
      console.error('Failed to fetch notifications:', err);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Auto-refresh every `refreshInterval` ms
  useEffect(() => {
    fetchNotifications();
    
    const interval = setInterval(fetchNotifications, refreshInterval);
    return () => clearInterval(interval);
  }, [fetchNotifications, refreshInterval]);

  return {
    notifications,
    isLoading,
    lastFetchTime,
    refresh: fetchNotifications,
  };
}
