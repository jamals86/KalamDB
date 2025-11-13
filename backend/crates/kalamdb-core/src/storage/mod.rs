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

pub use storage_registry::StorageRegistry;
