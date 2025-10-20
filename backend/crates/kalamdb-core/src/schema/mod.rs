//! Schema module for Arrow schema management
//!
//! This module handles Arrow schema serialization and system column injection.
//! Schema versioning and storage are now managed in RocksDB via system_table_schemas CF.

pub mod arrow_schema;
pub mod projection;
pub mod system_columns;

pub use arrow_schema::ArrowSchemaWithOptions;
pub use projection::{project_batch, schemas_compatible};
pub use system_columns::SystemColumns;
