//! Unified schema registry module

pub mod core;
pub mod tables_adapter;

pub use core::{SchemaRegistry, TableEntry};
pub use tables_adapter::TablesSchemaRegistryAdapter;
