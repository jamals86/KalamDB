//! Job Executors Module
//!
//! Unified job executor trait and registry for all job types.
//!
//! This module provides:
//! - `JobExecutor` trait: Common interface for all job executors
//! - `JobDecision`: Result type for job execution (Completed, Retry, Failed)
//! - `JobContext`: Execution context with app access and auto-prefixed logging
//! - `JobRegistry`: Thread-safe registry mapping JobType to executors
//! - Concrete executors: Flush, Cleanup, Retention, StreamEviction, UserCleanup, Compact, Backup, Restore

pub mod executor_trait;
pub mod registry;

// Concrete executor implementations
pub mod backup;
pub mod cleanup;
pub mod compact;
pub mod flush;
pub mod job_cleanup;
pub mod restore;
pub mod retention;
pub mod stream_eviction;
pub mod user_cleanup;

// Re-export key types
// Export core trait and types
pub use executor_trait::{CancellationToken, JobContext, JobDecision, JobExecutor, JobParams};
pub use registry::JobRegistry;

// Re-export concrete executors
pub use backup::BackupExecutor;
pub use cleanup::CleanupExecutor;
pub use compact::CompactExecutor;
pub use flush::FlushExecutor;
pub use job_cleanup::JobCleanupExecutor;
pub use restore::RestoreExecutor;
pub use retention::RetentionExecutor;
pub use stream_eviction::StreamEvictionExecutor;
pub use user_cleanup::UserCleanupExecutor;
