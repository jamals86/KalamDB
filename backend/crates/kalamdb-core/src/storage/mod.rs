//! Storage module for data persistence
//!
//! This module provides storage utilities for KalamDB core.
//! Low-level storage engines are isolated in `kalamdb-store` to allow
//! pluggable backends (RocksDB, in-memory, etc.).

pub mod column_family_manager;
pub mod filesystem_backend;
pub mod parquet_writer;
pub mod path_template;
pub mod storage_registry;

// Re-export generic backend traits from kalamdb-store
pub use kalamdb_store::{RocksDBBackend, StorageBackend};
pub use column_family_manager::ColumnFamilyManager;
pub use filesystem_backend::FilesystemBackend;
pub use parquet_writer::ParquetWriter;
pub use path_template::PathTemplate;
pub use storage_registry::StorageRegistry;
