//! System tables module

pub mod jobs;
pub mod live_queries;
pub mod storage_locations;
pub mod users;

pub use jobs::JobsTable;
pub use live_queries::LiveQueriesTable;
pub use storage_locations::StorageLocationsTable;
pub use users::UsersTable;

/// Register all system tables with DataFusion
pub fn register_system_tables() {
    // TODO: Implement registration logic when integrating with DataFusion
    // This will register system.users, system.live_queries, system.storage_locations, system.jobs
}
