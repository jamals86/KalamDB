//! Schema Registry for KalamDB
//!
//! Provides unified caching and metadata management for table schemas.
//!
//! **Architecture**:
//! - Single DashMap<TableId, Arc<CachedTableData>> for all table metadata + schemas
//! - Arrow schema memoization for 50-100× speedup on repeated access
//! - Lock-free concurrent access via DashMap
//! - LRU eviction policy (configurable max_size)

pub mod arrow_schema;
pub mod projection;
pub mod registry;
pub mod system_columns_metadata;
pub mod system_columns_service;
pub mod traits;
pub mod views;

mod error;

pub use arrow_schema::ArrowSchemaWithOptions;
pub use error::RegistryError;
pub use projection::{project_batch, schemas_compatible};
pub use registry::{CachedTableData, SchemaRegistry};
pub use system_columns_metadata::SystemColumns;
pub use system_columns_service::SystemColumnsService;

// Re-export common types from kalamdb_commons for convenience
pub use kalamdb_commons::models::{NamespaceId, TableName, UserId};
pub use kalamdb_commons::schemas::TableType;
