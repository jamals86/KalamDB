//! System.schemas table module (system_schemas in RocksDB)
//!
//! Phase 16: Consolidated single store using TableVersionId keys.
//! Stores both latest table definitions and version history in one partition.
//!
//! This module contains all components for the system.schemas table:
//! - Table schema definition with OnceLock caching
//! - SystemTableStore wrapper with versioning support
//! - TableProvider for DataFusion integration

pub mod schemas_provider;
pub mod schemas_store;
pub mod schemas_table;

pub use schemas_provider::SchemasTableProvider;
pub use schemas_store::{new_schemas_store, SchemasStore};
pub use schemas_table::SchemasTableSchema;
