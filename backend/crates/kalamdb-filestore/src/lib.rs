//! # kalamdb-filestore
//!
//! Object store operations for KalamDB cold storage.
//!
//! This crate handles all storage operations through StorageCached:
//! - Parquet file reading/writing (local filesystem, S3, GCS, Azure)
//! - Manifest file management
//!
//! ## Architecture
//!
//! - **storage_cached**: Unified file operations (read/write/list/delete)
//! - **parquet_storage_writer**: Write RecordBatches to Parquet with bloom filters
//! - **parquet_reader_ops**: Read Parquet files into RecordBatches
//! - **manifest_ops**: Manifest JSON read/write
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_filestore::StorageCached;
//!
//! // Write/read files using StorageCached
//! let cached = StorageCached::with_default_timeouts(storage);
//! let _ = cached.put_sync(table_type, &table_id, user_id, "file.parquet", data)?;
//! ```

mod core;
pub mod error;
pub mod files;
pub mod health;
pub mod manifest;
pub mod parquet;
pub mod paths;
pub mod registry;

// Backwards-compatible module aliases
pub use manifest::json as manifest_ops;
pub use parquet::reader as parquet_reader_ops;
pub use parquet::writer as parquet_storage_writer;

#[cfg(test)]
mod tests;

// Re-export commonly used types
pub use error::{FilestoreError, Result};
pub use files::{FileStorageService, StagedFile, StagingManager};
pub use manifest_ops::{manifest_exists, read_manifest_json, write_manifest_json};
pub use parquet_reader_ops::{
    parse_parquet_from_bytes, parse_parquet_schema_from_bytes,
    read_parquet_batches, read_parquet_batches_sync, read_parquet_schema, read_parquet_schema_sync,
};
pub use parquet_storage_writer::{
    ParquetWriteResult,
};

// Storage registry re-exports
pub use registry::{StorageCached, StorageRegistry};

// Health check re-exports
pub use health::{ConnectivityTestResult, HealthStatus, StorageHealthResult, StorageHealthService};
