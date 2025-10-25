//! Job retention policy
//!
//! This module handles automatic cleanup of old job records from the system.jobs table.
//! Retention policies help prevent unbounded growth of job history while preserving
//! recent records for debugging and monitoring.

use crate::error::KalamDbError;
use crate::tables::system::JobsTableProvider;
use std::sync::Arc;

/// Configuration for job retention
#[derive(Debug, Clone)]
pub struct RetentionConfig {
    /// Number of days to retain completed jobs
    pub retention_days: i64,
    /// Whether to retain failed jobs longer than successful ones
    pub extended_failure_retention: bool,
    /// Number of days to retain failed jobs if extended retention is enabled
    pub failure_retention_days: i64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            retention_days: 30,
            extended_failure_retention: true,
            failure_retention_days: 90,
        }
    }
}

/// Job retention policy enforcer
pub struct RetentionPolicy {
    jobs_provider: Arc<JobsTableProvider>,
    config: RetentionConfig,
}

impl RetentionPolicy {
    /// Create a new retention policy enforcer
    pub fn new(jobs_provider: Arc<JobsTableProvider>, config: RetentionConfig) -> Self {
        Self {
            jobs_provider,
            config,
        }
    }

    /// Create with default configuration (30 days for success, 90 for failures)
    pub fn with_defaults(jobs_provider: Arc<JobsTableProvider>) -> Self {
        Self::new(jobs_provider, RetentionConfig::default())
    }

    /// Execute retention policy cleanup
    ///
    /// This removes jobs older than the configured retention period.
    /// Failed jobs may have extended retention if configured.
    ///
    /// # Returns
    /// Number of jobs deleted
    pub fn enforce(&self) -> Result<usize, KalamDbError> {
        if self.config.extended_failure_retention {
            self.enforce_with_extended_failures()
        } else {
            self.jobs_provider
                .cleanup_old_jobs(self.config.retention_days)
        }
    }

