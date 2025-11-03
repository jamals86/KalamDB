//! Catalog module for namespace and table metadata management
//!
//! This module manages the catalog of namespaces, tables, and their associated metadata.
//!
//! **Note**: Basic type wrappers (UserId, NamespaceId, TableName, TableType) have been
//! moved to `kalamdb_commons::models` for shared usage across crates.
//!
//! **Note**: Namespace struct has been moved to `kalamdb_commons::system::Namespace`
//! as the single source of truth for all system table models.
//!
//! **Phase 10 Update**: New unified SchemaCache module replaces dual-cache architecture

pub mod schema_cache; // Phase 10: New unified cache
pub mod table_cache; // Phase 10: Will be deleted after migration
pub mod table_metadata; // Phase 10: Will be deleted after migration

pub use schema_cache::{CachedTableData, SchemaCache};
pub use table_cache::TableCache;
pub use table_metadata::TableMetadata;

// Re-export common types from kalamdb_commons for convenience
pub use kalamdb_commons::models::{NamespaceId, TableName, UserId};
pub use kalamdb_commons::schemas::TableType;
