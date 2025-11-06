//! System tables module
//!
//! All system tables now use EntityStore-based implementations.

pub mod audit_logs;
pub mod initialization; // Phase 10 (Phase 7): System schema versioning
pub mod registry; // Phase 5: SystemTablesRegistry - centralized provider access
pub mod system_table_definitions; // Phase 15: System table schema definitions
pub mod system_table_trait;
pub mod system_table_store; // Colocated SystemTableStore (moved from stores/)
// EntityStore-based system tables (using SystemTableStore<K,V>)
pub mod jobs;
pub mod live_queries;
pub mod namespaces;
// schemas module removed - tables now handles TableDefinition storage
pub mod stats; // Virtual view (will move to schema_registry/views later)
pub mod storages;
pub mod tables;
pub mod users;

// Export initialization (Phase 10 Phase 7)
pub use initialization::initialize_system_tables;

// Export registry (Phase 5 completion)
pub use registry::SystemTablesRegistry;

// Export common trait
pub use system_table_trait::SystemTableProviderExt;
// Export all v2 providers as the standard names (no suffix in public API)
pub use audit_logs::AuditLogsTableProvider;
pub use stats::StatsTableProvider; // Virtual view
pub use jobs::JobsTableProvider;
pub use live_queries::LiveQueriesTableProvider;
pub use namespaces::NamespacesTableProvider;
pub use storages::StoragesTableProvider;
pub use tables::TablesTableProvider;
pub use users::UsersTableProvider;
