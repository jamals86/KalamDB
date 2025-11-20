use crate::error::KalamDbError;
use crate::jobs::JobsManager;
use kalamdb_commons::system::JobFilter;
use kalamdb_commons::JobStatus;
use sysinfo::System;

/// Monitor for system health and job statistics
pub struct HealthMonitor;

impl HealthMonitor {
    /// Log system health metrics for monitoring
    ///
    /// Logs memory usage, CPU usage, thread count, and active job statistics.
    pub async fn log_metrics(jobs_manager: &JobsManager) -> Result<(), KalamDbError> {
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

        if let Some(proc) = process {
            // Memory usage in MB
            let memory_mb = proc.memory() / 1024 / 1024;
            let virtual_memory_mb = proc.virtual_memory() / 1024 / 1024;

            // CPU usage percentage
            let cpu_usage = proc.cpu_usage();

            // Get total number of processes (as proxy for system load)
            let total_processes = sys.processes().len();

            log::debug!(
                "Health metrics: Memory: {} MB (Virtual: {} MB) | CPU: {:.2}% | Total Processes: {} | Jobs: {} running, {} queued, {} failed (total: {})",
                memory_mb,
                virtual_memory_mb,
                cpu_usage,
                total_processes,
                running_count,
                queued_count,
                failed_count,
                all_jobs.len()
            );
        } else {
            log::debug!(
                "Health metrics: Jobs: {} running, {} queued, {} failed (total: {})",
                running_count,
                queued_count,
                failed_count,
                all_jobs.len()
            );
        }

        Ok(())
    }
}
