//! System tables module
//!
//! System table providers are now in kalamdb-system crate.
//! This module re-exports them for backward compatibility.

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

// Re-export system table providers from kalamdb-system
pub use kalamdb_system::{
    AuditLogsTableProvider,
    JobsTableProvider,
    LiveQueriesTableProvider,
    NamespacesTableProvider,
    StoragesTableProvider,
    StatsTableProvider,
    SystemTablesRegistry,
    TablesTableProvider,
    UsersTableProvider,
    SystemTableProviderExt,
};
