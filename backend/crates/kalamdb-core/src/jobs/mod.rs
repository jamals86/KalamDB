//! # Job Management System
//!
//! **Phase 9 Status**: UnifiedJobManager with 8 concrete executors is production-ready.
//! Legacy components (JobExecutor, old schedulers) are DEPRECATED and pending migration.
//!
//! ## Examples
//!
//! ```rust,no_run
//! // Phase 9: Unified Job Management with typed JobIds
//! use kalamdb_core::jobs::{UnifiedJobManager, JobRegistry};
//! use kalamdb_core::jobs::executors::*;
//! use kalamdb_core::tables::system::JobsTableProvider;
//! use kalamdb_store::test_utils::InMemoryBackend;
//! use kalamdb_store::StorageBackend;
//! use std::sync::Arc;
//! use kalamdb_commons::JobType;
//!
//! # fn example() {
//! let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
//! let jobs_provider = Arc::new(JobsTableProvider::new(backend));
//! let job_manager = Arc::new(UnifiedJobManager::new(jobs_provider));
//! 
//! // Register executors (8 concrete implementations)
//! let mut registry = JobRegistry::new();
//! registry.register("flush", Arc::new(FlushExecutor::new(/* ... */)));
//! registry.register("cleanup", Arc::new(CleanupExecutor::new(/* ... */)));
//! // ... register remaining 6 executors
//! 
//! // Create job with typed JobId (idempotency enforced)
//! let job_id = job_manager.create_job(
//!     JobType::Flush,
//!     "namespace_id",
//!     "flush-table-xyz",  // Idempotency key
//!     serde_json::json!({"table_id": "xyz"}),
//! ).await.unwrap();
//! 
//! // Job processing loop (spawned in lifecycle.rs)
//! // job_manager.run_loop(max_concurrent).await?;
//! # }
//! ```

// ============================================================================
// PHASE 9: UNIFIED JOB MANAGEMENT (PRODUCTION-READY)
// ============================================================================
pub mod unified_manager;
pub mod executors;

// ============================================================================
// DEPRECATED: LEGACY JOB COMPONENTS (PENDING MIGRATION)
// ============================================================================
// These modules are marked #[deprecated] and still used by lifecycle.rs
// Will be removed after background scheduler migration (Phase 9 deferred work)
#[deprecated(since = "0.1.0", note = "Use UnifiedJobManager with concrete executors instead. See unified_manager.rs")]
pub mod executor;
#[deprecated(since = "0.1.0", note = "Use UnifiedJobManager executors instead")]
pub mod job_cleanup;
#[deprecated(since = "0.1.0", note = "Use UnifiedJobManager executors instead")]
pub mod stream_eviction;
#[deprecated(since = "0.1.0", note = "Use UnifiedJobManager executors instead")]
pub mod stream_eviction_scheduler;
#[deprecated(since = "0.1.0", note = "Use UnifiedJobManager executors instead")]
pub mod user_cleanup;

#[cfg(test)]
mod tests;

// Phase 9 exports (primary API)
pub use unified_manager::JobManager as UnifiedJobManager;
pub use executors::{JobContext, JobDecision, JobExecutor as JobExecutorTrait, JobRegistry};

// Legacy exports (used by lifecycle.rs - pending migration)
#[allow(deprecated)]
pub use executor::{JobExecutor, JobResult};
#[allow(deprecated)]
pub use job_cleanup::JobCleanupTask;
#[allow(deprecated)]
pub use stream_eviction::{StreamEvictionConfig, StreamEvictionJob};
#[allow(deprecated)]
pub use stream_eviction_scheduler::StreamEvictionScheduler;
#[allow(deprecated)]
pub use user_cleanup::{UserCleanupConfig, UserCleanupJob};
