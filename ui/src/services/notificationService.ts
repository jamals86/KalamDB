import { fetchAuditLogs, type AuditLog } from "@/services/auditLogService";
import { fetchJobs, type Job } from "@/services/jobService";

export interface NotificationsPayload {
  runningJobs: Job[];
  recentAuditLogs: AuditLog[];
  runningJobsCount: number;
  hasUnread: boolean;
  fetchedAt: string;
}

export async function fetchNotifications(): Promise<NotificationsPayload> {
  const [runningJobs, recentAuditLogs] = await Promise.all([
    fetchJobs({ limit: 10, status: "Running" }),
    fetchAuditLogs({ limit: 10 }),
  ]);

  return {
    runningJobs,
    recentAuditLogs,
    runningJobsCount: runningJobs.length,
    hasUnread: runningJobs.length > 0,
    fetchedAt: new Date().toISOString(),
  };
}
