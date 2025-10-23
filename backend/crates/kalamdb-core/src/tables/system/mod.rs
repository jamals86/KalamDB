//! System tables module

pub mod base_provider;
pub mod information_schema_tables;
pub mod jobs;
pub mod jobs_provider;
pub mod live_queries;
pub mod live_queries_provider;
pub mod namespaces;
pub mod namespaces_provider;
pub mod storage_locations;
pub mod storage_locations_provider;
pub mod storages;
pub mod storages_provider;
pub mod system_tables;
pub mod system_tables_provider;
pub mod users;
pub mod users_provider;

pub use base_provider::SystemTableProviderExt;
pub use information_schema_tables::InformationSchemaTablesProvider;
pub use jobs::JobsTable;
pub use jobs_provider::{JobRecord, JobsTableProvider};
pub use live_queries::LiveQueriesTable;
pub use live_queries_provider::{LiveQueriesTableProvider, LiveQueryRecord};
pub use namespaces::NamespacesTable;
pub use namespaces_provider::NamespacesTableProvider;
pub use storage_locations::StorageLocationsTable;
pub use storage_locations_provider::{StorageLocationRecord, StorageLocationsTableProvider};
pub use storages::SystemStorages;
pub use storages_provider::SystemStoragesProvider;
pub use system_tables::SystemTables;
pub use system_tables_provider::SystemTablesTableProvider;
pub use users::UsersTable;
pub use users_provider::{UserRecord, UsersTableProvider};
