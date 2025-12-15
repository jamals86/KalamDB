//! System.tables table module (system_tables in RocksDB)
//!
//! Phase 16: Consolidated single store using TableVersionId keys.
//! Stores both latest table definitions and version history in one partition.
//!
//! This module contains all components for the system.tables table:
//! - Table schema definition with OnceLock caching
//! - SystemTableStore wrapper with versioning support
//! - TableProvider for DataFusion integration

pub mod tables_provider;
pub mod tables_store;
pub mod tables_table;

pub use tables_provider::TablesTableProvider;
pub use tables_store::{new_tables_store, TablesStore};
pub use tables_table::TablesTableSchema;
