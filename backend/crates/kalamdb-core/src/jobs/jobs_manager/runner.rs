use super::types::JobsManager;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::jobs::executors::JobDecision;
use crate::jobs::{HealthMonitor, StreamEvictionScheduler};
use crate::jobs::leader_guard::LeaderOnlyJobGuard;
use kalamdb_commons::system::{Job, JobFilter, JobSortField, SortOrder};
use kalamdb_commons::JobStatus;
use kalamdb_raft::GroupId;
use tokio::time::{sleep, Duration, Instant};

impl JobsManager {
    /// Update a job in the database asynchronously
    /// 
    /// Delegates to provider's async method which handles spawn_blocking internally.
    async fn update_job_async(&self, job: Job) -> Result<(), KalamDbError> {
        self.jobs_provider
            .update_job_async(job)
            .await
            .into_kalamdb_error("Failed to update job")
    }

    /// Check if this node should process jobs (leader check for cluster mode)
    ///
    /// In standalone mode, always returns true.
    /// In cluster mode, only the leader of the Meta group processes jobs.
    async fn should_process_jobs(&self) -> bool {
        let app_ctx = self.get_attached_app_context();
        
        // Check if we're in cluster mode
        if !app_ctx.executor().is_cluster_mode() {
            // Standalone mode - always process jobs
            return true;
        }
        
        // Cluster mode - only leader processes jobs
        app_ctx.executor().is_leader(GroupId::Meta).await
    }

    /// Run job processing loop
    ///
    /// Continuously polls for queued jobs and executes them using registered executors.
    /// Implements idempotency checks, retry logic with exponential backoff, and crash recovery.
    /// Also periodically checks for stream tables requiring TTL eviction.
    ///
    /// **Leader-Only Execution**: In cluster mode, only the leader node executes jobs.
    /// Follower nodes will skip job processing and wait for leadership changes.
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
        log::debug!(
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
        
        // Leadership check interval (for cluster mode)
        let leadership_check_interval = Duration::from_secs(1);
        let mut last_leadership_check = Instant::now();
        let mut was_leader = false;

        loop {
            // Check for shutdown signal (lock-free atomic check)
            if self.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                log::info!("Shutdown signal received, stopping job loop");
                break;
            }

            // Check leadership status (for cluster mode)
            let is_leader = if last_leadership_check.elapsed() >= leadership_check_interval {
                last_leadership_check = Instant::now();
                let should_process = self.should_process_jobs().await;
                
                // Log leadership transitions
                if should_process && !was_leader {
                    log::info!("[JobLoop] This node became leader - starting job processing");
                    // Perform leader failover recovery
                    self.handle_leader_failover().await;
                } else if !should_process && was_leader {
                    log::info!("[JobLoop] This node lost leadership - pausing job processing");
                }
                
                was_leader = should_process;
                should_process
            } else {
                was_leader
            };

            // Skip job processing if not leader in cluster mode
            if !is_leader {
                sleep(Duration::from_millis(500)).await;
                continue;
            }

            // Periodic health metrics logging
            if last_health_check.elapsed() >= health_check_interval {
                let app_ctx = self.get_attached_app_context();
                if let Err(e) = HealthMonitor::log_metrics(self, app_ctx).await {
                    log::warn!("Failed to log health metrics: {}", e);
                }
                last_health_check = Instant::now();
            }

            // Periodic stream eviction job creation (leader-only)
            if eviction_interval_secs > 0
                && last_stream_eviction.elapsed() >= stream_eviction_interval
            {
                let app_ctx = self.get_attached_app_context();
                if let Err(e) = StreamEvictionScheduler::check_and_schedule(&app_ctx, self).await {
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
        let filter = JobFilter {
            statuses: Some(vec![JobStatus::New, JobStatus::Queued]),
            sort_by: Some(JobSortField::CreatedAt),
            sort_order: Some(SortOrder::Asc),
            limit: Some(1),
            ..Default::default()
        };

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

        self.update_job_async(job.clone()).await?;

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

                self.update_job_async(job.clone()).await.map_err(|err| {
                    KalamDbError::io_message(format!(
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
        log::debug!("[{}] Handling execution result", job_id);
        match decision {
            JobDecision::Completed { message } => {
                log::debug!("[{}] Matched JobDecision::Completed", job_id);
                // Manually update job to completed state
                let now_ms = chrono::Utc::now().timestamp_millis();
                job.status = JobStatus::Completed;
                job.message = message.clone();
                job.updated_at = now_ms;
                job.finished_at = Some(now_ms);

                log::debug!("[{}] About to call update_job_async with status={:?}", job_id, job.status);
                if let Err(e) = self.update_job_async(job.clone()).await {
                    log::error!("[{}] Failed to update job status to Completed: {}", job_id, e);
                    return Err(KalamDbError::Other(format!(
                        "Failed to update job status to Completed: {}",
                        e
                    )));
                }
                log::debug!("[{}] update_job_async returned Ok", job_id);

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

                    self.update_job_async(job.clone()).await
                        .into_kalamdb_error("Failed to retry job")?;

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

                    self.update_job_async(job.clone()).await
                        .into_kalamdb_error("Failed to fail job")?;

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

                self.update_job_async(job.clone()).await
                    .into_kalamdb_error("Failed to fail job")?;

                self.log_job_event(&job_id, "error", &format!("Job failed: {}", message));
            }
        }

        Ok(())
    }

    /// Handle leader failover by recovering orphaned jobs
    ///
    /// Called when this node becomes the leader in cluster mode.
    async fn handle_leader_failover(&self) {
        use std::collections::HashSet;
        use crate::jobs::leader_failover::LeaderFailoverHandler;
        
        let app_ctx = self.get_attached_app_context();
        
        // Get active nodes from cluster info
        let cluster_info = app_ctx.executor().get_cluster_info();
        let active_nodes: HashSet<_> = cluster_info
            .nodes
            .iter()
            .filter(|n| matches!(n.status, kalamdb_raft::NodeStatus::Active))
            .map(|n| n.node_id.clone())
            .collect();
        
        // Create leader guard
        let job_guard = LeaderOnlyJobGuard::new(app_ctx.executor().clone());
        
        // Create failover handler
        let failover_handler = LeaderFailoverHandler::new(
            job_guard,
            self.jobs_provider.clone(),
            active_nodes,
        );
        
        // Perform failover recovery
        match failover_handler.on_become_leader().await {
            Ok(report) => {
                if !report.is_empty() {
                    log::info!(
                        "[JobLoop] Failover recovery: {} requeued, {} failed, {} continued",
                        report.requeued.len(),
                        report.marked_failed.len(),
                        report.continued.len()
                    );
                }
            }
            Err(e) => {
                log::error!("[JobLoop] Failover recovery failed: {}", e);
            }
        }
    }
}
