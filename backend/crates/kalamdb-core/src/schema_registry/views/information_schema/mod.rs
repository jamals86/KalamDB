//! Information schema views
//!
//! Standard SQL information_schema views for table and column metadata.
//! All views follow the DataFusion VirtualView pattern.

mod information_schema_columns;
mod information_schema_tables;

// Export view structs
pub use information_schema_columns::{InformationSchemaColumnsView, create_information_schema_columns_provider};
pub use information_schema_tables::{InformationSchemaTablesView, create_information_schema_tables_provider};

// Backward compatibility (deprecated)
#[allow(deprecated)]
pub use information_schema_columns::InformationSchemaColumnsProvider;
