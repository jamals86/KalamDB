//! Schema Registry module for Arrow schema management and table metadata caching
//!
//! This module provides the unified SchemaRegistry for managing table schemas and metadata.
//! It combines in-memory caching with persistent storage via TablesTableProvider.
//!
//! **Note**: Basic type wrappers (UserId, NamespaceId, TableName, TableType) have been
//! moved to `kalamdb_commons::models` for shared usage across crates.
//!
//! **Note**: Namespace struct has been moved to `kalamdb_commons::system::Namespace`
//! as the single source of truth for all system table models.
//!
//! **Phase 5 Complete**: SchemaRegistry provides read-through/write-through persistence
//! **Phase 10 Complete**: Unified SchemaRegistry replaces old dual-cache architecture
//! - Deleted: table_cache.rs (516 lines) - old TableCache implementation
//! - Deleted: tables/system/schemas/schema_cache.rs (443 lines) - old system cache
//! - Deleted: table_metadata.rs (252 lines) - replaced by CachedTableData
//! - Single source of truth: CachedTableData in unified SchemaRegistry


pub mod arrow_schema;
pub mod projection;
pub mod system_columns;
pub mod views;

pub use arrow_schema::ArrowSchemaWithOptions;
pub use projection::{project_batch, schemas_compatible};
pub use system_columns::SystemColumns;

pub mod registry; // Phase 10: Unified cache implementation

pub use registry::{CachedTableData, SchemaRegistry};

// Re-export common types from kalamdb_commons for convenience
pub use kalamdb_commons::models::{NamespaceId, TableName, UserId};
pub use kalamdb_commons::schemas::TableType;
