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
pub mod flush;
pub mod cleanup;
pub mod retention;
pub mod stream_eviction;
pub mod user_cleanup;
pub mod compact;
pub mod backup;
pub mod restore;

// Re-export key types
pub use executor_trait::{JobContext, JobDecision, JobExecutor};
pub use registry::JobRegistry;

// Re-export concrete executors
pub use backup::BackupExecutor;
pub use cleanup::CleanupExecutor;
pub use compact::CompactExecutor;
pub use flush::FlushExecutor;
pub use restore::RestoreExecutor;
pub use retention::RetentionExecutor;
pub use stream_eviction::StreamEvictionExecutor;
pub use user_cleanup::UserCleanupExecutor;
