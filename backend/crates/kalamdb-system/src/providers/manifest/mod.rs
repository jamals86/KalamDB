//! System.manifest table v2 (EntityStore-based)
//!
//! This module implements the system.manifest table using the EntityStore architecture.
//! It provides a read-only view of manifest cache entries for query optimization.

pub mod manifest_indexes;
pub mod manifest_provider;
pub mod manifest_store;
pub mod manifest_table;

pub use manifest_indexes::create_manifest_indexes;
pub use manifest_provider::{InMemoryChecker, ManifestTableProvider};
pub use manifest_store::{new_manifest_store, ManifestStore};
pub use manifest_table::ManifestTableSchema;
