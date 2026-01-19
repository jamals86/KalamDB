use std::sync::atomic::{AtomicUsize, Ordering};
use sysinfo::System;

/// Global counter for active WebSocket sessions
/// This is updated by kalamdb-api when sessions start/stop
static ACTIVE_WEBSOCKET_SESSIONS: AtomicUsize = AtomicUsize::new(0);

/// Increment the active WebSocket session count
pub fn increment_websocket_sessions() -> usize {
    ACTIVE_WEBSOCKET_SESSIONS.fetch_add(1, Ordering::SeqCst) + 1
}

/// Decrement the active WebSocket session count
pub fn decrement_websocket_sessions() -> usize {
    ACTIVE_WEBSOCKET_SESSIONS.fetch_sub(1, Ordering::SeqCst) - 1
}

/// Get the current active WebSocket session count
pub fn get_websocket_session_count() -> usize {
    ACTIVE_WEBSOCKET_SESSIONS.load(Ordering::SeqCst)
}

/// Health metrics snapshot
#[derive(Debug, Clone)]
pub struct HealthMetrics {
    pub memory_mb: Option<u64>,
    pub cpu_usage: Option<f32>,
    pub open_files: usize,
    pub namespace_count: usize,
    pub table_count: usize,
    pub subscription_count: usize,
    pub connection_count: usize,
    pub ws_session_count: usize,
    pub jobs_running: usize,
    pub jobs_queued: usize,
    pub jobs_failed: usize,
    pub jobs_total: usize,
}

/// Aggregated counts for health metrics.
#[derive(Debug, Clone, Copy)]
pub struct HealthCounts {
    pub namespace_count: usize,
    pub table_count: usize,
    pub subscription_count: usize,
    pub connection_count: usize,
    pub jobs_running: usize,
    pub jobs_queued: usize,
    pub jobs_failed: usize,
    pub jobs_total: usize,
}

/// Monitor for system health and job statistics
pub struct HealthMonitor;

impl HealthMonitor {
    /// Collect system health metrics
    ///
    /// Returns a structured snapshot of current health metrics.
    /// This is a low-level collector that doesn't log - consumers can log or report metrics as needed.
    pub fn collect_system_metrics() -> (Option<u64>, Option<f32>, usize) {
        let mut sys = System::new_all();
        sys.refresh_all();

        // Get process info
        let pid = match sysinfo::get_current_pid() {
            Ok(pid) => pid,
            Err(e) => {
                log::warn!("Failed to get current process ID for health metrics: {}", e);
                return (None, None, 0);
            },
        };

        let process = sys.process(pid);

        // Get open file descriptor count (Unix only)
        #[cfg(unix)]
        let open_files = Self::count_open_files();
        #[cfg(not(unix))]
        let open_files = 0;

        if let Some(proc) = process {
            let memory_mb = proc.memory() / 1024 / 1024;
            let cpu_usage = proc.cpu_usage();
            (Some(memory_mb), Some(cpu_usage), open_files)
        } else {
            (None, None, open_files)
        }
    }

    /// Build complete health metrics snapshot
    pub fn build_metrics(
        memory_mb: Option<u64>,
        cpu_usage: Option<f32>,
        open_files: usize,
        counts: HealthCounts,
    ) -> HealthMetrics {
        HealthMetrics {
            memory_mb,
            cpu_usage,
            open_files,
            namespace_count: counts.namespace_count,
            table_count: counts.table_count,
            subscription_count: counts.subscription_count,
            connection_count: counts.connection_count,
            ws_session_count: get_websocket_session_count(),
            jobs_running: counts.jobs_running,
            jobs_queued: counts.jobs_queued,
            jobs_failed: counts.jobs_failed,
            jobs_total: counts.jobs_total,
        }
    }

    /// Log health metrics to debug output
    pub fn log_metrics(metrics: &HealthMetrics) {
        if let (Some(memory_mb), Some(cpu_usage)) = (metrics.memory_mb, metrics.cpu_usage) {
            log::debug!(
                "Health metrics: Memory: {} MB | CPU: {:.2}% | Open Files: {} | Namespaces: {} | Tables: {} | Subscriptions: {} ({} connections, {} ws sessions) | Jobs: {} running, {} queued, {} failed (total: {})",
                memory_mb,
                cpu_usage,
                metrics.open_files,
                metrics.namespace_count,
                metrics.table_count,
                metrics.subscription_count,
                metrics.connection_count,
                metrics.ws_session_count,
                metrics.jobs_running,
                metrics.jobs_queued,
                metrics.jobs_failed,
                metrics.jobs_total
            );
        } else {
            log::debug!(
                "Health metrics: Open Files: {} | Namespaces: {} | Tables: {} | Subscriptions: {} ({} connections, {} ws sessions) | Jobs: {} running, {} queued, {} failed (total: {})",
                metrics.open_files,
                metrics.namespace_count,
                metrics.table_count,
                metrics.subscription_count,
                metrics.connection_count,
                metrics.ws_session_count,
                metrics.jobs_running,
                metrics.jobs_queued,
                metrics.jobs_failed,
                metrics.jobs_total
            );
        }
    }

    /// Count open file descriptors for the current process (Unix only)
    #[cfg(unix)]
    fn count_open_files() -> usize {
        use std::process::Command;

        if let Ok(output) = Command::new("sh")
            .arg("-c")
            .arg(format!("lsof -p {} 2>/dev/null | wc -l", std::process::id()))
            .output()
        {
            if let Ok(count_str) = String::from_utf8(output.stdout) {
                if let Ok(count) = count_str.trim().parse::<usize>() {
                    // lsof includes header line, so subtract 1
                    return count.saturating_sub(1);
                }
            }
        }

        0
    }
}
