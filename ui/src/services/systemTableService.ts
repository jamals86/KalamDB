import { getDb } from "@/lib/db";
import type { DbaStatRow as DbaStatRecord, SystemSettingRow } from "@/lib/models";
import { system_settings, system_stats, dba_stats } from "@/lib/schema";
import { asc } from "drizzle-orm";

export type Setting = SystemSettingRow;
export type SystemStatsMap = Record<string, string>;

export interface DbaStatRow {
  sampled_at: number;
  metric_name: string;
  metric_value: number;
}

export async function fetchSystemSettings(): Promise<Setting[]> {
  const db = getDb();
  return db.select().from(system_settings);
}

export function mapSettingsRows(rows: Setting[]): Setting[] {
  if (rows.length === 0) {
    const fallbackRows: Setting[] = [
      { name: "server.version", value: "0.1.0", description: "KalamDB server version", category: "server" },
      { name: "storage.default_backend", value: "rocksdb", description: "Default storage backend for write operations", category: "storage" },
      { name: "query.max_rows", value: "10000", description: "Maximum rows returned per query", category: "query" },
      { name: "auth.jwt_expiry", value: "3600", description: "JWT token expiry in seconds", category: "auth" },
    ];
    return fallbackRows;
  }
  return rows;
}

export async function fetchSystemStats(): Promise<SystemStatsMap> {
  const db = getDb();
  const rows = await db.select().from(system_stats);
  const stats: SystemStatsMap = {};
  for (const row of rows) {
    if (row.metric_name) {
      stats[row.metric_name] = String(row.metric_value ?? "");
    }
  }
  return stats;
}

const SUPPORTED_DBA_METRICS = new Set([
  "active_connections",
  "active_subscriptions",
  "memory_usage_mb",
  "cpu_usage_percent",
  "total_jobs",
  "jobs_running",
  "jobs_queued",
  "total_tables",
  "total_namespaces",
  "open_files_total",
  "open_files_regular",
]);

function normalizeEpochMillis(value: unknown): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    let epochMillis = value;
    while (Math.abs(epochMillis) >= 1e15) {
      epochMillis /= 1000;
    }
    return Math.trunc(epochMillis);
  }
  if (typeof value === "string") {
    const numeric = Number(value.trim());
    if (Number.isFinite(numeric)) return normalizeEpochMillis(numeric);
    const parsed = new Date(value.trim()).getTime();
    return Number.isNaN(parsed) ? 0 : parsed;
  }
  return 0;
}

function normalizeMetricValue(value: unknown): number {
  if (typeof value === "number") return Number.isFinite(value) ? value : Number.NaN;
  if (typeof value === "string") {
    const numeric = Number(value.trim());
    return Number.isFinite(numeric) ? numeric : Number.NaN;
  }
  return Number.NaN;
}

function getTimeRangeCutoff(timeRange: string): number {
  const match = timeRange.trim().match(/^(\d+)\s+(HOUR|HOURS|DAY|DAYS)$/i);
  if (!match) return 0;
  const amount = Number(match[1]);
  if (!Number.isFinite(amount) || amount <= 0) return 0;
  const unit = match[2].toUpperCase();
  const multiplier = unit.startsWith("DAY") ? 24 * 60 * 60 * 1000 : 60 * 60 * 1000;
  return Date.now() - amount * multiplier;
}

export async function fetchDbaStats(timeRange: string = "24 HOURS"): Promise<DbaStatRow[]> {
  const db = getDb();
  const rows: DbaStatRecord[] = await db.select().from(dba_stats).orderBy(asc(dba_stats.sampled_at));
  const cutoff = getTimeRangeCutoff(timeRange);

  return rows
    .map((row) => ({
      sampled_at: normalizeEpochMillis(row.sampled_at),
      metric_name: String(row.metric_name ?? ""),
      metric_value: normalizeMetricValue(row.metric_value),
    }))
    .filter((row) => {
      if (!row.metric_name || !SUPPORTED_DBA_METRICS.has(row.metric_name)) return false;
      if (!Number.isFinite(row.metric_value) || row.sampled_at <= 0) return false;
      return cutoff === 0 || row.sampled_at >= cutoff;
    });
}
