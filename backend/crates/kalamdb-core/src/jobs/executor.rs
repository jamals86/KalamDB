//! **DEPRECATED**: This module is deprecated in favor of `UnifiedJobManager` and executor traits.
//!
//! See `jobs::unified_manager` and `jobs::executors` for the new architecture:
//! - `UnifiedJobManager`: Main job orchestration (create_job, run_loop, execute_job)
//! - `JobExecutor` trait: Defines execute() method for concrete executors
//! - Concrete executors: FlushExecutor, CleanupExecutor, RetentionExecutor, etc.
//!
//! The new system provides:
//! - Typed JobIds with prefixes (FL-*, CL-*, RT-*, SE-*, UC-*, CO-*, BK-*, RS-*)
//! - Idempotency enforcement (prevents duplicate jobs)
//! - Retry logic with exponential backoff
//! - Crash recovery (marks Running jobs as Failed on startup)
//!
//! Job execution framework
//!
//! This module provides the infrastructure for executing background jobs with:
//! - Status tracking (running, completed, failed)
//! - Resource usage monitoring (CPU, memory)
//! - Parameter and result serialization
//! - Trace logging for debugging
//! - Node identification for distributed systems

use crate::error::KalamDbError;
use crate::tables::system::JobsTableProvider;
use crate::flush::{FlushPolicy, FlushTriggerMonitor};
use crate::jobs::JobManager;
use crate::catalog::TableName;
use kalamdb_commons::system::Job;
use kalamdb_commons::{JobId, JobStatus, JobType, NamespaceId, NodeId, TableName as CommonsTableName};
use std::sync::Arc;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::RwLock;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use std::time::Duration;

/// Job execution result
#[derive(Debug, Clone)]
pub enum JobResult {
    Success(String),
    Failure(String),
}

/// Configuration for a scheduled table
#[derive(Debug, Clone)]
struct ScheduledTable {
    /// Table name
    table_name: TableName,
}

/// Flush scheduler state
enum SchedulerState {
    /// Scheduler is stopped
    Stopped,
    /// Scheduler is running with background task
    Running(JoinHandle<()>),
}

/// Job executor that manages job lifecycle and metrics
///
/// **DEPRECATED**: Use `UnifiedJobManager` with concrete `JobExecutor` trait implementations instead.
#[deprecated(since = "0.1.0", note = "Use UnifiedJobManager from jobs::unified_manager with FlushExecutor, CleanupExecutor, etc. implementations")]
pub struct JobExecutor {
    jobs_provider: Arc<JobsTableProvider>,
    node_id: NodeId,
    
    // Flush scheduler components
    job_manager: Arc<dyn JobManager>,
    trigger_monitor: Arc<FlushTriggerMonitor>,
    //TODO: We should have one scheduler who checks all tables which is the main job
    scheduled_tables: Arc<RwLock<HashMap<String, ScheduledTable>>>,
    active_flushes: Arc<RwLock<HashMap<String, String>>>, //TODO: Use JobId type as key
    state: Arc<RwLock<SchedulerState>>,
    check_interval: Duration,
    shutdown: Arc<Notify>,
}

