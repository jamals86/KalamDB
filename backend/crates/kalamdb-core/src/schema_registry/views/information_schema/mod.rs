//! Information schema views
//!
//! Standard SQL information_schema views for table and column metadata.
//! All views follow the DataFusion VirtualView pattern.

mod information_schema_columns;
mod information_schema_tables;

pub use information_schema_columns::InformationSchemaColumnsProvider;
pub use information_schema_tables::InformationSchemaTablesProvider;
