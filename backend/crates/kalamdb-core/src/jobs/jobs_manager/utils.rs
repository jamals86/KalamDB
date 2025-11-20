use super::types::JobsManager;
use kalamdb_commons::{JobId, JobStatus, JobType};
use kalamdb_commons::system::JobFilter;
use crate::error::KalamDbError;

impl JobsManager {
    /// Generate typed JobId with prefix
    ///
    /// Prefixes:
    /// - FL: Flush jobs
    /// - CL: Cleanup jobs
    /// - RT: Retention jobs
    /// - SE: Stream eviction jobs
    /// - UC: User cleanup jobs
    /// - CO: Compaction jobs (future)
    /// - BK: Backup jobs (future)
    /// - RS: Restore jobs (future)
    pub(crate) fn generate_job_id(&self, job_type: &JobType) -> JobId {
        let prefix = job_type.short_prefix();

        // Generate UUID for uniqueness
        let uuid = uuid::Uuid::new_v4().to_string().replace("-", "");
        JobId::new(&format!("{}-{}", prefix, &uuid[..12]))
    }

    /// Log job event to jobs.log file
    ///
    /// All log lines are prefixed with [JobId] for easy filtering.
    ///
    /// # Arguments
    /// * `job_id` - Job ID for log prefix
    /// * `level` - Log level (info, warn, error)
    /// * `message` - Log message
    pub(crate) fn log_job_event(&self, job_id: &JobId, level: &str, message: &str) {
        // TODO: Implement dedicated jobs.log file appender (T137)
        // For now, use standard logging with [JobId] prefix
        match level {
            "info" => log::info!("[{}] {}", job_id, message),
            "warn" => log::warn!("[{}] {}", job_id, message),
            "error" => log::error!("[{}] {}", job_id, message),
            _ => log::debug!("[{}] {}", job_id, message),
        }
    }

    /// Recover incomplete jobs on startup
    ///
    /// Marks all Running jobs as Failed with "Server restarted" error.
    pub(crate) async fn recover_incomplete_jobs(&self) -> Result<(), KalamDbError> {
        let mut filter = JobFilter::default();
        filter.status = Some(JobStatus::Running);

        let running_jobs = self.list_jobs(filter).await?;

        if running_jobs.is_empty() {
            log::info!("No incomplete jobs to recover");
            return Ok(());
        }

        log::warn!(
            "Recovering {} incomplete jobs from previous run",
            running_jobs.len()
        );

        for mut job in running_jobs {
            // Manually update job to failed state
            let now_ms = chrono::Utc::now().timestamp_millis();
            job.status = JobStatus::Failed;
            job.message = Some("Server restarted".to_string());
            job.exception_trace = Some("Job was running when server shut down".to_string());
            job.updated_at = now_ms;
            job.finished_at = Some(now_ms);

            self.jobs_provider.update_job(job.clone()).map_err(|e| {
                KalamDbError::IoError(format!("Failed to recover job: {}", e))
            })?;

            self.log_job_event(
                &job.job_id,
                "error",
                "Job marked as failed (server restart)",
            );
        }

        Ok(())
    }
}
