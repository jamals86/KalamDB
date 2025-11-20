use super::types::JobsManager;
use kalamdb_commons::system::{Job, JobFilter};
use kalamdb_commons::{JobId, JobStatus};
use crate::error::KalamDbError;

impl JobsManager {
    /// Get job details
    ///
    /// # Arguments
    /// * `job_id` - ID of job to retrieve
    ///
    /// # Returns
    /// Job struct if found, None otherwise
    pub async fn get_job(&self, job_id: &JobId) -> Result<Option<Job>, KalamDbError> {
        self.jobs_provider
            .get_job(job_id)
            .map_err(|e| KalamDbError::IoError(format!("Failed to get job: {}", e)))
    }

    /// List jobs matching filter criteria
    ///
    /// # Arguments
    /// * `filter` - Filter criteria (status, job_type, namespace, etc.)
    ///
    /// # Returns
    /// Vector of matching jobs
    pub async fn list_jobs(
        &self,
        filter: JobFilter,
    ) -> Result<Vec<Job>, KalamDbError> {
        self.jobs_provider.list_jobs_filtered(&filter).map_err(|e| {
            KalamDbError::IoError(format!("Failed to list jobs: {}", e))
        })
    }

    /// Check if active job with idempotency key exists
    ///
    /// Active = New, Queued, Running, or Retrying status
    pub(crate) async fn has_active_job_with_key(&self, key: &str) -> Result<bool, KalamDbError> {
        let mut filter = JobFilter::default();
        filter.idempotency_key = Some(key.to_string());

        let jobs = self.list_jobs(filter).await?;
        Ok(jobs.into_iter().any(|job| {
            matches!(
                job.status,
                JobStatus::New | JobStatus::Queued | JobStatus::Running | JobStatus::Retrying
            )
        }))
    }
}
