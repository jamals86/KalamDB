//! Job cleanup task for system.jobs table
//!
//! This module provides functionality to clean up old job records from the
//! system.jobs table based on a retention period. Jobs that have completed,
//! failed, or been cancelled are eligible for cleanup after the retention
//! period expires.
//!
//! ## Retention Policy (T158p, T158s)
//!
//! - **Running jobs**: Never deleted (always kept)
//! - **Completed/Failed/Cancelled**: Deleted after retention period (default 30 days)
//! - **Reference time**: Uses `end_time` if available, otherwise `start_time`
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kalamdb_core::jobs::JobCleanupTask;
//! use kalamdb_core::tables::system::JobsTableProvider;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! # async fn example(jobs_provider: Arc<JobsTableProvider>) -> Result<(), Box<dyn std::error::Error>> {
//! // Create cleanup task with 30-day retention
//! let cleanup = JobCleanupTask::new(jobs_provider, Duration::from_secs(30 * 24 * 60 * 60));
//!
//! // Run cleanup once
//! let deleted = cleanup.run_cleanup().await?;
//! println!("Deleted {} old job records", deleted);
//!
//! // Or start scheduled cleanup task
//! let handle = cleanup.start_scheduled(Duration::from_secs(24 * 60 * 60)).await;
//! # Ok(())
//! # }
//! ```

use crate::error::KalamDbError;
use crate::tables::system::JobsTableProvider;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

/// Job cleanup task
pub struct JobCleanupTask {
    /// Jobs table provider
    jobs_provider: Arc<JobsTableProvider>,
    /// Retention period (jobs older than this are deleted)
    retention_period: Duration,
}

impl JobCleanupTask {
    /// Create a new job cleanup task
    ///
    /// # Arguments
    ///
    /// * `jobs_provider` - Provider for system.jobs table
    /// * `retention_period` - How long to keep completed/failed/cancelled jobs
    pub fn new(jobs_provider: Arc<JobsTableProvider>, retention_period: Duration) -> Self {
        Self {
            jobs_provider,
            retention_period,
        }
    }

    /// Run cleanup once (T158p, T158s)
    ///
    /// Deletes job records where:
    /// - Status is NOT 'running'
    /// - end_time (or start_time if end_time is None) is older than retention_period
    ///
    /// # Returns
    ///
    /// Number of job records deleted
    pub async fn run_cleanup(&self) -> Result<usize, KalamDbError> {
        let retention_days = self.retention_period.as_secs() / (24 * 60 * 60);
        
        log::debug!("Running job cleanup with {} day retention", retention_days);
        
        let deleted = self.jobs_provider.cleanup_old_jobs(retention_days as i64)?;
        
        log::info!("Job cleanup completed: deleted {} old records", deleted);
        
        Ok(deleted)
    }

    /// Start scheduled cleanup task
    ///
    /// Runs cleanup at the specified interval in a background task.
    ///
    /// # Arguments
    ///
    /// * `interval` - How often to run cleanup (e.g., every 24 hours)
    ///
    /// # Returns
    ///
    /// JoinHandle for the background task
    pub async fn start_scheduled(self, interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            
            loop {
                ticker.tick().await;
                
                match self.run_cleanup().await {
                    Ok(deleted) => {
                        log::info!("Scheduled cleanup deleted {} jobs", deleted);
                    }
                    Err(e) => {
                        log::error!("Scheduled cleanup failed: {}", e);
                    }
                }
            }
        })
    }

    /// Parse cron schedule and calculate next run time
    ///
    /// Helper for converting cron expressions (e.g., "0 0 * * *") to Duration.
    /// For now, this is a simplified implementation that only handles daily schedules.
    ///
    /// # Arguments
    ///
    /// * `schedule` - Cron expression (e.g., "0 0 * * *" for daily at midnight)
    ///
    /// # Returns
    ///
    /// Duration until next run (defaults to 24 hours for now)
    pub fn parse_cron_schedule(_schedule: &str) -> Duration {
        // TODO: Implement proper cron parsing
        // For now, default to 24 hours
        Duration::from_secs(24 * 60 * 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_schedule_parsing() {
        // Test daily schedule
        let duration = JobCleanupTask::parse_cron_schedule("0 0 * * *");
        assert_eq!(duration.as_secs(), 24 * 60 * 60);
    }
}
