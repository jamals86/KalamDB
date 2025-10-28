//! Stream eviction scheduler for automatic TTL cleanup
//!
//! This module provides background task that periodically runs stream table eviction
//! to remove expired events based on TTL settings.

use crate::error::KalamDbError;
use crate::jobs::stream_eviction::StreamEvictionJob;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// Scheduler state
enum SchedulerState {
    /// Scheduler is stopped
    Stopped,
    /// Scheduler is running with background task
    Running(JoinHandle<()>),
}

/// Stream eviction scheduler for automatic TTL cleanup
///
/// Periodically runs eviction jobs to remove expired events from stream tables.
/// Uses the existing StreamEvictionJob for actual eviction logic and job tracking.
pub struct StreamEvictionScheduler {
    /// Eviction job instance
    eviction_job: Arc<StreamEvictionJob>,

    /// Scheduler state
    state: Arc<RwLock<SchedulerState>>,

    /// Check interval for running eviction
    check_interval: Duration,

    /// Shutdown notification
    shutdown: Arc<Notify>,
}

impl StreamEvictionScheduler {
    /// Create a new stream eviction scheduler
    ///
    /// # Arguments
    /// * `eviction_job` - StreamEvictionJob instance
    /// * `check_interval` - How often to run eviction
    pub fn new(eviction_job: Arc<StreamEvictionJob>, check_interval: Duration) -> Self {
        Self {
            eviction_job,
            state: Arc::new(RwLock::new(SchedulerState::Stopped)),
            check_interval,
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Start the scheduler
    ///
    /// Spawns a background task that periodically runs stream table eviction.
    pub async fn start(&self) -> Result<(), KalamDbError> {
        let mut state = self.state.write().unwrap();

        match *state {
            SchedulerState::Running(_) => {
                return Err(KalamDbError::Other(
                    "Stream eviction scheduler already running".to_string(),
                ));
            }
            SchedulerState::Stopped => {
                let eviction_job = Arc::clone(&self.eviction_job);
                let interval = self.check_interval;
                let shutdown = Arc::clone(&self.shutdown);

                let handle = tokio::spawn(async move {
                    let mut interval_timer = tokio::time::interval(interval);

                    loop {
                        tokio::select! {
                            _ = interval_timer.tick() => {
                                // Run eviction for all stream tables
                                match eviction_job.run_eviction() {
                                    Ok(count) if count > 0 => {
                                        log::info!(
                                            "Stream eviction completed: {} events evicted",
                                            count
                                        );
                                    }
                                    Ok(_) => {
                                        log::trace!("Stream eviction completed: no events to evict");
                                    }
                                    Err(e) => {
                                        log::error!("Stream eviction failed: {}", e);
                                    }
                                }
                            }
                            _ = shutdown.notified() => {
                                log::info!("Stream eviction scheduler shutting down");
                                break;
                            }
                        }
                    }
                });

                *state = SchedulerState::Running(handle);
                log::info!(
                    "Stream eviction scheduler started (interval: {:?})",
                    interval
                );
                Ok(())
            }
        }
    }

    /// Stop the scheduler
    ///
    /// Gracefully stops the background eviction task.
    pub async fn stop(&self) -> Result<(), KalamDbError> {
        let mut state = self.state.write().unwrap();

        match std::mem::replace(&mut *state, SchedulerState::Stopped) {
            SchedulerState::Running(handle) => {
                drop(state); // Release lock before notifying

                // Notify shutdown - use notify_one() to wake the waiting task
                self.shutdown.notify_one();

                // Wait for task to complete
                if let Err(e) = handle.await {
                    log::error!("Error waiting for stream eviction scheduler to stop: {}", e);
                }

                log::info!("Stream eviction scheduler stopped");
                Ok(())
            }
            SchedulerState::Stopped => {
                log::warn!("Stream eviction scheduler was not running");
                Ok(())
            }
        }
    }

    /// Check if the scheduler is running
    pub fn is_running(&self) -> bool {
        matches!(
            *self.state.read().unwrap(),
            SchedulerState::Running(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobExecutor, StreamEvictionJob};
    use crate::storage::RocksDbInit;
    use crate::tables::system::JobsTableProvider;
    use kalamdb_sql::KalamSql;
    use kalamdb_store::StreamTableStore;
    use tempfile::TempDir;

    fn setup_scheduler() -> (StreamEvictionScheduler, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();

        let stream_store = Arc::new(StreamTableStore::new(db.clone()).unwrap());
        let backend: Arc<dyn kalamdb_store::storage_trait::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(db.clone()));
        let kalam_sql = Arc::new(KalamSql::new(backend).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(Arc::clone(&kalam_sql)));
        let job_executor = Arc::new(JobExecutor::new(jobs_provider, "test-node".to_string()));

        let eviction_job = Arc::new(StreamEvictionJob::with_defaults(
            stream_store,
            kalam_sql,
            job_executor,
        ));

        let scheduler = StreamEvictionScheduler::new(eviction_job, Duration::from_secs(1));

        (scheduler, temp_dir)
    }

    #[tokio::test]
    async fn test_scheduler_start_stop() {
        let (scheduler, _temp_dir) = setup_scheduler();

        assert!(!scheduler.is_running());

        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());

        scheduler.stop().await.unwrap();
        assert!(!scheduler.is_running());
    }

    #[tokio::test]
    async fn test_scheduler_double_start() {
        let (scheduler, _temp_dir) = setup_scheduler();

        scheduler.start().await.unwrap();
        let result = scheduler.start().await;
        assert!(result.is_err());

        scheduler.stop().await.unwrap();
    }
}
