//! System.tables table module (system_tables in RocksDB)
//!
//! This module contains all components for the system.tables table:
//! - Table schema definition with OnceLock caching
//! - SystemTableStore wrapper for type-safe storage
//! - TableProvider for DataFusion integration

pub mod tables_provider;
pub mod tables_store;
pub mod tables_table;

pub use tables_provider::TablesTableProvider;
pub use tables_store::{new_tables_store, TablesStore};
pub use tables_table::TablesTableSchema;
