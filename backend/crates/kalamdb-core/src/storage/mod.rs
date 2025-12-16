//! Storage module for data persistence
//!
//! This module provides storage utilities for KalamDB core.
//! Low-level storage engines are isolated in `kalamdb-store` to allow
//! pluggable backends (RocksDB, in-memory, etc.).
//!
//! Phase 13.8 Migration: ParquetWriter moved to kalamdb-filestore
//! StorageRegistry remains here as it manages storage metadata/configuration

pub mod storage_registry;

// Re-export generic backend traits from kalamdb-store
pub use kalamdb_store::StorageBackend;

// Re-export from kalamdb-filestore (Phase 13.8)
pub use kalamdb_filestore::ParquetWriter;
pub use kalamdb_filestore::{
    build_object_store, is_remote_url, materialize_remote_parquet_dir,
    materialize_remote_parquet_dir_sync, object_key_for_path, write_parquet_to_storage,
    write_parquet_to_storage_sync, ParquetWriteResult,
};

pub use storage_registry::StorageRegistry;
