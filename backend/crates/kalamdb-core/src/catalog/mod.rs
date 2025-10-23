//! Catalog module for namespace and table metadata management
//!
//! This module manages the catalog of namespaces, tables, and their associated metadata.
//!
//! **Note**: Basic type wrappers (UserId, NamespaceId, TableName, TableType) have been
//! moved to `kalamdb_commons::models` for shared usage across crates.

pub mod namespace;
pub mod storage_location;
pub mod table_cache;
pub mod table_metadata;

pub use namespace::Namespace;
pub use storage_location::{LocationType, StorageLocation};
pub use table_cache::TableCache;
pub use table_metadata::TableMetadata;

// Re-export common types from kalamdb_commons for convenience
pub use kalamdb_commons::models::{NamespaceId, TableName, TableType, UserId};