impl JobExecutor {
    /// Create a new job executor
    ///
    /// # Arguments
    /// * `jobs_provider` - Provider for system.jobs table
    /// * `node_id` - Unique identifier for this node
    /// * `job_manager` - Job manager for executing flush operations
    /// * `check_interval` - How often to check for flush triggers
    pub fn new(
        jobs_provider: Arc<JobsTableProvider>, 
        node_id: NodeId,
        job_manager: Arc<dyn JobManager>,
        check_interval: Duration,
    ) -> Self {
        Self {
            jobs_provider,
            node_id,
            job_manager,
            trigger_monitor: Arc::new(FlushTriggerMonitor::new()),
            scheduled_tables: Arc::new(RwLock::new(HashMap::new())),
            active_flushes: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(SchedulerState::Stopped)),
            check_interval,
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Execute a job with full lifecycle management
    ///
    /// This method handles:
    /// - T101: Job registration with status='running'
    /// - T102: Job completion recording (status, end_time, result)
    /// - T103: Resource usage tracking (memory, CPU)
    /// - T104: Parameter serialization
    /// - T105: Result and trace recording
    /// - T108: Node ID tracking
    ///
    /// # Arguments
    /// * `job_id` - Unique identifier for the job
    /// * `job_type` - Type of job (e.g., "flush", "compact", "cleanup")
    /// * `table_name` - Optional table name if job is table-specific
    /// * `parameters` - Job parameters as strings
    /// * `job_fn` - The actual job function to execute
    ///
    /// # Returns
    /// Result of job execution
    pub fn execute_job<F>(
        &self,
        job_id: String,   //TODO: Use JobId type directly
        job_type: String, //TODO: Use JobType type directly
        //TODO: Pass NamespaceId from context
        table_name: Option<String>, //TODO: Use TableName type directly
        parameters: Vec<String>,
        job_fn: F,
    ) -> Result<JobResult, KalamDbError>
    where
        F: FnOnce() -> Result<String, String>,
    {
        // Parse job_type string to enum
        // Map string to JobType; default unknown types to Cleanup for flexibility in tests and custom jobs
        let job_type_enum = JobType::from_str(&job_type).unwrap_or(JobType::Cleanup);

        // T101: Register job with status='running'
        let namespace_id = NamespaceId::new("default".to_string()); // TODO: Get from context
        let mut job = Job::new(
            JobId::new(job_id.clone()),
            job_type_enum,
            namespace_id,
            self.node_id.clone(),
        );

        if let Some(tn) = table_name {
            job = job.with_table_name(TableName::new(tn));
        }

        // T104: Store parameters as JSON array
        if !parameters.is_empty() {
            let params_json =
                serde_json::to_string(&parameters).unwrap_or_else(|_| "[]".to_string());
            job = job.with_parameters(params_json);
        }

        // Insert initial job record
        self.jobs_provider.insert_job(job.clone())?;

        // T103: Start resource tracking
        let start_time = Instant::now();
        let initial_memory = Self::get_memory_usage_mb();

        // Execute the job function
        let result = job_fn();

        // T103: Calculate resource usage
        let duration = start_time.elapsed();
        let final_memory = Self::get_memory_usage_mb();
        let memory_delta_mb = (final_memory - initial_memory).max(0.0);
        let memory_used_bytes = (memory_delta_mb * 1024.0 * 1024.0) as i64;

        // Estimate CPU usage based on execution time (simplified)
        // In a real system, this would use OS-specific APIs
        let cpu_used_micros = duration.as_micros() as i64;

        // T105: Build trace information
        let trace = format!(
            "Job executed in {:.2}s on node {}",
            duration.as_secs_f64(),
              self.node_id.as_str()
        );

        // T102: Update job with completion status and metrics
        let final_job = match &result {
            Ok(success_message) => job
                .with_metrics(Some(memory_used_bytes), Some(cpu_used_micros))
                .with_trace(trace)
                .complete(Some(success_message.clone())),
            Err(error_message) => job
                .with_metrics(Some(memory_used_bytes), Some(cpu_used_micros))
                .with_trace(trace)
                .fail(error_message.clone()),
        };

        // T102: Persist final job state
        self.jobs_provider.update_job(final_job)?;

        // Return result
        match result {
            Ok(msg) => Ok(JobResult::Success(msg)),
            Err(msg) => Ok(JobResult::Failure(msg)),
        }
    }

    /// Execute a job asynchronously (returns immediately, job runs in background)
    ///
    /// This is useful for long-running jobs that shouldn't block the caller.
    /// The job status can be monitored via the system.jobs table.
    ///
    /// # Arguments
    /// * `job_id` - Unique identifier for the job
    /// * `job_type` - Type of job
    /// * `table_name` - Optional table name
    /// * `parameters` - Job parameters
    /// * `job_fn` - The job function (must be Send + 'static)
    pub fn execute_async<F>(
        &self,
        job_id: String,   //TODO: Use JobId type directly
        job_type: String, //TODO: Use JobType type directly
        //TODO: Pass NamespaceId from context
        table_name: Option<String>, //TODO: Use TableName type directly
        parameters: Vec<String>,
        job_fn: F,
    ) -> Result<(), KalamDbError>
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        // Parse job_type string to enum
        // Map string to JobType; default unknown types to Cleanup to allow arbitrary labels (e.g., "background")
        let job_type_enum = JobType::from_str(&job_type).unwrap_or(JobType::Cleanup);

        // Register job immediately
        let namespace_id = NamespaceId::new("default".to_string()); // TODO: Get from context
        let mut job = Job::new(
            JobId::new(job_id.clone()),
            job_type_enum,
            namespace_id,
            self.node_id.clone(),
        );

        if let Some(tn) = table_name {
            job = job.with_table_name(TableName::new(tn));
        }

        if !parameters.is_empty() {
            let params_json =
                serde_json::to_string(&parameters).unwrap_or_else(|_| "[]".to_string());
            job = job.with_parameters(params_json);
        }

        self.jobs_provider.insert_job(job)?;

        // Spawn background execution
        let jobs_provider = Arc::clone(&self.jobs_provider);
        let node_id = self.node_id.clone();

        std::thread::spawn(move || {
            let start_time = Instant::now();
            let initial_memory = Self::get_memory_usage_mb();

            let result = job_fn();

            let duration = start_time.elapsed();
            let final_memory = Self::get_memory_usage_mb();
            let memory_delta_mb = (final_memory - initial_memory).max(0.0);
            let memory_used_bytes = (memory_delta_mb * 1024.0 * 1024.0) as i64;
            let cpu_used_micros = duration.as_micros() as i64;

            let trace = format!(
                "Async job executed in {:.2}s on node {}",
                duration.as_secs_f64(),
                node_id
            );

            // Fetch current job state and update
            if let Ok(Some(mut job)) = jobs_provider.get_job(&JobId::new(job_id.clone())) {
                let final_job = match result {
                    Ok(success_message) => {
                        job.memory_used = Some(memory_used_bytes);
                        job.cpu_used = Some(cpu_used_micros);
                        job.trace = Some(trace);
                        job.status = JobStatus::Completed;
                        job.completed_at = Some(chrono::Utc::now().timestamp_millis());
                        job.result = Some(success_message);
                        job
                    }
                    Err(error_message) => {
                        job.memory_used = Some(memory_used_bytes);
                        job.cpu_used = Some(cpu_used_micros);
                        job.trace = Some(trace);
                        job.status = JobStatus::Failed;
                        job.completed_at = Some(chrono::Utc::now().timestamp_millis());
                        job.error_message = Some(error_message);
                        job
                    }
                };

                let _ = jobs_provider.update_job(final_job);
            }
        });

        Ok(())
    }

