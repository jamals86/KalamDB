//! System.jobs table module (system_jobs in RocksDB)
//!
//! This module contains all components for the system.jobs table:
//! - Table schema definition with OnceLock caching
//! - SystemTableStore wrapper for type-safe storage
//! - TableProvider for DataFusion integration

pub mod jobs_provider;
pub mod jobs_store;
pub mod jobs_table;

pub use jobs_provider::JobsTableProvider;
pub use jobs_store::{new_jobs_store, JobsStore};
pub use jobs_table::JobsTableSchema;
