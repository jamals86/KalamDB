//! Job management trait for abstracting job execution and cancellation
//!
//! This module defines the `JobManager` trait which provides a generic interface
//! for managing asynchronous job execution with cancellation support. The design
//! is intentionally generic to support both Tokio-based implementations (using
//! JoinHandle) and future actor-based implementations (using actor supervision).
//!
//! ## Design Rationale
//!
//! The `JobManager` trait serves as an abstraction layer that allows the system
//! to evolve from a simpler Tokio-based job execution model to a more sophisticated
//! actor-based model without requiring changes to the calling code.
//!
//! ### Current Implementation: TokioJobManager
//!
//! The initial implementation uses Tokio's `JoinHandle` for job tracking:
//! - Jobs are spawned as Tokio tasks
//! - Cancellation uses `JoinHandle::abort()` for immediate termination
//! - Job status is tracked in-memory via HashMap
//! - Simple and lightweight for single-node deployments
//!
//! ### Future Migration: Actor-Based JobManager
//!
//! Future implementations could use actor frameworks (e.g., Actix, Ractor, Tokio Actors):
//! - Jobs run as supervised actors with lifecycle management
//! - Cancellation sends graceful shutdown messages to actors
//! - Actor supervision trees provide fault tolerance
//! - Better suited for distributed systems with job coordination
//!
//! ## Examples
//!
//! ```rust,no_run
//! use kalamdb_core::jobs::{JobManager, TokioJobManager, JobStatus};
//! use std::sync::Arc;
//! use uuid::Uuid;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a job manager
//! let manager = Arc::new(TokioJobManager::new());
//!
//! // Start a job
//! let job_id = Uuid::new_v4().to_string();
//! manager.start_job(
//!     job_id.clone(),
//!     "flush".to_string(),
//!     async move {
//!         // Perform flush operation
//!         tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
//!         Ok("Flushed 1000 rows".to_string())
//!     }
//! ).await?;
//!
//! // Check status
//! if let Some(status) = manager.get_job_status(&job_id).await? {
//!     println!("Job status: {:?}", status);
//! }
//!
//! // Cancel the job
//! manager.cancel_job(&job_id).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::KalamDbError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::future::Future;
use std::pin::Pin;

/// Job status enumeration
///
/// Represents the current state of a job in the system
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    /// Job is currently running
    Running,
    /// Job completed successfully with optional result message
    Completed(Option<String>),
    /// Job failed with error message
    Failed(String),
    /// Job was cancelled before completion
    Cancelled,
}

/// Job information returned by get_job_status
#[derive(Debug, Clone)]
pub struct JobInfo {
    /// Job ID
    pub job_id: String,
    /// Job type (e.g., "flush", "compact", "cleanup")
    pub job_type: String,
    /// Current status
    pub status: JobStatus,
    /// When the job was started
    pub start_time: DateTime<Utc>,
    /// When the job finished (if applicable)
    pub end_time: Option<DateTime<Utc>>,
}

/// Generic job manager trait
///
/// This trait provides a unified interface for managing asynchronous jobs with
/// support for starting, cancelling, and querying job status. The trait is
/// designed to be implementation-agnostic, allowing for both Tokio-based and
/// actor-based implementations.
///
/// # Design Considerations
///
/// 1. **Cancellation Semantics**: The `cancel_job` method should attempt graceful
///    cancellation when possible, but may use forceful termination if needed.
///    Implementations should document their cancellation behavior.
///
/// 2. **Status Tracking**: Job status should be updated in the system.jobs table
///    as the source of truth, with in-memory state as a cache for performance.
///
/// 3. **Error Handling**: Methods return `Result<(), KalamDbError>` to propagate
///    errors consistently across the system.
///
/// 4. **Async Design**: All methods are async to support both synchronous and
///    asynchronous implementations without blocking.
#[async_trait]
pub trait JobManager: Send + Sync {
    /// Start a new job
    ///
    /// Spawns the job asynchronously and returns immediately. The job will run
    /// in the background, and its status can be queried via `get_job_status`.
    ///
    /// # Arguments
    ///
    /// * `job_id` - Unique identifier for the job (typically a UUID)
    /// * `job_type` - Type of job (e.g., "flush", "compact", "cleanup")
    /// * `job_future` - Async function that performs the job work
    ///
    /// # Returns
    ///
    /// `Ok(())` if the job was started successfully, or an error if the job could
    /// not be spawned (e.g., duplicate job_id).
    ///
    /// # Notes
    ///
    /// The job_future should handle its own error logging and status updates to
    /// the system.jobs table. The JobManager is responsible only for lifecycle
    /// management (start/cancel/status).
    async fn start_job(
        &self,
        job_id: String,
        job_type: String,
        job_future: Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'static>>,
    ) -> Result<(), KalamDbError>;

    /// Cancel a running job
    ///
    /// Attempts to cancel a job that is currently running. The cancellation may
    /// be graceful or forceful depending on the implementation.
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to cancel
    ///
    /// # Returns
    ///
    /// `Ok(())` if the cancellation was initiated, or an error if:
    /// - The job does not exist
    /// - The job is already completed/failed/cancelled
    /// - The implementation cannot cancel the job
    ///
    /// # Notes
    ///
    /// This method initiates cancellation but does not wait for the job to
    /// terminate. Use `get_job_status` to verify the job has transitioned to
    /// the `Cancelled` state.
    ///
    /// ## Tokio Implementation
    ///
    /// Uses `JoinHandle::abort()` which forcefully terminates the task. The task
    /// will receive a cancellation signal and should clean up resources promptly.
    ///
    /// ## Future Actor Implementation
    ///
    /// Could send a graceful shutdown message to the actor, allowing it to:
    /// - Complete in-progress work
    /// - Save intermediate state
    /// - Release resources cleanly
    async fn cancel_job(&self, job_id: &str) -> Result<(), KalamDbError>;

    /// Get the current status of a job
    ///
    /// Queries the in-memory job registry for the current status. For jobs that
    /// have completed, the status may be cached briefly before being removed.
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to query
    ///
    /// # Returns
    ///
    /// `Some(JobInfo)` if the job is found, `None` if the job does not exist or
    /// has been removed from the registry (e.g., old completed jobs).
    ///
    /// # Notes
    ///
    /// This method queries in-memory state for performance. For authoritative
    /// status including historical jobs, query the system.jobs table directly.
    async fn get_job_status(&self, job_id: &str) -> Result<Option<JobInfo>, KalamDbError>;

    /// List all active jobs
    ///
    /// Returns information about all jobs that are currently tracked by the
    /// manager (typically jobs that are Running).
    ///
    /// # Returns
    ///
    /// A vector of JobInfo for all active jobs, or an empty vector if no jobs
    /// are running.
    async fn list_active_jobs(&self) -> Result<Vec<JobInfo>, KalamDbError>;

    /// Wait for a job to complete
    ///
    /// Blocks until the specified job completes (successfully or with failure)
    /// or is cancelled. This is useful for graceful shutdown scenarios where
    /// we want to ensure jobs finish before terminating the process.
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to wait for
    /// * `timeout` - Maximum time to wait (None = wait indefinitely)
    ///
    /// # Returns
    ///
    /// The final job status, or an error if the job does not exist or the
    /// timeout is exceeded.
    async fn wait_for_job(
        &self,
        job_id: &str,
        timeout: Option<std::time::Duration>,
    ) -> Result<JobStatus, KalamDbError>;
}