    // ===== Flush Scheduling Methods =====

    /// Get the trigger monitor for external row count updates
    pub fn trigger_monitor(&self) -> Arc<FlushTriggerMonitor> {
        Arc::clone(&self.trigger_monitor)
    }

    /// Schedule a table for automatic flushing
    pub async fn schedule_table(
        &self,
        table_name: TableName,
        cf_name: String,
        policy: FlushPolicy,
    ) -> Result<(), KalamDbError> {
        // Validate the flush policy
        policy
            .validate()
            .map_err(|e| KalamDbError::Other(format!("Invalid flush policy: {}", e)))?;

        // Check if already scheduled
        {
            let tables = self
                .scheduled_tables
                .read()
                .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;

            if tables.contains_key(&cf_name) {
                return Err(KalamDbError::Other(format!(
                    "Table {} is already scheduled for flushing",
                    cf_name
                )));
            }
        }

        // Register with trigger monitor
        use crate::catalog::TableType;
        self.trigger_monitor.register_table(
            table_name.clone(),
            cf_name.clone(),
            &TableType::User,
            policy.clone(),
        )?;

        // Add to scheduled tables
        let scheduled = ScheduledTable { table_name };

        let mut tables = self
            .scheduled_tables
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        tables.insert(cf_name, scheduled);

        Ok(())
    }

    /// Unschedule a table from automatic flushing
    pub async fn unschedule_table(&self, cf_name: &str) -> Result<(), KalamDbError> {
        // Remove from trigger monitor
        self.trigger_monitor.unregister_table(cf_name)?;

        // Remove from scheduled tables
        let mut tables = self
            .scheduled_tables
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        tables.remove(cf_name);

        Ok(())
    }

