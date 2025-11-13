//! Schema Registry module - Re-exports from kalamdb-registry
//!
//! **Migration Notice**: This module now re-exports from the `kalamdb-registry` crate.
//! The schema registry has been extracted to a separate crate to:
//! - Enable reuse across kalamdb-system and kalamdb-tables
//! - Break circular dependencies
//! - Provide a clean foundational layer for caching
//!
//! **Phase 10 Complete**: Unified SchemaRegistry replaces old dual-cache architecture
//! **Phase 14**: Extracted to kalamdb-registry crate
//!
//! For backwards compatibility, all types are re-exported here.

// Re-export everything from kalamdb-registry
pub use kalamdb_registry::{
    arrow_schema, projection, system_columns_metadata, system_columns_service, views,
    ArrowSchemaWithOptions, CachedTableData, SchemaRegistry, SystemColumns, SystemColumnsService,
};

// Also re-export the submodules directly for code that uses `use crate::schema_registry::registry::*`
pub use kalamdb_registry::registry;

// Re-export helper functions
pub use kalamdb_registry::{project_batch, schemas_compatible};

// Re-export common types from kalamdb_commons for convenience
pub use kalamdb_commons::models::{NamespaceId, TableName, UserId};
pub use kalamdb_commons::schemas::TableType;
