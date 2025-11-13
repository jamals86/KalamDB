//! System tables module
//!
//! System table providers are now in kalamdb-system crate.
//! This module re-exports them and provides kalamdb-core-specific extensions
//! like initialization and SystemTablesRegistry.

pub mod initialization; // Phase 10 (Phase 7): System schema versioning
pub mod registry; // Phase 5: SystemTablesRegistry - centralized provider access
pub mod stats; // Virtual view (depends on SchemaRegistry, stays in core)

// Re-export system_table_store and supporting modules from kalamdb-system
// This maintains compatibility for existing code in kalamdb-core
pub mod system_table_store {
    // Re-export SystemTableStore from kalamdb-system
    pub use kalamdb_system::system_table_store::SystemTableStore;
}

pub mod system_table_definitions {
    pub use kalamdb_system::system_table_definitions::*;
}

pub mod system_table_trait {
    pub use kalamdb_system::system_table_trait::*;
}

// Export initialization (Phase 10 Phase 7)
pub use initialization::initialize_system_tables;

// Export registry (Phase 5 completion)
pub use registry::SystemTablesRegistry;

// Re-export system table providers from kalamdb-system
pub use kalamdb_system::{
    AuditLogsTableProvider,
    JobsTableProvider,
    LiveQueriesTableProvider,
    NamespacesTableProvider,
    StoragesTableProvider,
    TablesTableProvider,
    UsersTableProvider,
    SystemTableProviderExt,
};

// Stats remains in kalamdb-core (depends on SchemaRegistry)
pub use stats::StatsTableProvider;