    /// Start the flush scheduler
    pub async fn start_flush_scheduler(&self) -> Result<(), KalamDbError> {
        let mut state = self
            .state
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        match &*state {
            SchedulerState::Running(_) => {
                Err(KalamDbError::Other(
                    "Flush scheduler is already running".to_string(),
                ))
            }
            SchedulerState::Stopped => {
                // Start background task
                let handle = self.spawn_scheduler_task();
                *state = SchedulerState::Running(handle);
                Ok(())
            }
        }
    }

    /// Stop the flush scheduler
    pub async fn stop_flush_scheduler(&self) -> Result<(), KalamDbError> {
        // Signal shutdown
        self.shutdown.notify_one();

        // Get the handle and wait for task to complete
        let handle = {
            let mut state = self
                .state
                .write()
                .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

            match std::mem::replace(&mut *state, SchedulerState::Stopped) {
                SchedulerState::Running(handle) => Some(handle),
                SchedulerState::Stopped => None,
            }
        };

        if let Some(handle) = handle {
            // Wait for task to complete
            handle.await.map_err(|e| {
                KalamDbError::Other(format!("Failed to join scheduler task: {}", e))
            })?;
        }

        Ok(())
    }

    /// Spawn the background scheduler task
    fn spawn_scheduler_task(&self) -> JoinHandle<()> {
        let scheduled_tables = Arc::clone(&self.scheduled_tables);
        let active_flushes = Arc::clone(&self.active_flushes);
        let trigger_monitor = Arc::clone(&self.trigger_monitor);
        let job_manager = Arc::clone(&self.job_manager);
        let check_interval = self.check_interval;
        let shutdown = Arc::clone(&self.shutdown);

        tokio::spawn(async move {
            log::info!("Flush scheduler started with check interval {:?}", check_interval);

            loop {
                tokio::select! {
                    _ = tokio::time::sleep(check_interval) => {
                        // Check for flush triggers
                        if let Err(e) = Self::check_and_trigger_flushes(
                            &scheduled_tables,
                            &active_flushes,
                            &trigger_monitor,
                            &job_manager,
                        ).await {
                            log::error!("Error checking flush triggers: {}", e);
                        }
                    }
                    _ = shutdown.notified() => {
                        log::info!("Flush scheduler shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Check for flush triggers and start jobs
    async fn check_and_trigger_flushes(
        scheduled_tables: &Arc<RwLock<HashMap<String, ScheduledTable>>>,
        active_flushes: &Arc<RwLock<HashMap<String, String>>>,
        trigger_monitor: &Arc<FlushTriggerMonitor>,
        job_manager: &Arc<dyn JobManager>,
    ) -> Result<(), KalamDbError> {
        // First, collect all tables that need flushing (without holding locks)
        let tables_to_flush: Vec<(String, TableName)> = {
            let scheduled = scheduled_tables
                .read()
                .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;
            
            let active = active_flushes
                .read()
                .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;

            let mut tables_to_flush = Vec::new();
            
            for (cf_name, scheduled) in scheduled.iter() {
                // Skip if already flushing
                if active.contains_key(cf_name) {
                    continue;
                }

                // Check if flush is needed
                if trigger_monitor.should_flush(cf_name)? {
                    tables_to_flush.push((cf_name.clone(), scheduled.table_name.clone()));
                }
            }
            
            tables_to_flush
        };

        // Now start flush jobs for each table (locks released)
        for (cf_name, table_name) in tables_to_flush {
            // Mark as active
            {
                let mut active = active_flushes
                    .write()
                    .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

                active.insert(cf_name.clone(), format!("flush-{}-{}", cf_name, chrono::Utc::now().timestamp()));
            }

            // Start the job
            let cf_name_clone = cf_name.clone();
            let active_flushes_clone = Arc::clone(active_flushes);
            let trigger_monitor_clone = Arc::clone(trigger_monitor);

            job_manager.start_job(
                format!("flush-{}-{}", cf_name, chrono::Utc::now().timestamp()),
                "flush".to_string(),
                Box::pin(async move {
                    // TODO: Implement actual flush logic
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                    // Reset trigger after successful flush
                    if let Err(e) = trigger_monitor_clone.on_flush_completed(&cf_name_clone) {
                        log::error!("Failed to reset trigger for {}: {}", cf_name_clone, e);
                    }

                    // Remove from active flushes
                    let mut active = active_flushes_clone
                        .write()
                        .map_err(|e| format!("Failed to acquire write lock: {}", e))?;

                    active.remove(&cf_name_clone);

                    Ok(format!("Flushed table {}", cf_name_clone))
                })
            ).await?;
        }

        Ok(())
    }

    /// Get current memory usage in MB
    ///
    /// This is a simplified implementation. In production, use OS-specific APIs:
    /// - Linux: /proc/self/status
    /// - macOS: task_info
    /// - Windows: GetProcessMemoryInfo
    fn get_memory_usage_mb() -> f64 {
        // Placeholder: In real implementation, query OS for process memory
        // For now, return 0.0 to avoid platform-specific dependencies
        0.0
    }

    /// Get the node ID for this executor
    pub fn node_id(&self) -> &str {
           self.node_id.as_str()
    }

    /// Wait for all active flush jobs to complete within the specified timeout
    /// Returns the number of jobs that were waited for
    pub async fn wait_for_active_flush_jobs(&self, timeout: Duration) -> Result<usize, KalamDbError> {
        let start = std::time::Instant::now();
        let mut poll_interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            // Check if timeout exceeded
            if start.elapsed() > timeout {
                return Err(KalamDbError::Other(format!(
                    "Timeout waiting for active flush jobs to complete after {:?}",
                    timeout
                )));
            }

            // Check active flushes
            let active_count = {
                let active_flushes = self.active_flushes.read().map_err(|e| {
                    KalamDbError::Other(format!("Failed to acquire active_flushes lock: {}", e))
                })?;
                active_flushes.len()
            };

            if active_count == 0 {
                log::info!("All active flush jobs completed");
                return Ok(0); // No jobs were actively running when we started waiting
            }

            // Wait a bit before checking again
            poll_interval.tick().await;
        }
    }

    /// Resume any incomplete flush jobs that were running when the system restarted
    /// Returns the number of jobs resumed
    pub async fn resume_incomplete_jobs(&self) -> Result<usize, KalamDbError> {
        // Query all jobs from the jobs provider
        let all_jobs = self.jobs_provider.list_jobs()?;

        // Filter for running flush jobs
        let incomplete_flush_jobs: Vec<_> = all_jobs
            .into_iter()
            .filter(|job| job.status == JobStatus::Running && job.job_type == JobType::Flush)
            .collect();

        if incomplete_flush_jobs.is_empty() {
            log::trace!("No incomplete flush jobs to resume");
            return Ok(0);
        }

        log::info!("Found {} incomplete flush jobs to resume", incomplete_flush_jobs.len());

        let mut resumed = 0;
        for job in incomplete_flush_jobs {
            let job_id = job.job_id.clone();
            let table_name = job.table_name.clone();

            // Create updated job with failed status
            let mut failed_job = job.clone();
            failed_job.status = JobStatus::Failed;
            failed_job.error_message = Some("Job was incomplete when system restarted".to_string());
            failed_job.completed_at = Some(chrono::Utc::now().timestamp_millis());

            // Update job status
            if let Err(e) = self.jobs_provider.update_job(failed_job) {
                log::error!("Failed to mark incomplete job {} as failed: {}", job_id, e);
                continue;
            }

            // Remove from active flushes if present
            if let Some(ref table_name) = table_name {
                if let Ok(mut active_flushes) = self.active_flushes.write() {
                    active_flushes.remove(table_name.as_str());
                }
            }

            log::info!("Marked incomplete flush job {} for table {:?} as failed", job_id, table_name);
            resumed += 1;
        }

        Ok(resumed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_sql::KalamSql;
    use kalamdb_store::RocksDbInit;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup_executor() -> (JobExecutor, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.adapter().backend()));
        let job_manager = Arc::new(crate::jobs::TokioJobManager::new());
        let executor = JobExecutor::new(
            jobs_provider, 
            NodeId::from("test-node-1"),
            job_manager,
            Duration::from_secs(5),
        );
        (executor, temp_dir)
    }

    #[test]
    fn test_successful_job_execution() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-1".to_string(),
            "test".to_string(),
            None,
            vec![],
            || Ok("Job completed successfully".to_string()),
        );

        assert!(result.is_ok());
        match result.unwrap() {
            JobResult::Success(msg) => assert_eq!(msg, "Job completed successfully"),
            JobResult::Failure(_) => panic!("Expected success"),
        }

        // Verify job record in database
        let job = executor
            .jobs_provider
            .get_job(&JobId::new("test-job-1"))
            .unwrap()
            .unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.result, Some("Job completed successfully".to_string()));
    }

    #[test]
    fn test_failed_job_execution() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-2".to_string(),
            "test".to_string(),
            None,
            vec![],
            || Err("Job failed with error".to_string()),
        );

        assert!(result.is_ok());
        match result.unwrap() {
            JobResult::Success(_) => panic!("Expected failure"),
            JobResult::Failure(msg) => assert_eq!(msg, "Job failed with error"),
        }

        // Verify job record in database
        let job = executor
            .jobs_provider
            .get_job(&JobId::new("test-job-2"))
            .unwrap()
            .unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.error_message, Some("Job failed with error".to_string()));
    }

