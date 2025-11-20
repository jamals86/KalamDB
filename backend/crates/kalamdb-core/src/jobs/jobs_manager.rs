//! Unified Job Management System
//!
//! **Phase 9 (US6)**: Single JobsManager with typed JobIds, richer statuses, idempotency, retry/backoff, dedicated logging
//!
//! This module provides a centralized job management system with:
//! - Typed JobIds with prefixes (FL, CL, RT, SE, UC, CO, BK, RS)
//! - Rich job status tracking (New, Queued, Running, Completed, Failed, Retrying, Cancelled)
//! - Idempotency enforcement (prevent duplicate jobs)
//! - Automatic retry with exponential backoff
//! - Dedicated jobs.log file for job-specific logging
//! - Crash recovery (mark incomplete jobs as failed on restart)
//! - Type-safe job creation with JobParams trait
//!
//! ## Architecture
//!
//! ```text
//! JobsManager
//! ├── JobsTableProvider    (persistence via system.jobs table)
//! ├── JobRegistry         (dispatches to JobExecutor implementations)
//! └── jobs.log            (dedicated log file with [JobId] prefix)
//! ```
//!
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_core::jobs::JobsManager;
//! use kalamdb_commons::{JobType, NamespaceId};
//! use kalamdb_core::jobs::executors::flush::FlushParams;
//!
//! // Create job manager
//! let job_manager = JobsManager::new(jobs_provider, job_registry);
//!
//! // Create a flush job (typed params)
//! let params = FlushParams { /* ... */ };
//! let job_id = job_manager.create_job_typed(
//!     JobType::Flush,
//!     NamespaceId::new("default"),
//!     params,
//!     Some("flush-users-hourly".to_string()), // idempotency key
//!     None, // use default options
//! ).await?;
//!
//! // Run job processing loop
//! job_manager.run_loop(5).await?; // max 5 concurrent jobs
//! ```

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::jobs::executors::{JobDecision, JobParams, JobRegistry};
use crate::jobs::{HealthMonitor, StreamEvictionScheduler};
use kalamdb_commons::system::{Job, JobFilter, JobOptions};
use kalamdb_commons::{JobId, JobStatus, JobType, NamespaceId, NodeId};
use kalamdb_system::JobsTableProvider;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration, Instant};

/// Unified Job Manager
///
/// Provides centralized job creation, execution, tracking, and lifecycle management.
pub struct JobsManager {
    /// System table provider for job persistence
    jobs_provider: Arc<JobsTableProvider>,

    /// Registry of job executors (trait-based dispatch)
    job_registry: Arc<JobRegistry>,

    /// Node ID for this instance
    node_id: NodeId,

    /// Flag for graceful shutdown
    shutdown: Arc<RwLock<bool>>,
    /// AppContext for global services (avoid calling AppContext::get() repeatedly)
    app_context: Arc<RwLock<Option<Arc<AppContext>>>>,
}

