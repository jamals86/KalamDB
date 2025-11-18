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
pub mod parquet_reader;
pub mod parquet_writer;
pub mod path_utils;

#[cfg(test)]
mod tests;

// Re-export commonly used types
pub use batch_manager::BatchManager;
pub use cleanup::delete_parquet_tree_for_table;
pub use error::{FilestoreError, Result};
pub use parquet_reader::ParquetReader;
pub use parquet_writer::ParquetWriter;
pub use path_utils::PathUtils;
