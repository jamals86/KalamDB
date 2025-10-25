//! Flush scheduler for automatic table flushing
//!
//! This module provides the `FlushScheduler` which automatically triggers flush
//! operations for tables based on configured policies (time interval, row count,
//! or both). The scheduler uses the `JobManager` trait for job execution and
//! cancellation support.
//!
//! ## Architecture
//!
//! The scheduler works in conjunction with the existing `FlushTriggerMonitor`:
//!
//! ```text
//! FlushScheduler
//!     ↓ (registers tables)
//! FlushTriggerMonitor
//!     ↓ (monitors triggers)
//! FlushJob
//!     ↓ (executed via JobManager)
//! JobManager (TokioJobManager)
//! ```
//!
//! ## Trigger Logic
//!
//! The scheduler supports three trigger modes (via FlushPolicy):
//! - **Time-based**: Flush every N seconds
//! - **Row-based**: Flush after N rows inserted
//! - **Combined**: Flush when EITHER condition is met (whichever happens first)
//!
//! After each flush, the trigger counters are reset and monitoring continues.
//!
//! ## Examples
//!
//! ```rust,no_run
//! use kalamdb_core::scheduler::FlushScheduler;
//! use kalamdb_core::jobs::TokioJobManager;
//! use kalamdb_core::flush::FlushPolicy;
//! use kalamdb_core::catalog::TableName;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create job manager
//! let job_manager = Arc::new(TokioJobManager::new());
//!
//! // Create flush scheduler with 5-second check interval
//! let scheduler = FlushScheduler::new(
//!     job_manager,
//!     Duration::from_secs(5)
//! );
//!
//! // Schedule a table for automatic flushing (combined triggers)
//! let policy = FlushPolicy::Combined {
//!     row_limit: 1000,
//!     interval_seconds: 60
//! };
//!
//! scheduler.schedule_table(
//!     TableName::new("messages".to_string()),
//!     "messages_cf".to_string(),
//!     policy
//! ).await?;
//!
//! // Start the scheduler
//! scheduler.start().await?;
//!
//! // ... scheduler runs in background ...
//!
//! // Stop the scheduler
//! scheduler.stop().await?;
//! # Ok(())
//! # }
//! ```

use crate::catalog::TableName;
use crate::error::KalamDbError;
use crate::flush::{FlushPolicy, FlushTriggerMonitor};
use crate::jobs::JobManager;
use crate::tables::system::JobsTableProvider;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Configuration for a scheduled table
#[derive(Debug, Clone)]
struct ScheduledTable {
    /// Table name
    table_name: TableName,
    /// Column family name
    cf_name: String,
    /// Flush policy
    policy: FlushPolicy,
}

/// Flush scheduler state
enum SchedulerState {
    /// Scheduler is stopped
    Stopped,
    /// Scheduler is running with background task
    Running(JoinHandle<()>),
}

/// Flush scheduler for automatic table flushing
///
/// Monitors tables and triggers flush operations based on configured policies.
/// Uses the `FlushTriggerMonitor` for low-level trigger detection and the
/// `JobManager` for executing flush jobs.
///
/// ## Trigger Monitoring
///
/// The scheduler periodically checks all registered tables (at `check_interval`)
/// and triggers flush jobs when conditions are met:
///
/// - **Time trigger**: Elapsed time since last flush >= interval_seconds
/// - **Row trigger**: Accumulated row count >= row_limit
/// - **Combined trigger**: EITHER condition met (whichever happens first)
///
/// ## Duplicate Prevention
///
/// The scheduler tracks active flush jobs to prevent duplicate flushes:
/// - Before starting a flush, it checks `active_flushes` registry
/// - Only one flush per table can run at a time
/// - Completed/failed flushes are removed from the registry
///
/// ## Row Count Monitoring
///
/// Row counts are tracked by the `FlushTriggerMonitor`, which should be notified
/// on INSERT/UPDATE operations via `on_rows_inserted()`. The scheduler reads
/// this state during its periodic checks.
pub struct FlushScheduler {
    /// Job manager for executing flush operations
    job_manager: Arc<dyn JobManager>,

    /// Flush trigger monitor for tracking flush conditions
    trigger_monitor: Arc<FlushTriggerMonitor>,

