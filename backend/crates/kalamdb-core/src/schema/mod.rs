//! Schema module for Arrow schema management
//!
//! This module handles Arrow schema serialization, versioning, and storage.

pub mod arrow_schema;
pub mod manifest;
pub mod storage;
pub mod system_columns;
pub mod versioning;

pub use arrow_schema::ArrowSchemaWithOptions;
pub use manifest::{SchemaManifest, VersionHistoryEntry};
pub use storage::SchemaStorage;
pub use system_columns::SystemColumns;
pub use versioning::SchemaVersioning;
