use super::types::JobsManager;
use crate::jobs::{HealthMonitor, StreamEvictionScheduler};
use crate::jobs::executors::JobDecision;
use kalamdb_commons::system::{Job, JobFilter, JobSortField, SortOrder};
use kalamdb_commons::JobStatus;
use crate::error::KalamDbError;
use tokio::time::{sleep, Duration, Instant};

impl JobsManager {
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
    pub async fn run_loop(&self, max_concurrent: usize) -> Result<(), KalamDbError> {
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
            match self.poll_next().await {
                Ok(Some(job)) => {
                    // TODO: Implement concurrency control (semaphore with max_concurrent)
                    // For now, process jobs sequentially
                    if let Err(e) = self.execute_job(job).await {
                        log::error!("Job execution failed critically: {}", e);
                        // Sleep briefly to avoid tight loop on persistent errors
                        sleep(Duration::from_secs(1)).await;
                    }
                }
                Ok(None) => {
                    // No jobs available, sleep briefly
                    sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    log::error!("Failed to poll for next job: {}", e);
                    sleep(Duration::from_secs(1)).await;
                }
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
    async fn poll_next(&self) -> Result<Option<Job>, KalamDbError> {
        // Query for jobs with status=Queued or New, ordered by created_at ASC
        // Use limit=1 to fetch only the next job efficiently
        let mut filter = JobFilter::default();
        filter.statuses = Some(vec![JobStatus::New, JobStatus::Queued]);
        filter.sort_by = Some(JobSortField::CreatedAt);
        filter.sort_order = Some(SortOrder::Asc);
        filter.limit = Some(1);
        
        let jobs = self.list_jobs(filter).await?;

        Ok(jobs.into_iter().next())
    }

    /// Execute a single job
    ///
    /// Dispatches job to appropriate executor via JobRegistry.
    async fn execute_job(&self, mut job: Job) -> Result<(), KalamDbError> {
        let job_id = job.job_id.clone();

        // Mark job as Running
        job.status = JobStatus::Running;
        job.started_at = Some(chrono::Utc::now().timestamp_millis());
        job.updated_at = chrono::Utc::now().timestamp_millis();

        self.jobs_provider.update_job(job.clone()).map_err(|e| {
            KalamDbError::IoError(format!("Failed to start job: {}", e))
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
                    KalamDbError::IoError(format!(
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
                    KalamDbError::IoError(format!("Failed to complete job: {}", e))
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
                        KalamDbError::IoError(format!("Failed to retry job: {}", e))
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
                        KalamDbError::IoError(format!("Failed to fail job: {}", e))
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
                    KalamDbError::IoError(format!("Failed to fail job: {}", e))
                })?;

                self.log_job_event(&job_id, "error", &format!("Job failed: {}", message));
            }
        }

        Ok(())
    }
}