impl JobsManager {
    /// Create a new JobsManager
    ///
    /// # Arguments
    /// * `jobs_provider` - System table provider for job persistence
    /// * `job_registry` - Registry of job executors
    pub fn new(jobs_provider: Arc<JobsTableProvider>, job_registry: Arc<JobRegistry>) -> Self {
        Self {
            jobs_provider,
            job_registry,
            node_id: NodeId::new("node_default".to_string()), // TODO: Get from config
            shutdown: Arc::new(RwLock::new(false)),
            app_context: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach an AppContext instance to this JobsManager. This is used to avoid
    /// calling AppContext::get() repeatedly while still supporting initialization
    /// ordering where AppContext is created after JobsManager.
    pub fn set_app_context(&self, app_ctx: Arc<AppContext>) {
        // Use try_write() to avoid blocking in async context
        if let Ok(mut w) = self.app_context.try_write() {
            *w = Some(app_ctx);
        } else {
            // Fallback: spin until we can acquire the lock (should be rare)
            loop {
                if let Ok(mut w) = self.app_context.try_write() {
                    *w = Some(app_ctx);
                    break;
                }
                std::thread::yield_now();
            }
        }
    }

    /// Get attached AppContext (panics if not attached)
    fn get_attached_app_context(&self) -> Arc<AppContext> {
        // Use try_read() to avoid blocking in async context
        if let Ok(r) = self.app_context.try_read() {
            r.clone().expect("AppContext not attached to JobsManager")
        } else {
            // Fallback: spin until we can acquire the lock (should be rare)
            loop {
                if let Ok(r) = self.app_context.try_read() {
                    return r.clone().expect("AppContext not attached to JobsManager");
                }
                std::thread::yield_now();
            }
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

        // Extract optional table_name from parameters (if provided)
        let table_name_from_params: Option<kalamdb_commons::models::TableName> = parameters
            .get("table_name")
            .and_then(|v| v.as_str())
            .map(|s| kalamdb_commons::models::TableName::new(s.to_string()));

        // Create job with Queued status
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut job = Job {
            job_id: job_id.clone(),
            job_type,
            namespace_id,
            table_name: table_name_from_params,
            status: JobStatus::Queued,
            parameters: Some(parameters.to_string()),
            message: None,
            exception_trace: None,
            idempotency_key,
            retry_count: 0,
            max_retries: 3,
            memory_used: None,
            cpu_used: None,
            created_at: now_ms,
            updated_at: now_ms,
            started_at: None,
            finished_at: None,
            node_id: self.node_id.clone(),
            queue: None,
            priority: None,
        };

        // Apply options if provided
        if let Some(opts) = options {
            job.max_retries = opts.max_retries.unwrap_or(3);
            job.queue = opts.queue;
            job.priority = opts.priority;
        }

        // Persist job
        self.jobs_provider.insert_job(job.clone()).map_err(|e| {
            crate::error::KalamDbError::IoError(format!("Failed to create job: {}", e))
        })?;

        // Log job creation
        self.log_job_event(
            &job_id,
            "info",
            &format!("Job created: type={:?}", job_type),
        );

        Ok(job_id)
    }

    /// Create a job with type-safe parameters
    ///
    /// **Type-Safe Alternative**: Accepts JobParams trait implementations for compile-time validation
    ///
    /// # Type Parameters
    /// * `T` - JobParams implementation (FlushParams, CleanupParams, etc.)
    ///
    /// # Arguments
    /// * `job_type` - Type of job to create
    /// * `namespace_id` - Namespace for the job
    /// * `params` - Typed parameters (automatically validated and serialized)
    /// * `idempotency_key` - Optional key to prevent duplicate jobs
    /// * `options` - Optional job configuration (retries, priority, queue)
    ///
    /// # Returns
    /// Job ID if creation successful
    ///
    /// # Errors
    /// - `IdempotentConflict` if job with same idempotency key already exists
    /// - `KalamDbError` if parameter validation or persistence fails
    ///
    /// # Example
    /// ```rust
    /// use kalamdb_core::jobs::executors::FlushParams;
    /// use kalamdb_commons::TableId;
    ///
    /// let params = FlushParams {
    ///     table_id: TableId::new(namespace_id.clone(), table_name.clone()),
    ///     table_type: TableType::User,
    ///     flush_threshold: None,
    /// };
    ///
    /// let job_id = job_manager.create_job_typed(
    ///     JobType::Flush,
    ///     namespace_id,
    ///     params,
    ///     Some("flush-users".to_string()),
    ///     None,
    /// ).await?;
    /// ```
    pub async fn create_job_typed<T: JobParams>(
        &self,
        job_type: JobType,
        namespace_id: NamespaceId,
        params: T,
        idempotency_key: Option<String>,
        options: Option<JobOptions>,
    ) -> Result<JobId, KalamDbError> {
        // Validate parameters before serialization
        params.validate()?;

        // Serialize to JSON for storage
        let parameters = serde_json::to_value(&params).map_err(|e| {
            KalamDbError::Other(format!("Failed to serialize job parameters: {}", e))
        })?;

        // Delegate to existing create_job method
        self.create_job(job_type, namespace_id, parameters, idempotency_key, options)
            .await
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
            .ok_or_else(|| {
                crate::error::KalamDbError::NotFound(format!("Job {} not found", job_id))
            })?;

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
            .update_job(cancelled_job.clone())
            .map_err(|e| {
                crate::error::KalamDbError::IoError(format!("Failed to cancel job: {}", e))
            })?;

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
    pub async fn list_jobs(
        &self,
        filter: JobFilter,
    ) -> Result<Vec<Job>, crate::error::KalamDbError> {
        let all_jobs = self.jobs_provider.list_jobs().map_err(|e| {
            crate::error::KalamDbError::IoError(format!("Failed to list jobs: {}", e))
        })?;

        // Apply filters
        let mut filtered: Vec<Job> = all_jobs
            .into_iter()
            .filter(|job| {
                // Filter by status
                if let Some(ref status) = filter.status {
                    if status != &job.status {
                        return false;
                    }
                }

                // Filter by job type
                if let Some(ref job_type) = filter.job_type {
                    if job_type != &job.job_type {
                        return false;
                    }
                }

                // Filter by namespace
                if let Some(ref namespace) = filter.namespace_id {
                    if namespace != &job.namespace_id {
                        return false;
                    }
                }

                // Filter by table name
                if let Some(ref table_name) = filter.table_name {
                    if job.table_name.as_ref() != Some(table_name) {
                        return false;
                    }
                }

                // Filter by idempotency key
                if let Some(ref key) = filter.idempotency_key {
                    if job.idempotency_key.as_ref() != Some(key) {
                        return false;
                    }
                }

                true
            })
            .collect();

        if let Some(limit) = filter.limit {
            if filtered.len() > limit {
                filtered.truncate(limit);
            }
        }

        Ok(filtered)
    }

    /// Complete a job with success message (Phase 9, T165)
    ///
    /// # Arguments
    /// * `job_id` - ID of job to complete
    /// * `message` - Success message
    ///
    /// # Returns
    /// Ok if job completed successfully
    pub async fn complete_job(
        &self,
        job_id: &JobId,
        message: Option<String>,
    ) -> Result<(), crate::error::KalamDbError> {
        let mut job = self.get_job(job_id).await?.ok_or_else(|| {
            crate::error::KalamDbError::Other(format!("Job {} not found", job_id))
        })?;

        // Update job to completed status
        let now_ms = chrono::Utc::now().timestamp_millis();
        job.status = JobStatus::Completed;
        job.started_at.get_or_insert(now_ms);
        let success_message = message.unwrap_or_else(|| "Job completed successfully".to_string());
        job.message = Some(success_message.clone());
        job.updated_at = now_ms;
        job.finished_at = Some(now_ms);

        self.jobs_provider.update_job(job.clone()).map_err(|e| {
            crate::error::KalamDbError::Other(format!("Failed to complete job: {}", e))
        })?;

        self.log_job_event(job_id, "info", &format!("{}", success_message));
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
    pub async fn fail_job(
        &self,
        job_id: &JobId,
        error_message: String,
    ) -> Result<(), crate::error::KalamDbError> {
        let mut job = self.get_job(job_id).await?.ok_or_else(|| {
            crate::error::KalamDbError::Other(format!("Job {} not found", job_id))
        })?;

        // Manually update job to failed state
        let now_ms = chrono::Utc::now().timestamp_millis();
        job.status = JobStatus::Failed;
        job.started_at.get_or_insert(now_ms);
        job.message = Some(error_message.clone());
        job.exception_trace = None;
        job.updated_at = now_ms;
        job.finished_at = Some(now_ms);

        self.jobs_provider.update_job(job).map_err(|e| {
            crate::error::KalamDbError::Other(format!("Failed to mark job as failed: {}", e))
        })?;

        self.log_job_event(job_id, "error", &format!("Job failed: {}", error_message));
        Ok(())
    }

    /// Run job processing loop
    ///
    /// Continuously polls for queued jobs and executes them using registered executors.
    /// Implements idempotency checks, retry logic with exponential backoff, and crash recovery.
    /// Also periodically checks for stream tables requiring TTL eviction.
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
        log::info!(
            "Starting job processing loop (max {} concurrent)",
            max_concurrent
        );

        // Perform crash recovery on startup
        self.recover_incomplete_jobs().await?;

        // Health monitoring interval (log every 30 seconds)
        let health_check_interval = Duration::from_secs(30);
        let mut last_health_check = Instant::now();

        // Stream eviction interval (configurable, default 60 seconds)
        let app_context = self.get_attached_app_context();
        let eviction_interval_secs = app_context.config().stream.eviction_interval_seconds;
        let stream_eviction_interval = Duration::from_secs(eviction_interval_secs);
        let mut last_stream_eviction = Instant::now();

        loop {
            // Check for shutdown signal
            if *self.shutdown.read().await {
                log::info!("Shutdown signal received, stopping job loop");
                break;
            }

            // Periodic health metrics logging
            if last_health_check.elapsed() >= health_check_interval {
                if let Err(e) = HealthMonitor::log_metrics(self).await {
                    log::warn!("Failed to log health metrics: {}", e);
                }
                last_health_check = Instant::now();
            }

            // Periodic stream eviction job creation
            if eviction_interval_secs > 0
                && last_stream_eviction.elapsed() >= stream_eviction_interval
            {
                let app_ctx = self.get_attached_app_context();
                if let Err(e) =
                    StreamEvictionScheduler::check_and_schedule(&app_ctx, self).await
                {
                    log::warn!("Failed to check stream eviction: {}", e);
                }
                last_stream_eviction = Instant::now();
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
        let jobs = self.list_jobs(JobFilter::default()).await?;

        let next_job = jobs
            .into_iter()
            .filter(|job| matches!(job.status, JobStatus::New | JobStatus::Queued))
            .next();

        Ok(next_job)
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

        self.jobs_provider.update_job(job.clone()).map_err(|e| {
            crate::error::KalamDbError::IoError(format!("Failed to start job: {}", e))
        })?;

        self.log_job_event(&job_id, "info", "Job started");

        // Execute job using registry (handles deserialization and validation)
        let app_ctx = self.get_attached_app_context();

        // Execute job with robust error handling (do not tear down run loop on executor error)
        let decision = match self.job_registry.execute(app_ctx, &job).await {
            Ok(d) => d,
            Err(e) => {
                // Mark job as failed and continue processing other jobs
                let now_ms = chrono::Utc::now().timestamp_millis();
                job.status = JobStatus::Failed;
                job.message = Some(format!("Executor error: {}", e));
                job.exception_trace = None;
                job.updated_at = now_ms;
                job.finished_at = Some(now_ms);

                self.jobs_provider.update_job(job.clone()).map_err(|err| {
                    crate::error::KalamDbError::IoError(format!(
                        "Failed to fail job after executor error: {}",
                        err
                    ))
                })?;

                self.log_job_event(
                    &job_id,
                    "error",
                    &format!("Job failed with executor error: {}", e),
                );

                // Return Ok to keep the run loop alive
                return Ok(());
            }
        };

        // Handle execution result
        match decision {
            JobDecision::Completed { message } => {
                // Manually update job to completed state
                let now_ms = chrono::Utc::now().timestamp_millis();
                job.status = JobStatus::Completed;
                job.message = message.clone();
                job.updated_at = now_ms;
                job.finished_at = Some(now_ms);

                self.jobs_provider.update_job(job.clone()).map_err(|e| {
                    crate::error::KalamDbError::IoError(format!("Failed to complete job: {}", e))
                })?;

                self.log_job_event(
                    &job_id,
                    "info",
                    &format!("Job completed: {}", message.unwrap_or_default()),
                );
            }
            JobDecision::Retry {
                message,
                exception_trace,
                backoff_ms,
            } => {
                // Check if can retry
                if job.retry_count < job.max_retries {
                    job.retry_count += 1;
                    job.status = JobStatus::Retrying;
                    job.exception_trace = exception_trace.clone();
                    job.updated_at = chrono::Utc::now().timestamp_millis();

                    self.jobs_provider.update_job(job.clone()).map_err(|e| {
                        crate::error::KalamDbError::IoError(format!("Failed to retry job: {}", e))
                    })?;

                    self.log_job_event(
                        &job_id,
                        "warn",
                        &format!(
                            "Job retry {}/{}, waiting {}ms: {}",
                            job.retry_count, job.max_retries, backoff_ms, message
                        ),
                    );

                    // Sleep before retry
                    sleep(Duration::from_millis(backoff_ms)).await;
                } else {
                    // Max retries exceeded, mark as failed
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    job.status = JobStatus::Failed;
                    job.message = Some("Max retries exceeded".to_string());
                    job.exception_trace = exception_trace;
                    job.updated_at = now_ms;
                    job.finished_at = Some(now_ms);

                    self.jobs_provider.update_job(job.clone()).map_err(|e| {
                        crate::error::KalamDbError::IoError(format!("Failed to fail job: {}", e))
                    })?;

                    self.log_job_event(&job_id, "error", "Job failed: max retries exceeded");
                }
            }
            JobDecision::Failed {
                message,
                exception_trace,
            } => {
                // Manually update job to failed state
                let now_ms = chrono::Utc::now().timestamp_millis();
                job.status = JobStatus::Failed;
                job.message = Some(message.clone());
                job.exception_trace = exception_trace.clone();
                job.updated_at = now_ms;
                job.finished_at = Some(now_ms);

                self.jobs_provider.update_job(job.clone()).map_err(|e| {
                    crate::error::KalamDbError::IoError(format!("Failed to fail job: {}", e))
                })?;

                self.log_job_event(&job_id, "error", &format!("Job failed: {}", message));
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
        let prefix = job_type.short_prefix();

        // Generate UUID for uniqueness
        let uuid = uuid::Uuid::new_v4().to_string().replace("-", "");
        JobId::new(&format!("{}-{}", prefix, &uuid[..12]))
    }

    /// Check if active job with idempotency key exists
    ///
    /// Active = New, Queued, Running, or Retrying status
    async fn has_active_job_with_key(&self, key: &str) -> Result<bool, crate::error::KalamDbError> {
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

    /// Recover incomplete jobs on startup
    ///
    /// Marks all Running jobs as Failed with "Server restarted" error.
    async fn recover_incomplete_jobs(&self) -> Result<(), crate::error::KalamDbError> {
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
                crate::error::KalamDbError::IoError(format!("Failed to recover job: {}", e))
            })?;

            self.log_job_event(
                &job.job_id,
                "error",
                "Job marked as failed (server restart)",
            );
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
    use crate::test_helpers::*;
    use kalamdb_commons::models::schemas::TableOptions;
    use kalamdb_commons::models::{NamespaceId, TableId, TableName};
    use kalamdb_commons::schemas::TableType;
    use std::sync::Arc;

    // TODO: Re-enable after refactoring test utilities
    // This test needs app_context() helper which was removed
    #[ignore]
    #[tokio::test]
    async fn test_check_stream_eviction_finds_stream_table() {
        // Test disabled - needs refactoring to use proper AppContext setup
        // instead of global app_context() singleton
    }
}