    /// Enforce retention with extended failure retention
    fn enforce_with_extended_failures(&self) -> Result<usize, KalamDbError> {
        let now = chrono::Utc::now().timestamp_millis();
        let retention_ms = self.config.retention_days * 24 * 60 * 60 * 1000;
        let failure_retention_ms = self.config.failure_retention_days * 24 * 60 * 60 * 1000;

        let jobs = self.jobs_provider.list_jobs()?;
        let mut deleted = 0;

        for job in jobs {
            if job.status == "running" {
                continue;
            }

            let reference_time = job.end_time.unwrap_or(job.start_time);
            let age_ms = now - reference_time;

            let threshold = if job.status == "failed" {
                failure_retention_ms
            } else {
                retention_ms
            };

            if age_ms > threshold {
                self.jobs_provider.delete_job(&job.job_id)?;
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    /// Get the retention configuration
    pub fn config(&self) -> &RetentionConfig {
        &self.config
    }

    /// Update retention configuration
    pub fn set_config(&mut self, config: RetentionConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::storage::RocksDbInit;
    use crate::tables::system::JobRecord;
    use tempfile::TempDir;

    fn setup_retention() -> (RetentionPolicy, Arc<JobsTableProvider>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(db).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql));
        let retention = RetentionPolicy::with_defaults(Arc::clone(&jobs_provider));
        (retention, jobs_provider, temp_dir)
    }

    #[test]
    fn test_retention_default_config() {
        let (retention, _provider, _temp_dir) = setup_retention();
        assert_eq!(retention.config().retention_days, 30);
        assert_eq!(retention.config().failure_retention_days, 90);
        assert!(retention.config().extended_failure_retention);
    }

    #[test]
    fn test_cleanup_old_completed_jobs() {
        let (retention, provider, _temp_dir) = setup_retention();

        // Create an old completed job (60 days ago)
        let old_time = chrono::Utc::now().timestamp_millis() - (60 * 24 * 60 * 60 * 1000);
        let mut old_job = JobRecord::new(
            "old-completed".to_string(),
            "flush".to_string(),
            "node-1".to_string(),
        )
        .complete(Some("Success".to_string()));
        old_job.created_at = old_time;
        old_job.started_at = Some(old_time);
        old_job.completed_at = Some(old_time + 1000);

        // Create a recent completed job
        let recent_job = JobRecord::new(
            "recent-completed".to_string(),
            "flush".to_string(),
            "node-1".to_string(),
        )
        .complete(Some("Success".to_string()));

        provider.insert_job(old_job).unwrap();
        provider.insert_job(recent_job).unwrap();

        // Run retention
        let deleted = retention.enforce().unwrap();
        assert_eq!(deleted, 1);

        // Verify old job is gone, recent job remains
        assert!(provider.get_job("old-completed").unwrap().is_none());
        assert!(provider.get_job("recent-completed").unwrap().is_some());
    }

    #[test]
    fn test_extended_failure_retention() {
        let (retention, provider, _temp_dir) = setup_retention();

        // Create a failed job that's 60 days old (older than success retention, but within failure retention)
        let failed_time = chrono::Utc::now().timestamp_millis() - (60 * 24 * 60 * 60 * 1000);
        let mut failed_job = JobRecord::new(
            "old-failed".to_string(),
            "backup".to_string(),
            "node-1".to_string(),
        )
        .fail("Disk full".to_string());
        failed_job.created_at = failed_time;
        failed_job.started_at = Some(failed_time);
        failed_job.completed_at = Some(failed_time + 1000);

        provider.insert_job(failed_job).unwrap();

        // Run retention
        let deleted = retention.enforce().unwrap();

        // Failed job should still exist due to extended retention
        assert_eq!(deleted, 0);
        assert!(provider.get_job("old-failed").unwrap().is_some());
    }

    #[test]
    fn test_cleanup_very_old_failures() {
        let (retention, provider, _temp_dir) = setup_retention();

        // Create a failed job that's 120 days old (older than failure retention)
        let very_old_time = chrono::Utc::now().timestamp_millis() - (120 * 24 * 60 * 60 * 1000);
        let mut very_old_failed = JobRecord::new(
            "very-old-failed".to_string(),
            "cleanup".to_string(),
            "node-1".to_string(),
        )
        .fail("Error".to_string());
        very_old_failed.created_at = very_old_time;
        very_old_failed.started_at = Some(very_old_time);
        very_old_failed.completed_at = Some(very_old_time + 1000);

        provider.insert_job(very_old_failed).unwrap();

        // Run retention
        let deleted = retention.enforce().unwrap();

        // Very old failed job should be deleted
        assert_eq!(deleted, 1);
        assert!(provider.get_job("very-old-failed").unwrap().is_none());
    }

    #[test]
    fn test_never_delete_running_jobs() {
        let (retention, provider, _temp_dir) = setup_retention();

        // Create a running job that started 120 days ago
        let old_running_time = chrono::Utc::now().timestamp_millis() - (120 * 24 * 60 * 60 * 1000);
        let mut old_running = JobRecord::new(
            "old-running".to_string(),
            "long-task".to_string(),
            "node-1".to_string(),
        );
        old_running.created_at = old_running_time;
        old_running.started_at = Some(old_running_time);
        old_running.completed_at = None;

        provider.insert_job(old_running).unwrap();

        // Run retention
        let deleted = retention.enforce().unwrap();

        // Running job should never be deleted
        assert_eq!(deleted, 0);
        assert!(provider.get_job("old-running").unwrap().is_some());
    }

    #[test]
    fn test_custom_retention_config() {
        let temp_dir = TempDir::new().unwrap();
        let init = RocksDbInit::new(temp_dir.path().to_str().unwrap());
        let db = init.open().unwrap();
        let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(db).unwrap());
        let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql));

        let custom_config = RetentionConfig {
            retention_days: 7,
            extended_failure_retention: false,
            failure_retention_days: 7,
        };

        let retention = RetentionPolicy::new(Arc::clone(&jobs_provider), custom_config);

        assert_eq!(retention.config().retention_days, 7);
        assert!(!retention.config().extended_failure_retention);
    }

    #[test]
    fn test_update_retention_config() {
        let (mut retention, _provider, _temp_dir) = setup_retention();

        let new_config = RetentionConfig {
            retention_days: 14,
            extended_failure_retention: false,
            failure_retention_days: 14,
        };

        retention.set_config(new_config);
        assert_eq!(retention.config().retention_days, 14);
    }
}
