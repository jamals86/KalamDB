//! System.jobs table module (system_jobs in RocksDB)
//!
//! This module contains all components for the system.jobs table:
//! - Table schema definition with OnceLock caching
//! - IndexedEntityStore wrapper for type-safe storage with automatic index management
//! - TableProvider for DataFusion integration
//! - Index definitions for efficient queries

pub mod jobs_indexes;
pub mod jobs_provider;
pub mod jobs_table;

pub use jobs_indexes::{
    create_jobs_indexes, parse_job_status, status_to_u8, JobIdempotencyKeyIndex,
    JobNamespaceTableIndex, JobStatusCreatedAtIndex,
};
pub use jobs_provider::{JobsStore, JobsTableProvider};
pub use jobs_table::JobsTableSchema;
