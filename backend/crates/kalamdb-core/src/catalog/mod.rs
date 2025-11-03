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
//! **Phase 10 Complete**: Unified SchemaCache replaces old dual-cache architecture
//! - Deleted: table_cache.rs (516 lines) - old TableCache implementation
//! - Deleted: tables/system/schemas/schema_cache.rs (443 lines) - old system SchemaCache
//! - Deleted: table_metadata.rs (252 lines) - replaced by CachedTableData
//! - Single source of truth: CachedTableData in unified SchemaCache

pub mod schema_cache; // Phase 10: Unified cache implementation

pub use schema_cache::{CachedTableData, SchemaCache};

// Re-export common types from kalamdb_commons for convenience
pub use kalamdb_commons::models::{NamespaceId, TableName, UserId};
pub use kalamdb_commons::schemas::TableType;
