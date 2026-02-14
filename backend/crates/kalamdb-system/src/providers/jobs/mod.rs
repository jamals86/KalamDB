//! System.jobs table module (system_jobs in RocksDB)
//!
//! This module contains all components for the system.jobs table:
//! - Job model and related types (JobFilter, JobOptions)
//! - Table schema definition with OnceLock caching
//! - IndexedEntityStore wrapper for type-safe storage with automatic index management
//! - TableProvider for DataFusion integration
//! - Index definitions for efficient queries

pub mod jobs_indexes;
pub mod jobs_provider;
pub mod models;

pub use jobs_indexes::{
    create_jobs_indexes, parse_job_status, status_to_u8, JobIdempotencyKeyIndex,
    JobStatusCreatedAtIndex,
};
pub use jobs_provider::{JobsStore, JobsTableProvider};
pub use models::{Job, JobFilter, JobOptions, JobSortField, SortOrder};
