//! # kalamdb-filestore
//!
//! Object store operations for KalamDB cold storage.
//!
//! This crate handles all storage operations through `object_store`:
//! - Parquet file reading/writing (local filesystem, S3, GCS, Azure)
//! - Manifest file management
//! - Batch file management
//! - File cleanup operations
//!
//! ## Architecture
//!
//! - **object_store_ops**: Core file operations (read/write/list/delete)
//! - **parquet_storage_writer**: Write RecordBatches to Parquet with bloom filters
//! - **parquet_reader_ops**: Read Parquet files into RecordBatches
//! - **manifest_ops**: Manifest JSON read/write
//! - **ObjectStoreBatchManager**: Track and manage batch files
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_filestore::{write_parquet_with_store_sync, read_parquet_batches_sync};
//!
//! // Write batch to Parquet using ObjectStore
//! write_parquet_with_store_sync(store, &storage, path, schema, batches, None)?;
//!
//! // Read batch from Parquet
//! let batches = read_parquet_batches_sync(store, &storage, path)?;
//! ```

pub mod batch_manager;
pub mod cleanup;
pub mod error;
pub mod manifest_ops;
pub mod object_store_factory;
pub mod object_store_ops;
pub mod parquet_reader_ops;
pub mod parquet_storage_writer;
pub mod remote_materializer;

#[cfg(test)]
mod tests;

// Re-export commonly used types
pub use batch_manager::ObjectStoreBatchManager;
pub use cleanup::delete_parquet_tree_for_table;
pub use error::{FilestoreError, Result};
pub use manifest_ops::{manifest_exists, read_manifest_json, write_manifest_json};
pub use object_store_factory::{build_object_store, is_remote_url, object_key_for_path};
pub use object_store_ops::{
    delete_file, delete_file_sync,
    delete_prefix, delete_prefix_sync,
    head_file, head_file_sync,
    list_files, list_files_sync,
    prefix_exists, prefix_exists_sync,
    read_file, read_file_sync,
    write_file, write_file_sync,
    FileMetadata,
};
pub use parquet_reader_ops::{
    read_parquet_batches, read_parquet_batches_sync,
    read_parquet_schema, read_parquet_schema_sync,
};
pub use parquet_storage_writer::{
    write_parquet_to_storage, write_parquet_to_storage_sync, 
    write_parquet_with_store, write_parquet_with_store_sync,
    ParquetWriteResult,
};
pub use remote_materializer::{materialize_remote_parquet_dir, materialize_remote_parquet_dir_sync};
