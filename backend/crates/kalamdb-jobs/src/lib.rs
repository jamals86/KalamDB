//! # kalamdb-jobs
//!
//! Background job system for KalamDB.
//!
//! This crate manages asynchronous background jobs including:
//! - **FlushExecutor**: Flush hot storage (RocksDB) to cold storage (Parquet)
//! - **CleanupExecutor**: Clean up old flushed data
//! - **RetentionExecutor**: Apply retention policies
//! - **StreamEvictionExecutor**: Evict expired stream table data
//! - **UserCleanupExecutor**: Clean up deleted user data
//! - **CompactExecutor**: Compact Parquet files
//! - **BackupExecutor**: Create database backups
//! - **RestoreExecutor**: Restore from backups
//!
//! ## Architecture
//!
//! ### UnifiedJobManager
//! - Central job scheduler and coordinator
//! - Typed JobIds (FL/CL/RT/SE/UC/CO/BK/RS)
//! - Idempotency via idempotency keys
//! - Retry logic (3× default with exponential backoff)
//! - Crash recovery (marks Running jobs as Failed on restart)
//!
//! ### Job Lifecycle
//! ```text
//! New → Queued → Running → Completed
//!                      ↓
//!                  Failed → Retrying → Running
//!                      ↓
//!                  Cancelled
//! ```
//!
//! ### Idempotency
//! Format: "{job_type}:{namespace}:{table}:{date}"
//! Prevents duplicate jobs from running simultaneously.
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_jobs::{UnifiedJobManager, FlushExecutor};
//!
//! // Create job manager
//! let manager = UnifiedJobManager::new(app_context);
//!
//! // Submit flush job
//! let job_id = manager.submit_flush_job(
//!     namespace_id,
//!     table_name,
//!     flush_metadata,
//! ).await?;
//!
//! // Check job status
//! let status = manager.get_job_status(&job_id).await?;
//! ```

pub mod manager;
pub mod executors;
pub mod error;

// Re-export commonly used types
pub use error::{JobError, Result};
pub use manager::UnifiedJobManager;

// Re-export all executors
pub use executors::{
    BackupExecutor,
    CleanupExecutor,
    CompactExecutor,
    FlushExecutor,
    RestoreExecutor,
    RetentionExecutor,
    StreamEvictionExecutor,
    UserCleanupExecutor,
    JobExecutor,
};
