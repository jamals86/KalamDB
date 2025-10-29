//! System tables module

pub mod base_provider;
pub mod information_schema_columns;
pub mod information_schema_tables;
pub mod jobs;
pub mod jobs_provider;
pub mod live_queries;
pub mod live_queries_provider;
pub mod namespaces;
pub mod namespaces_provider;
pub mod storages;
pub mod storages_provider;
pub mod system_tables;
pub mod system_tables_provider;
pub mod table_schemas;
pub mod table_schemas_provider;
// Old users module (to be deprecated)
pub mod users;
pub mod users_provider;
// New EntityStore-based users module (Phase 14)
// Note: Temporarily using 'users_v2' name to avoid conflict during migration
pub mod users_v2;
// New EntityStore-based tables module (Phase 14)
pub mod tables_v2;
// New EntityStore-based jobs module (Phase 14)
pub mod jobs_v2;

pub use base_provider::SystemTableProviderExt;
pub use information_schema_columns::InformationSchemaColumnsProvider;
pub use information_schema_tables::InformationSchemaTablesProvider;
pub use jobs::JobsTable;
// Export the v2 provider as the main JobsTableProvider
pub use jobs_v2::JobsTableProvider;
pub use live_queries::LiveQueriesTable;
pub use live_queries_provider::LiveQueriesTableProvider;
pub use namespaces::NamespacesTable;
pub use namespaces_provider::NamespacesTableProvider;
pub use storages::SystemStorages;
pub use storages_provider::SystemStoragesProvider;
pub use system_tables::SystemTables;
pub use system_tables_provider::SystemTablesTableProvider;
pub use table_schemas::TableSchemasTable;
pub use table_schemas_provider::TableSchemasProvider;
pub use users::UsersTable;
pub use users_provider::{UserRecord, UsersTableProvider};
