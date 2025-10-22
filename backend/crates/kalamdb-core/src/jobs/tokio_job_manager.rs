//! Tokio-based implementation of JobManager
//!
//! This module provides a concrete implementation of the `JobManager` trait using
//! Tokio's task spawning and JoinHandle-based cancellation. This is the initial
//! implementation designed for single-node deployments.
//!
//! ## Architecture
//!
//! The `TokioJobManager` maintains an in-memory registry of active jobs using
//! a HashMap protected by an RwLock. Each job is tracked with:
//! - JoinHandle for cancellation via `abort()`
//! - JobInfo metadata (status, timestamps, job type)
//!
//! ## Cancellation Behavior
//!
//! Cancellation is immediate and forceful:
//! 1. `cancel_job()` calls `JoinHandle::abort()`
//! 2. The Tokio task receives a cancellation signal
//! 3. The task terminates as soon as possible
//! 4. Job status is updated to `Cancelled` in the registry
//!
//! **Note**: Aborted tasks may not complete cleanup operations. For graceful
//! cancellation, consider implementing cancellation tokens within the job logic.
//!
//! ## Examples
//!
//! ```rust,no_run
//! use kalamdb_core::jobs::{JobManager, TokioJobManager, JobStatus};
//! use std::sync::Arc;
//! use uuid::Uuid;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = Arc::new(TokioJobManager::new());
//!
//! // Start a long-running job
//! let job_id = Uuid::new_v4().to_string();
//! manager.start_job(
//!     job_id.clone(),
//!     "flush".to_string(),
//!     Box::pin(async move {
//!         // Simulate work
//!         for i in 0..100 {
//!             tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
//!             // Could check for cancellation here
//!         }
//!         Ok("Flushed 1000 rows".to_string())
//!     })
//! ).await?;
//!
//! // Cancel after 1 second
//! tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//! manager.cancel_job(&job_id).await?;
//!
//! // Verify cancellation
//! if let Some(info) = manager.get_job_status(&job_id).await? {
//!     assert_eq!(info.status, JobStatus::Cancelled);
//! }
//! # Ok(())
//! # }
//! ```

use super::job_manager::{JobInfo, JobManager, JobStatus};
use crate::error::KalamDbError;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};
use tokio::task::JoinHandle;

/// Job entry in the registry
struct JobEntry {
    /// Job information
    info: JobInfo,
    /// Tokio task handle for cancellation
    handle: JoinHandle<Result<String, String>>,
}

/// Tokio-based job manager implementation
///
/// Uses Tokio's task spawning and JoinHandle-based cancellation for job management.
/// Suitable for single-node deployments where jobs run as local tasks.
pub struct TokioJobManager {
    /// Active jobs registry (keyed by job_id)
    jobs: Arc<RwLock<HashMap<String, JobEntry>>>,
}

impl Default for TokioJobManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TokioJobManager {
    /// Create a new Tokio job manager
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Remove completed/failed/cancelled jobs from registry
    ///
    /// Called periodically to prevent unbounded memory growth
    fn cleanup_finished_jobs(&self) {
        if let Ok(mut jobs) = self.jobs.write() {
            jobs.retain(|_, entry| entry.handle.is_finished() == false);
        }
    }
}

#[async_trait]
impl JobManager for TokioJobManager {
    /// Start a new job using Tokio task spawning
    ///
    /// The job is spawned as a Tokio task and tracked in the registry with a
    /// JoinHandle for cancellation support.
    ///
    /// # Implementation Notes
    ///
    /// - Jobs are spawned with `tokio::spawn()` for independent execution
    /// - The JoinHandle is stored for cancellation via `abort()`
    /// - Job status is updated in the registry as the job progresses
    /// - Duplicate job_id will return an error
    async fn start_job(
        &self,
        job_id: String,
        job_type: String,
        job_future: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'static>,
        >,
    ) -> Result<(), KalamDbError> {
        // Check for duplicate job_id
        {
            let jobs = self
                .jobs
                .read()
                .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;

            if jobs.contains_key(&job_id) {
                return Err(KalamDbError::Other(format!(
                    "Job with ID {} already exists",
                    job_id
                )));
            }
        }

        // Create job info
        let info = JobInfo {
            job_id: job_id.clone(),
            job_type: job_type.clone(),
            status: JobStatus::Running,
            start_time: Utc::now(),
            end_time: None,
        };

        // Clone Arc for task closure
        let jobs_arc = Arc::clone(&self.jobs);
        let job_id_clone = job_id.clone();

        // Spawn Tokio task
        let handle = tokio::spawn(async move {
            let result = job_future.await;

            // Update job status after completion
            if let Ok(mut jobs) = jobs_arc.write() {
                if let Some(entry) = jobs.get_mut(&job_id_clone) {
                    match &result {
                        Ok(msg) => {
                            entry.info.status = JobStatus::Completed(Some(msg.clone()));
                            entry.info.end_time = Some(Utc::now());
                        }
                        Err(err) => {
                            entry.info.status = JobStatus::Failed(err.clone());
                            entry.info.end_time = Some(Utc::now());
                        }
                    }
                }
            }

            result
        });

        // Add to registry
        let entry = JobEntry { info, handle };

        let mut jobs = self
            .jobs
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        jobs.insert(job_id, entry);

        Ok(())
    }

