//! Storage module for data persistence
//!
//! This module provides storage utilities for KalamDB core.
//! All storage operations go through kalamdb-filestore's StorageCached.
//!
//! **Note**: As of the registry consolidation, StorageCached and StorageRegistry
//! have been moved to kalamdb-filestore. This module now serves as a compatibility
//! re-export layer.

// Re-export generic backend traits from kalamdb-store
pub use kalamdb_store::StorageBackend;

// Re-export registry types from kalamdb-filestore (backward compatibility)
pub use kalamdb_filestore::{StorageCached, StorageRegistry};
