//! System tables module
//!
//! All system tables now use EntityStore-based implementations.

pub mod information_schema_columns;
pub mod information_schema_tables;
// EntityStore-based system tables (using SystemTableStore<K,V>)
pub mod jobs_v2;
pub mod live_queries_v2;
pub mod namespaces_v2;
pub mod storages_v2;
pub mod tables_v2;
pub mod users_v2;

// Export all v2 providers as the standard names (no _v2 suffix in public API)
pub use information_schema_columns::InformationSchemaColumnsProvider;
pub use information_schema_tables::InformationSchemaTablesProvider;
pub use jobs_v2::JobsTableProvider;
pub use live_queries_v2::LiveQueriesTableProvider;
pub use namespaces_v2::NamespacesTableProvider;
pub use storages_v2::StoragesTableProvider;
pub use tables_v2::TablesTableProvider;
pub use users_v2::UsersTableProvider;

