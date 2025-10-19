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
//! use kalamdb_core::catalog::CatalogStore;
//! use std::sync::Arc;
//!
//! # fn example(catalog_store: Arc<CatalogStore>) {
//! let jobs_provider = Arc::new(JobsTableProvider::new(catalog_store));
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
pub mod retention;
pub mod stream_eviction;

pub use executor::{JobExecutor, JobResult};
pub use retention::{RetentionConfig, RetentionPolicy};
pub use stream_eviction::{StreamEvictionConfig, StreamEvictionJob};
