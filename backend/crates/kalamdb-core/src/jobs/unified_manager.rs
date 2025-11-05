//! Unified Job Management System
//!
//! **Phase 9 (US6)**: Single JobManager with typed JobIds, richer statuses, idempotency, retry/backoff, dedicated logging
//!
//! This module provides a centralized job management system with:
//! - Typed JobIds with prefixes (FL, CL, RT, SE, UC, CO, BK, RS)
//! - Rich job status tracking (New, Queued, Running, Completed, Failed, Retrying, Cancelled)
//! - Idempotency enforcement (prevent duplicate jobs)
//! - Automatic retry with exponential backoff
//! - Dedicated jobs.log file for job-specific logging
//! - Crash recovery (mark incomplete jobs as failed on restart)
//!
//! ## Architecture
//!
//! ```text
//! JobManager
//! ├── JobsTableProvider    (persistence via system.jobs table)
//! ├── JobRegistry         (dispatches to JobExecutor implementations)
//! └── jobs.log            (dedicated log file with [JobId] prefix)
//! ```
//!
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_core::jobs::JobManager;
//! use kalamdb_commons::{JobType, NamespaceId};
//!
//! // Create job manager
//! let job_manager = JobManager::new(jobs_provider, job_registry);
//!
//! // Create a flush job
//! let job_id = job_manager.create_job(
//!     JobType::Flush,
//!     NamespaceId::new("default"),
//!     serde_json::json!({"table_name": "users"}),
//!     Some("flush-users-hourly".to_string()), // idempotency key
//!     None, // use default options
//! ).await?;
//!
//! // Run job processing loop
//! job_manager.run_loop(5).await?; // max 5 concurrent jobs
//! ```

use crate::app_context::AppContext;
use crate::jobs::executors::{JobContext, JobDecision, JobExecutor, JobRegistry};
use crate::tables::system::JobsTableProvider;
use kalamdb_commons::system::{Job, JobFilter, JobOptions};
use kalamdb_commons::{JobId, JobStatus, JobType, NamespaceId, NodeId};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

/// Unified Job Manager
///
/// Provides centralized job creation, execution, tracking, and lifecycle management.
pub struct JobManager {
    /// System table provider for job persistence
    jobs_provider: Arc<JobsTableProvider>,
    
    /// Registry of job executors (trait-based dispatch)
    job_registry: Arc<JobRegistry>,
    
    /// Node ID for this instance
    node_id: NodeId,
    
    /// Flag for graceful shutdown
    shutdown: Arc<RwLock<bool>>,
}

impl JobManager {
    /// Create a new JobManager
    ///
    /// # Arguments
    /// * `jobs_provider` - System table provider for job persistence
    /// * `job_registry` - Registry of job executors
    pub fn new(jobs_provider: Arc<JobsTableProvider>, job_registry: Arc<JobRegistry>) -> Self {
        Self {
            jobs_provider,
            job_registry,
            node_id: NodeId::new("node_default"), // TODO: Get from config
            shutdown: Arc::new(RwLock::new(false)),
        }
    }

    /// Create a new job
    ///
    /// # Arguments
    /// * `job_type` - Type of job to create
    /// * `namespace_id` - Namespace for the job
    /// * `parameters` - Job parameters as JSON value
    /// * `idempotency_key` - Optional key to prevent duplicate jobs
    /// * `options` - Optional job creation options (retry, priority, queue)
    ///
    /// # Returns
    /// JobId for the created job
    ///
    /// # Errors
    /// - `IdempotentConflict` if a job with the same idempotency key is already running
    /// - `IoError` if job insertion fails
    ///
    /// # Example
    /// ```rust
    /// let job_id = job_manager.create_job(
    ///     JobType::Flush,
    ///     NamespaceId::new("default"),
    ///     serde_json::json!({"table_name": "users"}),
    ///     Some("flush-users-hourly".to_string()),
    ///     None,
    /// ).await?;
    /// ```
    pub async fn create_job(
        &self,
        job_type: JobType,
        namespace_id: NamespaceId,
        parameters: serde_json::Value,
        idempotency_key: Option<String>,
        options: Option<JobOptions>,
    ) -> Result<JobId, crate::error::KalamDbError> {
        // Check idempotency: prevent duplicate jobs with same key
        if let Some(ref key) = idempotency_key {
            if self.has_active_job_with_key(key).await? {
                return Err(crate::error::KalamDbError::IdempotentConflict(format!(
                    "Job with idempotency key '{}' is already running or queued",
                    key
                )));
            }
        }

        // Generate job ID with type-specific prefix
        let job_id = self.generate_job_id(&job_type);

        // Create job with New status
        let mut job = Job::new(job_id.clone(), job_type, namespace_id, self.node_id.clone());
        job.parameters = Some(parameters.to_string());
        job.idempotency_key = idempotency_key;

        // Apply options if provided
        if let Some(opts) = options {
            job.max_retries = opts.max_retries.unwrap_or(3);
            job.queue = opts.queue;
            job.priority = opts.priority;
        }

        // Persist job
        self.jobs_provider
            .insert_job(&job)
            .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to create job: {}", e)))?;

        // Log job creation
        self.log_job_event(&job_id, "info", &format!("Job created: type={:?}", job_type));

        Ok(job_id)
    }

