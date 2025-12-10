use super::types::JobsManager;
use crate::error::KalamDbError;
use crate::jobs::executors::JobParams;
use kalamdb_commons::system::{Job, JobOptions};
use kalamdb_commons::{JobId, JobStatus, JobType, NamespaceId};

impl JobsManager {
    /// Insert a job in the database asynchronously
    /// 
    /// Delegates to provider's async method which handles spawn_blocking internally.
    async fn insert_job_async(&self, job: Job) -> Result<(), KalamDbError> {
        self.jobs_provider
            .insert_job_async(job)
            .await
            .map_err(|e| KalamDbError::io_message(format!("Failed to insert job: {}", e)))
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
    pub async fn create_job(
        &self,
        job_type: JobType,
        namespace_id: NamespaceId,
        parameters: serde_json::Value,
        idempotency_key: Option<String>,
        options: Option<JobOptions>,
    ) -> Result<JobId, KalamDbError> {
        // Check idempotency: prevent duplicate jobs with same key
        if let Some(ref key) = idempotency_key {
            if self.has_active_job_with_key(key).await? {
                return Err(KalamDbError::IdempotentConflict(format!(
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
        self.insert_job_async(job.clone()).await?;

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

        // Serialize to JSON for storage and pre-validation
        let params_json = serde_json::to_string(&params).map_err(|e| {
            KalamDbError::Other(format!("Failed to serialize job parameters: {}", e))
        })?;

        // Call executor's pre_validate to check if job should be created
        let app_ctx = self.get_attached_app_context();
        let should_create = self
            .job_registry
            .pre_validate(&app_ctx, &job_type, &params_json)
            .await?;

        if !should_create {
            // Log skip and return a special "skipped" job ID (or error)
            log::trace!(
                "Job pre-validation returned false; skipping job creation for {:?} in namespace {}",
                job_type,
                namespace_id
            );
            return Err(KalamDbError::Other(format!(
                "Job {:?} skipped: pre-validation returned false (nothing to do)",
                job_type
            )));
        }

        let parameters = serde_json::from_str(&params_json).map_err(|e| {
            KalamDbError::Other(format!("Failed to parse job parameters: {}", e))
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
    pub async fn cancel_job(&self, job_id: &JobId) -> Result<(), KalamDbError> {
        // Get job using async method
        let job = self
            .get_job(job_id)
            .await?
            .ok_or_else(|| KalamDbError::NotFound(format!("Job {} not found", job_id)))?;

        // Can only cancel New, Queued, or Running jobs
        if !matches!(
            job.status,
            JobStatus::New | JobStatus::Queued | JobStatus::Running
        ) {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot cancel job in status {:?}",
                job.status
            )));
        }

        // Update status to Cancelled
        let cancelled_job = job.cancel();

        self.jobs_provider
            .update_job_async(cancelled_job)
            .await
            .map_err(|e| KalamDbError::io_message(format!("Failed to cancel job: {}", e)))?;

        // Log cancellation
        self.log_job_event(job_id, "warn", "Job cancelled by user");

        Ok(())
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
    ) -> Result<(), KalamDbError> {
        let mut job = self
            .get_job(job_id)
            .await?
            .ok_or_else(|| KalamDbError::Other(format!("Job {} not found", job_id)))?;

        // Update job to completed status
        let now_ms = chrono::Utc::now().timestamp_millis();
        job.status = JobStatus::Completed;
        job.started_at.get_or_insert(now_ms);
        let success_message = message.unwrap_or_else(|| "Job completed successfully".to_string());
        job.message = Some(success_message.clone());
        job.updated_at = now_ms;
        job.finished_at = Some(now_ms);

        self.jobs_provider
            .update_job_async(job)
            .await
            .map_err(|e| KalamDbError::Other(format!("Failed to complete job: {}", e)))?;

        self.log_job_event(job_id, "info", &success_message);
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
    ) -> Result<(), KalamDbError> {
        let mut job = self
            .get_job(job_id)
            .await?
            .ok_or_else(|| KalamDbError::Other(format!("Job {} not found", job_id)))?;

        // Manually update job to failed state
        let now_ms = chrono::Utc::now().timestamp_millis();
        job.status = JobStatus::Failed;
        job.started_at.get_or_insert(now_ms);
        job.message = Some(error_message.clone());
        job.exception_trace = None;
        job.updated_at = now_ms;
        job.finished_at = Some(now_ms);

        self.jobs_provider
            .update_job_async(job)
            .await
            .map_err(|e| KalamDbError::Other(format!("Failed to mark job as failed: {}", e)))?;

        self.log_job_event(job_id, "error", &format!("Job failed: {}", error_message));
        Ok(())
    }
}