    /// Cancel a job using JoinHandle::abort()
    ///
    /// This performs immediate forceful cancellation. The task will receive a
    /// cancellation signal and terminate as soon as possible.
    ///
    /// # Implementation Notes
    ///
    /// - Uses `JoinHandle::abort()` for immediate termination
    /// - Job status is updated to `Cancelled` in the registry
    /// - Aborted tasks may not complete cleanup operations
    /// - Returns error if job doesn't exist or is already finished
    async fn cancel_job(&self, job_id: &str) -> Result<(), KalamDbError> {
        let mut jobs = self
            .jobs
            .write()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire write lock: {}", e)))?;

        let entry = jobs
            .get_mut(job_id)
            .ok_or_else(|| KalamDbError::Other(format!("Job {} not found", job_id)))?;

        // Check if job is already finished
        if entry.handle.is_finished() {
            return Err(KalamDbError::Other(format!(
                "Job {} is already finished",
                job_id
            )));
        }

        // Abort the task
        entry.handle.abort();

        // Update status to Cancelled
        entry.info.status = JobStatus::Cancelled;
        entry.info.end_time = Some(Utc::now());

        Ok(())
    }

    /// Get the current status of a job from the in-memory registry
    ///
    /// Returns None if the job doesn't exist (either never existed or was
    /// cleaned up after completion).
    async fn get_job_status(&self, job_id: &str) -> Result<Option<JobInfo>, KalamDbError> {
        let jobs = self
            .jobs
            .read()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;

        Ok(jobs.get(job_id).map(|entry| entry.info.clone()))
    }

    /// List all active jobs (Running status)
    ///
    /// Returns a snapshot of all jobs currently in the registry that are running.
    async fn list_active_jobs(&self) -> Result<Vec<JobInfo>, KalamDbError> {
        let jobs = self
            .jobs
            .read()
            .map_err(|e| KalamDbError::Other(format!("Failed to acquire read lock: {}", e)))?;

        let active_jobs = jobs
            .values()
            .filter(|entry| matches!(entry.info.status, JobStatus::Running))
            .map(|entry| entry.info.clone())
            .collect();

        Ok(active_jobs)
    }

    /// Wait for a job to complete with optional timeout
    ///
    /// Polls the job status until it transitions to a terminal state (Completed,
    /// Failed, or Cancelled). If a timeout is specified and exceeded, returns an error.
    ///
    /// # Implementation Notes
    ///
    /// - Uses polling with 100ms intervals
    /// - Returns immediately if job is already finished
    /// - For graceful shutdown, use this with the flush_job_shutdown_timeout
    async fn wait_for_job(
        &self,
        job_id: &str,
        timeout: Option<std::time::Duration>,
    ) -> Result<JobStatus, KalamDbError> {
        let start = std::time::Instant::now();

        loop {
            // Check job status
            if let Some(info) = self.get_job_status(job_id).await? {
                match info.status {
                    JobStatus::Running => {
                        // Continue waiting
                    }
                    other => {
                        // Job finished
                        return Ok(other);
                    }
                }
            } else {
                // Job not found - assume it was cleaned up after completion
                return Err(KalamDbError::Other(format!(
                    "Job {} not found (may have been cleaned up)",
                    job_id
                )));
            }

            // Check timeout
            if let Some(timeout) = timeout {
                if start.elapsed() >= timeout {
                    return Err(KalamDbError::Other(format!(
                        "Timeout waiting for job {} to complete",
                        job_id
                    )));
                }
            }

            // Sleep before next poll
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_start_and_complete_job() {
        let manager = TokioJobManager::new();
        let job_id = "test-job-1".to_string();

        // Start job
        manager
            .start_job(
                job_id.clone(),
                "test".to_string(),
                Box::pin(async { Ok("success".to_string()) }),
            )
            .await
            .unwrap();

        // Wait for completion
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check status
        let info = manager.get_job_status(&job_id).await.unwrap().unwrap();
        assert_eq!(
            info.status,
            JobStatus::Completed(Some("success".to_string()))
        );
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let manager = TokioJobManager::new();
        let job_id = "test-job-2".to_string();

        // Start long-running job
        manager
            .start_job(
                job_id.clone(),
                "test".to_string(),
                Box::pin(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    Ok("success".to_string())
                }),
            )
            .await
            .unwrap();

        // Cancel immediately
        manager.cancel_job(&job_id).await.unwrap();

        // Check status
        let info = manager.get_job_status(&job_id).await.unwrap().unwrap();
        assert_eq!(info.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_duplicate_job_id() {
        let manager = TokioJobManager::new();
        let job_id = "test-job-3".to_string();

        // Start first job
        manager
            .start_job(
                job_id.clone(),
                "test".to_string(),
                Box::pin(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    Ok("success".to_string())
                }),
            )
            .await
            .unwrap();

        // Try to start duplicate
        let result = manager
            .start_job(
                job_id.clone(),
                "test".to_string(),
                Box::pin(async { Ok("success".to_string()) }),
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_active_jobs() {
        let manager = TokioJobManager::new();

        // Start multiple jobs
        for i in 0..3 {
            let job_id = format!("test-job-{}", i);
            manager
                .start_job(
                    job_id,
                    "test".to_string(),
                    Box::pin(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        Ok("success".to_string())
                    }),
                )
                .await
                .unwrap();
        }

        // List active jobs
        let active = manager.list_active_jobs().await.unwrap();
        assert_eq!(active.len(), 3);
    }
}