    /// Cancel a running or queued job
    ///
    /// # Arguments
    /// * `job_id` - ID of job to cancel
    ///
    /// # Errors
    /// Returns error if job not found or cancellation fails
    pub async fn cancel_job(&self, job_id: &JobId) -> Result<(), crate::error::KalamDbError> {
        // Get job
        let job = self
            .jobs_provider
            .get_job(job_id)
            .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to get job: {}", e)))?
            .ok_or_else(|| crate::error::KalamDbError::NotFound(format!("Job {} not found", job_id)))?;

        // Can only cancel New, Queued, or Running jobs
        if !matches!(
            job.status,
            JobStatus::New | JobStatus::Queued | JobStatus::Running
        ) {
            return Err(crate::error::KalamDbError::InvalidOperation(format!(
                "Cannot cancel job in status {:?}",
                job.status
            )));
        }

        // Update status to Cancelled
        let cancelled_job = job.cancel();

        self.jobs_provider
            .update_job(&cancelled_job)
            .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to cancel job: {}", e)))?;

        // Log cancellation
        self.log_job_event(job_id, "warn", "Job cancelled by user");

        Ok(())
    }

    /// Get job details
    ///
    /// # Arguments
    /// * `job_id` - ID of job to retrieve
    ///
    /// # Returns
    /// Job struct if found, None otherwise
    pub async fn get_job(&self, job_id: &JobId) -> Result<Option<Job>, crate::error::KalamDbError> {
        self.jobs_provider
            .get_job(job_id)
            .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to get job: {}", e)))
    }

    /// List jobs matching filter criteria
    ///
    /// # Arguments
    /// * `filter` - Filter criteria (status, job_type, namespace, etc.)
    ///
    /// # Returns
    /// Vector of matching jobs
    pub async fn list_jobs(&self, filter: JobFilter) -> Result<Vec<Job>, crate::error::KalamDbError> {
        self.jobs_provider
            .list_jobs(filter)
            .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to list jobs: {}", e)))
    }

    /// Complete a job with success message (Phase 9, T165)
    ///
    /// # Arguments
    /// * `job_id` - ID of job to complete
    /// * `message` - Success message
    ///
    /// # Returns
    /// Ok if job completed successfully
    pub async fn complete_job(&self, job_id: &JobId, message: Option<String>) -> Result<(), crate::error::KalamDbError> {
        let job = self.get_job(job_id).await?
            .ok_or_else(|| crate::error::KalamDbError::Other(format!("Job {} not found", job_id)))?;
        
        let completed_job = job.complete(message);
        
        self.jobs_provider
            .update_job(&completed_job)
            .map_err(|e| crate::error::KalamDbError::Other(format!("Failed to complete job: {}", e)))?;
        
        self.log_job_event(job_id, "info", &format!("Job completed successfully"));
        Ok(())
    }

    /// Fail a job with error message (Phase 9, T165)
    ///
    /// # Arguments
    /// * `job_id` - ID of job to fail
    /// * `error_message` - Error message describing failure
    ///
    /// # Returns
    /// Ok if job marked as failed successfully
    pub async fn fail_job(&self, job_id: &JobId, error_message: String) -> Result<(), crate::error::KalamDbError> {
        let job = self.get_job(job_id).await?
            .ok_or_else(|| crate::error::KalamDbError::Other(format!("Job {} not found", job_id)))?;
        
        let failed_job = job.fail(error_message.clone(), None);
        
        self.jobs_provider
            .update_job(&failed_job)
            .map_err(|e| crate::error::KalamDbError::Other(format!("Failed to mark job as failed: {}", e)))?;
        
        self.log_job_event(job_id, "error", &format!("Job failed: {}", error_message));
        Ok(())
    }

