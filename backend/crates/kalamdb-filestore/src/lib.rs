//! # kalamdb-filestore
//!
//! Filesystem and Parquet file operations for KalamDB.
//!
//! This crate handles all cold storage operations:
//! - Parquet file reading/writing
//! - Batch file management
//! - Storage path utilities
//! - File cleanup operations
//! - Future: S3/object store integration
//!
//! ## Architecture
//!
//! - **Parquet Writer**: Writes Arrow RecordBatches to Parquet files
//! - **Parquet Reader**: Reads Parquet files back into Arrow RecordBatches
//! - **Batch Manager**: Tracks and manages flushed batch files
//! - **Path Utilities**: Consistent path generation for storage hierarchy
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_filestore::{ParquetWriter, ParquetReader};
//!
//! // Write batch to Parquet
//! let writer = ParquetWriter::new(storage_path)?;
//! writer.write_batch(namespace_id, table_name, batch)?;
//!
//! // Read batch from Parquet
//! let reader = ParquetReader::new(storage_path)?;
//! let batches = reader.read_batches(namespace_id, table_name)?;
//! ```

pub mod batch_manager;
pub mod cleanup;
pub mod error;
pub mod object_store_factory;
pub mod parquet_reader;
pub mod parquet_storage_writer;
pub mod parquet_writer;
pub mod path_utils;
pub mod remote_materializer;

#[cfg(test)]
mod tests;

// Re-export commonly used types
pub use batch_manager::BatchManager;
pub use cleanup::delete_parquet_tree_for_table;
pub use error::{FilestoreError, Result};
pub use object_store_factory::{build_object_store, is_remote_url, object_key_for_path};
pub use parquet_reader::ParquetReader;
pub use parquet_storage_writer::{
    write_parquet_to_storage, write_parquet_to_storage_sync, ParquetWriteResult,
};
pub use parquet_writer::ParquetWriter;
pub use path_utils::PathUtils;
pub use remote_materializer::{materialize_remote_parquet_dir, materialize_remote_parquet_dir_sync};