    #[test]
    fn test_job_with_table_name() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-3".to_string(),
            "flush".to_string(),
            Some("test_table".to_string()),
            vec![],
            || Ok("Flushed successfully".to_string()),
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job(&JobId::new("test-job-3"))
            .unwrap()
            .unwrap();
        assert_eq!(
            job.table_name.as_ref().map(|name| name.as_str()),
            Some("test_table")
        );
    }

    #[test]
    fn test_job_with_parameters() {
        let (executor, _temp_dir) = setup_executor();

        let params = vec!["param1".to_string(), "param2".to_string()];

        let result = executor.execute_job(
            "test-job-4".to_string(),
            "backup".to_string(),
            None,
            params.clone(),
            || Ok("Backup completed".to_string()),
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job(&JobId::new("test-job-4"))
            .unwrap()
            .unwrap();
        assert!(job.parameters.is_some());
        let parsed: Vec<String> = serde_json::from_str(&job.parameters.unwrap()).unwrap();
        assert_eq!(parsed, params);
    }

    #[test]
    fn test_job_metrics_recorded() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_job(
            "test-job-5".to_string(),
            "test".to_string(),
            None,
            vec![],
            || {
                // Simulate some work
                std::thread::sleep(std::time::Duration::from_millis(10));
                Ok("Done".to_string())
            },
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job(&JobId::new("test-job-5"))
            .unwrap()
            .unwrap();
        assert!(job.memory_used.is_some());
        assert!(job.cpu_used.is_some());
        assert!(job.trace.is_some());
    }

    #[test]
    fn test_node_id_tracking() {
        let (executor, _temp_dir) = setup_executor();

        assert_eq!(executor.node_id(), "test-node-1");

        let result = executor.execute_job(
            "test-job-6".to_string(),
            "test".to_string(),
            None,
            vec![],
            || Ok("Success".to_string()),
        );

        assert!(result.is_ok());

        let job = executor
            .jobs_provider
            .get_job(&JobId::new("test-job-6"))
            .unwrap()
            .unwrap();
        assert_eq!(job.node_id, NodeId::from("test-node-1"));
    }

    #[test]
    fn test_async_job_execution() {
        let (executor, _temp_dir) = setup_executor();

        let result = executor.execute_async(
            "async-job-1".to_string(),
            "background".to_string(),
            None,
            vec![],
            || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok("Async completed".to_string())
            },
        );

        assert!(result.is_ok());

        // Wait for async job to complete
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Verify job was updated
        let job = executor
            .jobs_provider
            .get_job(&JobId::new("async-job-1"))
            .unwrap()
            .unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(job.result, Some("Async completed".to_string()));
    }
}
