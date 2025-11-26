use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::jobs::JobsManager;
use kalamdb_commons::system::JobFilter;
use kalamdb_commons::JobStatus;
use std::sync::Arc;
use sysinfo::System;

/// Monitor for system health and job statistics
pub struct HealthMonitor;

impl HealthMonitor {
    /// Log system health metrics for monitoring
    ///
    /// Logs memory usage, CPU usage, open files count, namespace/table counts, and active job statistics.
    pub async fn log_metrics(
        jobs_manager: &JobsManager,
        app_context: Arc<AppContext>,
    ) -> Result<(), KalamDbError> {
        let mut sys = System::new_all();
        sys.refresh_all();

        // Get process info
        let pid = sysinfo::get_current_pid().unwrap();
        let process = sys.process(pid);

        // Get job statistics
        let all_jobs = jobs_manager.list_jobs(JobFilter::default()).await?;
        let running_count = all_jobs
            .iter()
            .filter(|j| j.status == JobStatus::Running)
            .count();
        let queued_count = all_jobs
            .iter()
            .filter(|j| j.status == JobStatus::Queued)
            .count();
        let failed_count = all_jobs
            .iter()
            .filter(|j| j.status == JobStatus::Failed)
            .count();

        // Get namespace and table counts from system tables
        let namespaces_provider = app_context.system_tables().namespaces();
        let tables_provider = app_context.system_tables().tables();
        
        let namespace_count = namespaces_provider.list_namespaces().map(|ns| ns.len()).unwrap_or(0);
        let table_count = tables_provider.list_tables().map(|ts| ts.len()).unwrap_or(0);

        // Get live subscription stats
        let live_stats = app_context.live_query_manager().get_stats().await;
        let subscription_count = live_stats.total_subscriptions;
        let connection_count = live_stats.total_connections;

        // Get open file descriptor count (Unix only)
        #[cfg(unix)]
        let open_files = Self::count_open_files();
        #[cfg(not(unix))]
        let open_files = 0;

        if let Some(proc) = process {
            // Memory usage in MB
            let memory_mb = proc.memory() / 1024 / 1024;

            // CPU usage percentage
            let cpu_usage = proc.cpu_usage();

            log::debug!(
                "Health metrics: Memory: {} MB | CPU: {:.2}% | Open Files: {} | Namespaces: {} | Tables: {} | Subscriptions: {} ({} connections) | Jobs: {} running, {} queued, {} failed (total: {})",
                memory_mb,
                cpu_usage,
                open_files,
                namespace_count,
                table_count,
                subscription_count,
                connection_count,
                running_count,
                queued_count,
                failed_count,
                all_jobs.len()
            );
        } else {
            log::debug!(
                "Health metrics: Open Files: {} | Namespaces: {} | Tables: {} | Subscriptions: {} ({} connections) | Jobs: {} running, {} queued, {} failed (total: {})",
                open_files,
                namespace_count,
                table_count,
                subscription_count,
                connection_count,
                running_count,
                queued_count,
                failed_count,
                all_jobs.len()
            );
        }

        Ok(())
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
