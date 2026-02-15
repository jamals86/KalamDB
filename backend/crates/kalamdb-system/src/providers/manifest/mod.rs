//! System.manifest table v2 (EntityStore-based)
//!
//! This module implements the system.manifest table using the EntityStore architecture.
//! It provides a read-only view of manifest cache entries for query optimization.

pub mod manifest_indexes;
mod manifest_definition;
pub mod manifest_provider;
pub mod models;

pub use manifest_indexes::create_manifest_indexes;
pub use manifest_definition::{manifest_arrow_schema, manifest_table_definition};
pub use manifest_provider::{InMemoryChecker, ManifestTableProvider};
pub use models::{
    ColumnStats, FileRef, FileSubfolderState, Manifest, ManifestCacheEntry, SegmentMetadata,
    SegmentStatus, SyncState,
};
