//! Information schema views
//!
//! Standard SQL information_schema views for table and column metadata.

mod information_schema_columns;
mod information_schema_tables;

pub use information_schema_columns::InformationSchemaColumnsProvider;
pub use information_schema_tables::InformationSchemaTablesProvider;
