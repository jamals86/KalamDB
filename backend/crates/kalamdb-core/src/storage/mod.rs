//! Storage module for data persistence
//!
//! This module provides storage utilities for KalamDB core.
//! Low-level storage engines are isolated in `kalamdb-store` to allow
//! pluggable backends (RocksDB, in-memory, etc.).

pub mod parquet_writer;
pub mod storage_registry;

// Re-export generic backend traits from kalamdb-store
pub use kalamdb_store::StorageBackend;
pub use parquet_writer::ParquetWriter;
pub use storage_registry::StorageRegistry;
