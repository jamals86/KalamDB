//! Schema Models
//!
//! This module provides the single source of truth for all table schema definitions.
//! It consolidates TableDefinition, ColumnDefinition, SchemaVersion models.

pub mod table_definition;
pub mod column_definition;
pub mod schema_version;
pub mod column_default;
pub mod table_type;
pub mod table_options;

pub use table_definition::TableDefinition;
pub use column_definition::ColumnDefinition;
pub use schema_version::SchemaVersion;
pub use column_default::ColumnDefault;
pub use table_type::TableType;
pub use table_options::{
    TableOptions, UserTableOptions, SharedTableOptions, 
    StreamTableOptions, SystemTableOptions
};
