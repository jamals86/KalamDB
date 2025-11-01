//! Job management module
//!
//! This module provides infrastructure for executing and monitoring background jobs:
//! - Job execution framework with lifecycle management
//! - Resource usage tracking (CPU, memory)
//! - Retention policies for job history cleanup
//!
//! ## Examples
//!
//! ```rust,no_run
//! use kalamdb_core::jobs::{JobExecutor, RetentionPolicy};
//! use kalamdb_core::tables::system::JobsTableProvider;
//! use kalamdb_sql::KalamSql;
//! use std::sync::Arc;
//!
//! # fn example(kalam_sql: Arc<KalamSql>) {
//! let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.clone()));
//! let executor = JobExecutor::new(Arc::clone(&jobs_provider), "node-1".to_string());
//!
//! // Execute a job
//! let result = executor.execute_job(
//!     "flush-001".to_string(),
//!     "flush".to_string(),
//!     Some("users.messages".to_string()),
//!     vec!["batch-001".to_string()],
//!     || {
//!         // Perform flush operation
//!         Ok("Flushed 1000 rows".to_string())
//!     },
//! );
//!
//! // Setup retention policy
//! let retention = RetentionPolicy::with_defaults(jobs_provider);
//! let deleted = retention.enforce().unwrap();
//! println!("Deleted {} old jobs", deleted);
//! # }
//! ```

pub mod executor;
pub mod job_cleanup;
pub mod job_manager;
pub mod retention;
pub mod stream_eviction;
pub mod stream_eviction_scheduler;
pub mod tokio_job_manager;
pub mod user_cleanup;

pub use executor::{JobExecutor, JobResult};
pub use job_cleanup::JobCleanupTask;
pub use job_manager::{JobInfo, JobManager, JobStatus};
pub use retention::{RetentionConfig, RetentionPolicy};
pub use stream_eviction::{StreamEvictionConfig, StreamEvictionJob};
pub use stream_eviction_scheduler::StreamEvictionScheduler;
pub use tokio_job_manager::TokioJobManager;
pub use user_cleanup::{UserCleanupConfig, UserCleanupJob};
