//! Schema Models
//!
//! This module provides the single source of truth for all table schema definitions.
//! It consolidates TableDefinition, ColumnDefinition, SchemaVersion models.

pub mod column_default;
pub mod column_definition;
pub mod schema_version;
pub mod table_definition;
pub mod table_options;
pub mod table_type;

pub use column_default::ColumnDefault;
pub use column_definition::ColumnDefinition;
pub use schema_version::SchemaVersion;
pub use table_definition::TableDefinition;
pub use table_options::{
    SharedTableOptions, StreamTableOptions, SystemTableOptions, TableOptions, UserTableOptions,
};
pub use table_type::TableType;