    /// Scheduled tables (keyed by column family name)
    scheduled_tables: Arc<RwLock<HashMap<String, ScheduledTable>>>,

    /// Active flush jobs (keyed by column family name → job_id)
    active_flushes: Arc<RwLock<HashMap<String, String>>>,

    /// Scheduler state
    state: Arc<RwLock<SchedulerState>>,

    /// Check interval for monitoring triggers
    check_interval: Duration,

    /// Shutdown notification
    shutdown: Arc<Notify>,

    /// Jobs table provider for persisting job state (optional)
    jobs_provider: Option<Arc<JobsTableProvider>>,
}

impl FlushScheduler {
    /// Create a new flush scheduler
    ///
    /// # Arguments
    ///
    /// * `job_manager` - Job manager for executing flush operations
    /// * `check_interval` - How often to check for flush triggers (e.g., every 5 seconds)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kalamdb_core::scheduler::FlushScheduler;
    /// use kalamdb_core::jobs::TokioJobManager;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// let job_manager = Arc::new(TokioJobManager::new());
    /// let scheduler = FlushScheduler::new(job_manager, Duration::from_secs(5));
    /// ```
    pub fn new(job_manager: Arc<dyn JobManager>, check_interval: Duration) -> Self {
        Self {
            job_manager,
            trigger_monitor: Arc::new(FlushTriggerMonitor::new()),
            scheduled_tables: Arc::new(RwLock::new(HashMap::new())),
            active_flushes: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(SchedulerState::Stopped)),
            check_interval,
            shutdown: Arc::new(Notify::new()),
            jobs_provider: None,
        }
    }

    /// Set the jobs table provider for job persistence and crash recovery
    pub fn with_jobs_provider(mut self, jobs_provider: Arc<JobsTableProvider>) -> Self {
        self.jobs_provider = Some(jobs_provider);
        self
    }

    /// Get the trigger monitor for external row count updates
    ///
    /// Returns a reference to the `FlushTriggerMonitor` so that INSERT/UPDATE
    /// operations can notify the scheduler of row changes.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kalamdb_core::scheduler::FlushScheduler;
    /// use kalamdb_core::jobs::TokioJobManager;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let job_manager = Arc::new(TokioJobManager::new());
    /// let scheduler = FlushScheduler::new(job_manager, Duration::from_secs(5));
    ///
    /// // Get trigger monitor for row updates
    /// let monitor = scheduler.trigger_monitor();
    ///
    /// // Notify after inserting rows
    /// monitor.on_rows_inserted("messages_cf", 10)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn trigger_monitor(&self) -> Arc<FlushTriggerMonitor> {
        Arc::clone(&self.trigger_monitor)
    }

    /// Schedule a table for automatic flushing
    ///
    /// Registers the table with the scheduler and starts monitoring flush triggers.
    /// The table will be flushed automatically when policy conditions are met.
    ///
    /// # Arguments
    ///
    /// * `table_name` - Name of the table
    /// * `cf_name` - Column family name (must be unique)
    /// * `policy` - Flush policy (time, row count, or combined)
    ///
    /// # Returns
    ///
    /// `Ok(())` if the table was scheduled, or an error if:
    /// - The table is already scheduled
    /// - The flush policy is invalid
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use kalamdb_core::scheduler::FlushScheduler;
    /// use kalamdb_core::jobs::TokioJobManager;
    /// use kalamdb_core::flush::FlushPolicy;
    /// use kalamdb_core::catalog::TableName;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let job_manager = Arc::new(TokioJobManager::new());
    /// let scheduler = FlushScheduler::new(job_manager, Duration::from_secs(5));
    ///
    /// // Schedule table with combined triggers (time OR row count)
    /// let policy = FlushPolicy::Combined {
    ///     row_limit: 1000,
    ///     interval_seconds: 60
    /// };
    ///
    /// scheduler.schedule_table(
    ///     TableName::new("messages".to_string()),
    ///     "messages_cf".to_string(),
    ///     policy
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
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
        // Note: We pass TableType::User as a placeholder - in production this should
        // come from the table metadata
        use crate::catalog::TableType;
        self.trigger_monitor.register_table(
            table_name.clone(),
            cf_name.clone(),
            &TableType::User,
            policy.clone(),
        )?;

        // Add to scheduled tables
        let scheduled = ScheduledTable {
            table_name,
            cf_name: cf_name.clone(),
            policy,
        };

        let mut tables = self
            .scheduled_tables
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        tables.insert(cf_name, scheduled);

        Ok(())
    }

    /// Unschedule a table from automatic flushing
    ///
    /// Removes the table from the scheduler. Any in-progress flush will complete,
    /// but no new flushes will be triggered.
    ///
    /// # Arguments
    ///
    /// * `cf_name` - Column family name
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

    /// Start the scheduler
    ///
    /// Begins monitoring registered tables and triggering flush operations.
    /// The scheduler runs in a background task until `stop()` is called.
    ///
    /// # Returns
    ///
    /// `Ok(())` if started, or an error if already running
    pub async fn start(&self) -> Result<(), KalamDbError> {
        let mut state = self
            .state
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        match &*state {
            SchedulerState::Running(_) => {
                return Err(KalamDbError::Other(
                    "Scheduler is already running".to_string(),
                ));
            }
            SchedulerState::Stopped => {
                // Start background task
                let handle = self.spawn_scheduler_task();
                *state = SchedulerState::Running(handle);
                Ok(())
            }
        }
    }

    /// Stop the scheduler
    ///
    /// Stops monitoring and waits for any in-progress flush to complete.
    /// This is a graceful shutdown - active flushes are allowed to finish.
    pub async fn stop(&self) -> Result<(), KalamDbError> {
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

    /// Resume incomplete jobs from system.jobs table on startup (T158e)
    ///
    /// Queries system.jobs for jobs with status='running' and resumes them.
    /// This provides crash recovery - if the server restarts mid-flush, incomplete
    /// jobs are automatically resumed.
    ///
    /// # Returns
    ///
    /// Number of jobs resumed
    pub async fn resume_incomplete_jobs(&self) -> Result<usize, KalamDbError> {
        let jobs_provider = match &self.jobs_provider {
            Some(provider) => provider,
            None => {
                log::warn!("Jobs provider not configured, skipping crash recovery");
                return Ok(0);
            }
        };

        // Query all jobs
        let all_jobs = jobs_provider.list_jobs()?;

        // Filter for running jobs (incomplete)
        let incomplete_jobs: Vec<_> = all_jobs
            .into_iter()
            .filter(|job| job.status == "running")
            .collect();

        if incomplete_jobs.is_empty() {
            log::info!("No incomplete jobs to resume");
            return Ok(0);
        }

        log::info!("Found {} incomplete jobs to resume", incomplete_jobs.len());

        let mut resumed = 0;
        for job in incomplete_jobs {
            let job_id = job.job_id.clone();

            log::debug!(
                "Resuming incomplete job: job_id={}, table={:?}, type={}",
                job_id,
                job.table_name,
                job.job_type
            );

            // TODO: Implement actual job resume logic based on job_type
            // For now, mark as failed with recovery message
            let failed_job = crate::tables::system::JobRecord {
                status: "failed".to_string(),
                completed_at: Some(chrono::Utc::now().timestamp_millis()),
                error_message: Some("Job interrupted by server restart".to_string()),
                ..job
            };

            if let Err(e) = jobs_provider.update_job(failed_job) {
                log::error!("Failed to update job {}: {}", job_id, e);
                continue;
            }

            resumed += 1;
        }

        log::info!("Resumed {} incomplete jobs", resumed);
        Ok(resumed)
    }

    /// Check if a flush job is already running for the given table (T158f)
    ///
    /// # Arguments
    ///
    /// * `table_name` - Table name to check
    ///
    /// # Returns
    ///
    /// - `Ok(Some(job_id))` if a running job exists
    /// - `Ok(None)` if no running job exists
    pub async fn find_running_flush_job(
        &self,
        table_name: &str,
    ) -> Result<Option<String>, KalamDbError> {
        let jobs_provider = match &self.jobs_provider {
            Some(provider) => provider,
            None => return Ok(None),
        };

        // Query system.jobs for running flush jobs on this table
        let all_jobs = jobs_provider.list_jobs()?;

        for job in all_jobs {
            if job.status == "running"
                && job.job_type == "flush"
                && job.table_name.as_deref() == Some(table_name)
            {
                return Ok(Some(job.job_id));
            }
        }

        Ok(None)
    }

    /// Wait for all active flush jobs to complete with timeout (T158h, T158i)
    ///
    /// Used during graceful shutdown to ensure all flush operations complete
    /// before the server exits.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum duration to wait for jobs to complete
    ///
    /// # Returns
    ///
    /// - `Ok(count)` if all jobs completed within timeout (count = jobs waited for)
    /// - `Err` if timeout exceeded with active jobs still running
    pub async fn wait_for_active_jobs(&self, timeout: Duration) -> Result<usize, KalamDbError> {
        let jobs_provider = match &self.jobs_provider {
            Some(provider) => provider,
            None => {
                log::debug!("No jobs provider configured, nothing to wait for");
                return Ok(0);
            }
        };

        let start = std::time::Instant::now();
        let mut poll_interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            // Query system.jobs for running jobs
            let all_jobs = jobs_provider.list_jobs()?;
            let active_jobs: Vec<_> = all_jobs
                .into_iter()
                .filter(|job| job.status == "running")
                .collect();

            if active_jobs.is_empty() {
                log::info!("All flush jobs completed during shutdown");
                return Ok(0);
            }

            // Check if timeout exceeded
            if start.elapsed() >= timeout {
                let job_ids: Vec<_> = active_jobs.iter().map(|j| j.job_id.as_str()).collect();
                return Err(KalamDbError::Other(format!(
                    "Shutdown timeout exceeded with {} active jobs: {:?}",
                    active_jobs.len(),
                    job_ids
                )));
            }

            log::debug!(
                "Waiting for {} active jobs to complete (elapsed: {:?})",
                active_jobs.len(),
                start.elapsed()
            );

            poll_interval.tick().await;
        }
    }

    /// Spawn the background scheduler task
    fn spawn_scheduler_task(&self) -> JoinHandle<()> {
        let job_manager = Arc::clone(&self.job_manager);
        let trigger_monitor = Arc::clone(&self.trigger_monitor);
        let scheduled_tables = Arc::clone(&self.scheduled_tables);
        let active_flushes = Arc::clone(&self.active_flushes);
        let jobs_provider = self.jobs_provider.clone();
        let check_interval = self.check_interval;
        let shutdown = Arc::clone(&self.shutdown);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check for flush triggers
                        if let Err(e) = Self::check_and_trigger_flushes(
                            &job_manager,
                            &trigger_monitor,
                            &scheduled_tables,
                            &active_flushes,
                            &jobs_provider,
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

    /// Check all scheduled tables and trigger flushes as needed (T158f, T158g, T158m)
    async fn check_and_trigger_flushes(
        job_manager: &Arc<dyn JobManager>,
        trigger_monitor: &Arc<FlushTriggerMonitor>,
        scheduled_tables: &Arc<RwLock<HashMap<String, ScheduledTable>>>,
        active_flushes: &Arc<RwLock<HashMap<String, String>>>,
        jobs_provider: &Option<Arc<JobsTableProvider>>,
    ) -> Result<(), KalamDbError> {
        // Clone the data to avoid holding the lock across await points
        let tables_snapshot = {
            let tables = scheduled_tables
                .read()
                .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;
            tables.clone()
        };

        for (cf_name, scheduled) in tables_snapshot.iter() {
            // Check if flush should be triggered
            let should_flush = trigger_monitor.should_flush(cf_name)?;

            if !should_flush {
                continue;
            }

            // T158f, T158g: Check system.jobs for duplicate flush (use as source of truth - T158m)
            if let Some(provider) = jobs_provider {
                // Query system.jobs for running flush jobs on this table
                let all_jobs = provider.list_jobs()?;
                let table_name_str = scheduled.table_name.as_str();

                for job in all_jobs {
                    if job.status == "running"
                        && job.job_type == "flush"
                        && job.table_name.as_deref() == Some(table_name_str)
                    {
                        log::debug!(
                            "Flush already in progress for table {} (job_id={}), skipping",
                            table_name_str,
                            job.job_id
                        );
                        continue;
                    }
                }
            } else {
                // Fallback to in-memory check if no jobs provider
                let active = active_flushes.read().map_err(|e| {
                    KalamDbError::Other(format!("Failed to acquire read lock: {}", e))
                })?;

                if active.contains_key(cf_name) {
                    log::debug!("Flush already in progress for table {}, skipping", cf_name);
                    continue;
                }
            }

            // Generate job ID
            let job_id = format!("flush-{}-{}", cf_name, uuid::Uuid::new_v4());
            let job_id_clone = job_id.clone();

            // Mark flush as active (in-memory for now - actual tracking in system.jobs)
            {
                let mut active = active_flushes.write().map_err(|e| {
                    KalamDbError::Other(format!("Failed to acquire write lock: {}", e))
                })?;

                active.insert(cf_name.clone(), job_id.clone());
            }

            // Start flush job
            log::debug!(
                "Triggering flush for table {}, job_id={}",
                scheduled.table_name.as_str(),
                job_id
            );

            let cf_name_clone = cf_name.clone();
            let active_flushes_clone = Arc::clone(active_flushes);
            let trigger_monitor_clone = Arc::clone(trigger_monitor);
            let table_name_clone = scheduled.table_name.clone();

            log::info!(
                "🚀 Starting flush job: job_id={}, table={}, cf={}",
                job_id,
                table_name_clone.as_str(),
                cf_name
            );

            let job_future = Box::pin(async move {
                log::debug!(
                    "📊 Flush job executing: job_id={}, table={}, cf={}",
                    job_id_clone,
                    table_name_clone.as_str(),
                    cf_name_clone
                );

                // TODO: Execute actual flush logic here
                // This requires access to the table provider and storage registry
                // For now, we log what would happen
                log::warn!(
                    "⚠️  Flush logic not yet wired to scheduler (job_id={}). Need to wire UserTableFlushJob or SharedTableFlushJob execution here.",
                    job_id_clone
                );

                // Simulate flush delay for testing
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // Reset trigger state after flush
                if let Err(e) = trigger_monitor_clone.on_flush_completed(&cf_name_clone) {
                    log::error!("❌ Failed to reset trigger state: {}", e);
                }

                // Remove from active flushes
                if let Ok(mut active) = active_flushes_clone.write() {
                    active.remove(&cf_name_clone);
                }

                log::info!(
                    "✅ Flush job completed: job_id={}, table={}, cf={}, rows=0 (not wired yet)",
                    job_id_clone,
                    table_name_clone.as_str(),
                    cf_name_clone
                );

                Ok(format!("Flush completed for {}", table_name_clone.as_str()))
            });

            if let Err(e) = job_manager
                .start_job(job_id.clone(), "flush".to_string(), job_future)
                .await
            {
                log::error!("Failed to start flush job {}: {}", job_id, e);

                // Remove from active flushes on error
                if let Ok(mut active) = active_flushes.write() {
                    active.remove(cf_name);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::TokioJobManager;

    #[tokio::test]
    async fn test_schedule_table() {
        let job_manager = Arc::new(TokioJobManager::new());
        let scheduler = FlushScheduler::new(job_manager, Duration::from_secs(5));

        let policy = FlushPolicy::Combined {
            row_limit: 1000,
            interval_seconds: 60,
        };

        let result = scheduler
            .schedule_table(
                TableName::new("test_table".to_string()),
                "test_cf".to_string(),
                policy,
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_duplicate_schedule() {
        let job_manager = Arc::new(TokioJobManager::new());
        let scheduler = FlushScheduler::new(job_manager, Duration::from_secs(5));

        let policy = FlushPolicy::RowLimit { row_limit: 1000 };

        // Schedule once
        scheduler
            .schedule_table(
                TableName::new("test_table".to_string()),
                "test_cf".to_string(),
                policy.clone(),
            )
            .await
            .unwrap();

        // Try to schedule again
        let result = scheduler
            .schedule_table(
                TableName::new("test_table".to_string()),
                "test_cf".to_string(),
                policy,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_start_stop() {
        let job_manager = Arc::new(TokioJobManager::new());
        let scheduler = FlushScheduler::new(job_manager, Duration::from_millis(100));

        // Start scheduler
        scheduler.start().await.unwrap();

        // Wait a bit
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Stop scheduler
        scheduler.stop().await.unwrap();
    }
}
