//! Storage module for data persistence
//!
//! This module provides storage utilities for KalamDB core.
//! All storage operations go through object_store for unified
//! local/remote storage handling.

pub mod storage_cache;
pub mod storage_registry;

// Re-export generic backend traits from kalamdb-store
pub use kalamdb_store::StorageBackend;

// Re-export from kalamdb-filestore
pub use kalamdb_filestore::{
    build_object_store, is_remote_url, materialize_remote_parquet_dir,
    materialize_remote_parquet_dir_sync, object_key_for_path, write_parquet_to_storage,
    write_parquet_to_storage_sync, write_parquet_with_store, write_parquet_with_store_sync,
    ParquetWriteResult,
};

// Re-export unified object_store operations
pub use kalamdb_filestore::{
    delete_file, delete_file_sync,
    delete_prefix, delete_prefix_sync,
    head_file, head_file_sync,
    list_files, list_files_sync,
    prefix_exists, prefix_exists_sync,
    read_file, read_file_sync,
    write_file, write_file_sync,
    FileMetadata,
};

// Re-export cached storage and registry
pub use storage_cache::StorageCached;
pub use storage_registry::StorageRegistry;
