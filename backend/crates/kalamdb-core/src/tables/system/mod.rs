//! System tables module

pub mod information_schema_tables;
pub mod jobs;
pub mod jobs_provider;
pub mod live_queries;
pub mod live_queries_provider;
pub mod storage_locations;
pub mod storage_locations_provider;
pub mod users;
pub mod users_provider;

pub use information_schema_tables::InformationSchemaTablesProvider;
pub use jobs::JobsTable;
pub use jobs_provider::{JobRecord, JobsTableProvider};
pub use live_queries::LiveQueriesTable;
pub use live_queries_provider::{LiveQueriesTableProvider, LiveQueryRecord};
pub use storage_locations::StorageLocationsTable;
pub use storage_locations_provider::{StorageLocationRecord, StorageLocationsTableProvider};
pub use users::UsersTable;
pub use users_provider::{UserRecord, UsersTableProvider};

/// Register all system tables with DataFusion
pub fn register_system_tables() {
    // TODO: Implement registration logic when integrating with DataFusion
    // This will register system.users, system.live_queries, system.storage_locations, system.jobs
}
