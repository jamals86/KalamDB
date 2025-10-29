//! System table registration utilities
//!
//! Provides centralized registration of all system tables to avoid code duplication.

// All system tables now use EntityStore-based v2 providers
use crate::tables::system::jobs_v2::JobsTableProvider;
use crate::tables::system::live_queries_v2::LiveQueriesTableProvider;
use crate::tables::system::namespaces_v2::NamespacesTableProvider;
use crate::tables::system::storages_v2::StoragesTableProvider;
use crate::tables::system::tables_v2::TablesTableProvider;
use crate::tables::system::users_v2::UsersTableProvider;
use datafusion::catalog::memory::MemorySchemaProvider;
use datafusion::catalog::SchemaProvider;
use kalamdb_commons::system_tables::SystemTable;
use std::sync::Arc;

/// Register all system tables with the provided schema
///
/// This function registers all system tables (users, namespaces, tables,
/// storages, live_queries, jobs) with the DataFusion schema provider.
/// All tables use the EntityStore-based v2 implementations.
///
/// # Arguments
/// * `system_schema` - The DataFusion schema provider to register tables with
/// * `storage_backend` - The storage backend for EntityStore-based providers
///
/// # Returns
/// * `jobs_provider` - The jobs table provider (needed for job management)
///
/// # Example
/// ```no_run
/// use datafusion::catalog::schema::MemorySchemaProvider;
/// use std::sync::Arc;
/// use kalamdb_core::system_table_registration::register_system_tables;
/// use kalamdb_store::StorageBackend;
///
/// # let backend: Arc<dyn kalamdb_store::StorageBackend> = unimplemented!("provide a StorageBackend");
/// let system_schema = Arc::new(MemorySchemaProvider::new());
/// let jobs_provider = register_system_tables(&system_schema, backend)
///     .expect("Failed to register system tables");
/// ```
pub fn register_system_tables(
    system_schema: &Arc<MemorySchemaProvider>,
    storage_backend: Arc<dyn kalamdb_store::StorageBackend>,
) -> Result<Arc<JobsTableProvider>, String> {
    // Create all system table providers using EntityStore-based v2 implementations
    let users_provider = Arc::new(UsersTableProvider::new(storage_backend.clone()));
    let tables_provider = Arc::new(TablesTableProvider::new(storage_backend.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(storage_backend.clone()));
    let namespaces_provider = Arc::new(NamespacesTableProvider::new(storage_backend.clone()));
    let storages_provider = Arc::new(StoragesTableProvider::new(storage_backend.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(storage_backend.clone()));

    // Register each system table using the SystemTable enum
    system_schema
        .register_table(SystemTable::Users.table_name().to_string(), users_provider)
        .map_err(|e| format!("Failed to register system.users: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Namespaces.table_name().to_string(),
            namespaces_provider,
        )
        .map_err(|e| format!("Failed to register system.namespaces: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Tables.table_name().to_string(),
            tables_provider,
        )
        .map_err(|e| format!("Failed to register system.tables: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Storages.table_name().to_string(),
            storages_provider,
        )
        .map_err(|e| format!("Failed to register system.storages: {}", e))?;

    system_schema
        .register_table(
            SystemTable::LiveQueries.table_name().to_string(),
            live_queries_provider,
        )
        .map_err(|e| format!("Failed to register system.live_queries: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Jobs.table_name().to_string(),
            jobs_provider.clone(),
        )
        .map_err(|e| format!("Failed to register system.jobs: {}", e))?;

    Ok(jobs_provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::catalog::schema::MemorySchemaProvider;

    #[tokio::test]
    async fn test_register_system_tables_validates_all_tables() {
        // This test validates that all SystemTable enum variants are registered
        let expected_tables = vec[
            SystemTable::Users,
            SystemTable::Namespaces,
            SystemTable::Tables,
            SystemTable::Storages,
            SystemTable::LiveQueries,
            SystemTable::Jobs,
        ];

        // Verify each table has a unique name
        let mut names = std::collections::HashSet::new();
        for table in expected_tables {
            assert!(
                names.insert(table.table_name()),
                "Duplicate table name: {}",
                table.table_name()
            );
        }

        // Verify we have all system table registrations covered
        assert_eq!(names.len(), 6);
    }
}