    /// Run job processing loop
    ///
    /// Continuously polls for queued jobs and executes them using registered executors.
    /// Implements idempotency checks, retry logic with exponential backoff, and crash recovery.
    ///
    /// # Arguments
    /// * `max_concurrent` - Maximum number of concurrent jobs to run
    ///
    /// # Example
    /// ```rust
    /// // Run job loop with max 5 concurrent jobs
    /// job_manager.run_loop(5).await?;
    /// ```
    pub async fn run_loop(&self, max_concurrent: usize) -> Result<(), crate::error::KalamDbError> {
        log::info!("Starting job processing loop (max {} concurrent)", max_concurrent);

        // Perform crash recovery on startup
        self.recover_incomplete_jobs().await?;

        loop {
            // Check for shutdown signal
            if *self.shutdown.read().await {
                log::info!("Shutdown signal received, stopping job loop");
                break;
            }

            // Poll for next job
            if let Some(job) = self.poll_next().await? {
                // TODO: Implement concurrency control (semaphore with max_concurrent)
                // For now, process jobs sequentially
                self.execute_job(job).await?;
            } else {
                // No jobs available, sleep briefly
                sleep(Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    /// Poll for next job to execute
    ///
    /// Finds the next queued job with idempotency check.
    ///
    /// # Returns
    /// Next job to execute, or None if no jobs available
    async fn poll_next(&self) -> Result<Option<Job>, crate::error::KalamDbError> {
        // Query for jobs with status=Queued or New, ordered by priority and created_at
        let filter = JobFilter {
            status: Some(vec![JobStatus::New, JobStatus::Queued]),
            ..Default::default()
        };

        let jobs = self.list_jobs(filter).await?;

        // Return first job (already sorted by priority/created_at)
        Ok(jobs.into_iter().next())
    }

    /// Execute a single job
    ///
    /// Dispatches job to appropriate executor via JobRegistry.
    async fn execute_job(&self, mut job: Job) -> Result<(), crate::error::KalamDbError> {
        let job_id = job.job_id.clone();

        // Mark job as Running
        job.status = JobStatus::Running;
        job.started_at = Some(chrono::Utc::now().timestamp_millis());
        job.updated_at = chrono::Utc::now().timestamp_millis();

        self.jobs_provider
            .update_job(&job)
            .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to start job: {}", e)))?;

        self.log_job_event(&job_id, "info", "Job started");

        // Get executor for job type
        let executor = self
            .job_registry
            .get(&job.job_type)
            .ok_or_else(|| crate::error::KalamDbError::Other(format!("No executor found for job type {:?}", job.job_type)))?;

        // Create job context
        let app_ctx = AppContext::get();
        let ctx = JobContext::new(app_ctx, &job_id);

        // Execute job
        let decision = executor.execute(&ctx, &job).await;

        // Handle execution result
        match decision {
            JobDecision::Completed { message } => {
                let completed_job = job.complete(message.clone());
                self.jobs_provider
                    .update_job(&completed_job)
                    .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to complete job: {}", e)))?;

                self.log_job_event(&job_id, "info", &format!("Job completed: {}", message.unwrap_or_default()));
            }
            JobDecision::Retry {
                backoff_ms,
                exception_trace,
            } => {
                // Check if can retry
                if job.retry_count < job.max_retries {
                    job.retry_count += 1;
                    job.status = JobStatus::Retrying;
                    job.exception_trace = Some(exception_trace.clone());
                    job.updated_at = chrono::Utc::now().timestamp_millis();

                    self.jobs_provider
                        .update_job(&job)
                        .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to retry job: {}", e)))?;

                    self.log_job_event(
                        &job_id,
                        "warn",
                        &format!("Job retry {}/{}, waiting {}ms", job.retry_count, job.max_retries, backoff_ms),
                    );

                    // Sleep before retry
                    sleep(Duration::from_millis(backoff_ms as u64)).await;
                } else {
                    // Max retries exceeded, mark as failed
                    let failed_job = job.fail("Max retries exceeded".to_string(), Some(exception_trace));
                    self.jobs_provider
                        .update_job(&failed_job)
                        .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to fail job: {}", e)))?;

                    self.log_job_event(&job_id, "error", "Job failed: max retries exceeded");
                }
            }
            JobDecision::Failed { exception_trace } => {
                let failed_job = job.fail("Job execution failed".to_string(), Some(exception_trace.clone()));
                self.jobs_provider
                    .update_job(&failed_job)
                    .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to fail job: {}", e)))?;

                self.log_job_event(&job_id, "error", &format!("Job failed: {}", exception_trace));
            }
        }

        Ok(())
    }

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
    fn generate_job_id(&self, job_type: &JobType) -> JobId {
        let prefix = match job_type {
            JobType::Flush => "FL",
            JobType::Cleanup => "CL",
            JobType::Retention => "RT",
            JobType::StreamEviction => "SE",
            JobType::UserCleanup => "UC",
            JobType::Compact => "CO",
            JobType::Backup => "BK",
            JobType::Restore => "RS",
        };

        // Generate UUID for uniqueness
        let uuid = uuid::Uuid::new_v4().to_string().replace("-", "");
        JobId::new(&format!("{}-{}", prefix, &uuid[..12]))
    }

    /// Check if active job with idempotency key exists
    ///
    /// Active = New, Queued, Running, or Retrying status
    async fn has_active_job_with_key(&self, key: &str) -> Result<bool, crate::error::KalamDbError> {
        let filter = JobFilter {
            idempotency_key: Some(key.to_string()),
            status: Some(vec![
                JobStatus::New,
                JobStatus::Queued,
                JobStatus::Running,
                JobStatus::Retrying,
            ]),
            ..Default::default()
        };

        let jobs = self.list_jobs(filter).await?;
        Ok(!jobs.is_empty())
    }

    /// Recover incomplete jobs on startup
    ///
    /// Marks all Running jobs as Failed with "Server restarted" error.
    async fn recover_incomplete_jobs(&self) -> Result<(), crate::error::KalamDbError> {
        let filter = JobFilter {
            status: Some(vec![JobStatus::Running]),
            ..Default::default()
        };

        let running_jobs = self.list_jobs(filter).await?;

        if running_jobs.is_empty() {
            log::info!("No incomplete jobs to recover");
            return Ok(());
        }

        log::warn!("Recovering {} incomplete jobs from previous run", running_jobs.len());

        for job in running_jobs {
            let failed_job = job.fail(
                "Server restarted".to_string(),
                Some("Job was running when server shut down".to_string()),
            );

            self.jobs_provider
                .update_job(&failed_job)
                .map_err(|e| crate::error::KalamDbError::IoError(format!("Failed to recover job: {}", e)))?;

            self.log_job_event(&failed_job.job_id, "error", "Job marked as failed (server restart)");
        }

        Ok(())
    }

    /// Log job event to jobs.log file
    ///
    /// All log lines are prefixed with [JobId] for easy filtering.
    ///
    /// # Arguments
    /// * `job_id` - Job ID for log prefix
    /// * `level` - Log level (info, warn, error)
    /// * `message` - Log message
    fn log_job_event(&self, job_id: &JobId, level: &str, message: &str) {
        // TODO: Implement dedicated jobs.log file appender (T137)
        // For now, use standard logging with [JobId] prefix
        match level {
            "info" => log::info!("[{}] {}", job_id, message),
            "warn" => log::warn!("[{}] {}", job_id, message),
            "error" => log::error!("[{}] {}", job_id, message),
            _ => log::debug!("[{}] {}", job_id, message),
        }
    }

    /// Request graceful shutdown
    pub async fn shutdown(&self) {
        log::info!("Initiating job manager shutdown");
        *self.shutdown.write().await = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::JobType;

    #[test]
    fn test_generate_job_id_prefixes() {
        let jobs_provider = Arc::new(crate::test_helpers::create_test_jobs_provider());
        let job_registry = Arc::new(JobRegistry::new());
        let manager = JobManager::new(jobs_provider, job_registry);

        // Test all job type prefixes
        assert!(manager.generate_job_id(&JobType::Flush).as_str().starts_with("FL-"));
        assert!(manager.generate_job_id(&JobType::Cleanup).as_str().starts_with("CL-"));
        assert!(manager.generate_job_id(&JobType::Retention).as_str().starts_with("RT-"));
        assert!(manager.generate_job_id(&JobType::StreamEviction).as_str().starts_with("SE-"));
        assert!(manager.generate_job_id(&JobType::UserCleanup).as_str().starts_with("UC-"));
        assert!(manager.generate_job_id(&JobType::Compact).as_str().starts_with("CO-"));
        assert!(manager.generate_job_id(&JobType::Backup).as_str().starts_with("BK-"));
        assert!(manager.generate_job_id(&JobType::Restore).as_str().starts_with("RS-"));
    }
}
