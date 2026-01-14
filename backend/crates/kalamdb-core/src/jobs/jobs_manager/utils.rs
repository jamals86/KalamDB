use super::types::JobsManager;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use kalamdb_commons::system::JobFilter;
use kalamdb_commons::{JobId, JobStatus, JobType};
use log::Level;

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
        JobId::new(format!("{}-{}", prefix, &uuid[..12]))
    }

    /// Log job event to jobs.log file
    ///
    /// All log lines are prefixed with [JobId] for easy filtering.
    ///
    /// # Arguments
    /// * `job_id` - Job ID for log prefix
    /// * `level` - Log level (info, warn, error)
    /// * `message` - Log message
    pub(crate) fn log_job_event(&self, job_id: &JobId, level: &Level, message: &str) {
        // TODO: Implement dedicated jobs.log file appender (T137)
        // For now, use standard logging with [JobId] prefix
        match level {
            Level::Error => log::error!("[{}] {}", job_id.as_str(), message),
            Level::Warn => log::warn!("[{}] {}", job_id.as_str(), message),
            Level::Info => log::info!("[{}] {}", job_id.as_str(), message),
            Level::Debug => log::debug!("[{}] {}", job_id.as_str(), message),
            Level::Trace => log::trace!("[{}] {}", job_id.as_str(), message),
        }
    }

    /// Recover incomplete jobs on startup
    ///
    /// Marks all Running jobs as Failed with "Server restarted" error.
    /// Uses provider's async methods which handle spawn_blocking internally.
    pub(crate) async fn recover_incomplete_jobs(&self) -> Result<(), KalamDbError> {
        let filter = JobFilter {
            status: Some(JobStatus::Running),
            ..Default::default()
        };

        let running_jobs = self.list_jobs(filter).await?;

        if running_jobs.is_empty() {
            log::debug!("No incomplete jobs to recover");
            return Ok(());
        }

        log::warn!(
            "Recovering {} incomplete jobs from previous run",
            running_jobs.len()
        );

        let now_ms = chrono::Utc::now().timestamp_millis();

        // Update each job using provider's async method
        for mut job in running_jobs {
            let job_id = job.job_id.clone();
            job.status = JobStatus::Failed;
            job.message = Some("Server restarted".to_string());
            job.exception_trace = Some("Job was running when server shut down".to_string());
            job.updated_at = now_ms;
            job.finished_at = Some(now_ms);

            self.jobs_provider
                .update_job_async(job)
                .await
                .into_kalamdb_error("Failed to recover job")?;

            self.log_job_event(&job_id, &Level::Error, "Job marked as failed (server restart)");
        }

        Ok(())
    }
}
