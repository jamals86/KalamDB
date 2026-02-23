//! System.schemas table module (system_schemas in RocksDB)
//!
//! Phase 16: Consolidated single store using TableVersionId keys.
//! Stores both latest table definitions and version history in one partition.
//!
//! This module contains all components for the system.schemas table:
//! - Canonical table definition + cached Arrow schema helpers
//! - SchemasStore wrapper with versioning support
//! - TableProvider for DataFusion integration

mod schemas_definition;
pub mod schemas_provider;
pub mod schemas_store;

pub use schemas_definition::{schemas_arrow_schema, schemas_table_definition};
pub use schemas_provider::SchemasTableProvider;
pub use schemas_store::{new_schemas_store, SchemasStore};
