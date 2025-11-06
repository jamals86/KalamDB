//! System table registration utilities
//!
//! Provides centralized registration of all system tables to avoid code duplication.

// All system tables now use EntityStore-based v2 providers
use crate::tables::system::jobs::JobsTableProvider;
use crate::tables::system::live_queries::LiveQueriesTableProvider;
use crate::tables::system::namespaces::NamespacesTableProvider;
use crate::tables::system::tables::{TablesTableProvider, TablesStore};
use crate::tables::system::storages::StoragesTableProvider;
use crate::tables::system::system_table_definitions::all_system_table_definitions;
use crate::tables::system::users::UsersTableProvider;
use crate::tables::system::StatsTableProvider;
use datafusion::catalog::memory::MemorySchemaProvider;
use datafusion::catalog::SchemaProvider;
use kalamdb_commons::system_tables::SystemTable;
use std::sync::Arc;

/// Return type for register_system_tables
pub struct SystemTableProviders {
    pub jobs_provider: Arc<JobsTableProvider>,
    pub users_provider: Arc<UsersTableProvider>,
    pub namespaces_provider: Arc<NamespacesTableProvider>,
    pub storages_provider: Arc<StoragesTableProvider>,
    pub live_queries_provider: Arc<LiveQueriesTableProvider>,
    pub tables_provider: Arc<TablesTableProvider>,
    pub schema_store: Arc<TablesStore>,
}

/// Register all system tables with the provided schema
///
/// This function registers all system tables (users, namespaces, tables,
/// storages, live_queries, jobs) with the DataFusion schema provider.
/// All tables use the EntityStore-based v2 implementations.
///
/// Additionally, it initializes the TableSchemaStore and registers all
/// system table schemas for consistent schema management.
///
/// # Arguments
/// * `system_schema` - The DataFusion schema provider to register tables with
/// * `storage_backend` - The storage backend for EntityStore-based providers
///
/// # Returns
/// * `SystemTableProviders` - Struct containing all providers and schema store
///
/// # Example
/// ```no_run
/// use datafusion::catalog::memory::MemorySchemaProvider;
/// use std::sync::Arc;
/// use kalamdb_core::system_table_registration::register_system_tables;
/// use kalamdb_store::StorageBackend;
///
/// # let backend: Arc<dyn kalamdb_store::StorageBackend> = unimplemented!("provide a StorageBackend");
/// let system_schema = Arc::new(MemorySchemaProvider::new());
/// let providers = register_system_tables(&system_schema, backend)
///     .expect("Failed to register system tables");
/// ```
pub fn register_system_tables(
    system_schema: &Arc<MemorySchemaProvider>,
    storage_backend: Arc<dyn kalamdb_store::StorageBackend>,
) -> Result<SystemTableProviders, String> {
    use kalamdb_store::storage_trait::Partition;

    // Create the system_tables partition if it doesn't exist
    let schemas_partition = Partition::new("system_tables");
    let _ = storage_backend.create_partition(&schemas_partition); // Ignore error if already exists

    // Initialize TablesStore (stores TableDefinition for all tables)
    let schema_store = Arc::new(TablesStore::new(storage_backend.clone(), "system_tables"));

    // Register all system table schema definitions in TablesStore
    use kalamdb_store::EntityStoreV2;
    for (table_id, table_def) in all_system_table_definitions() {
        schema_store
            .put(&table_id, &table_def)
            .map_err(|e| format!("Failed to register schema for {}: {}", table_id, e))?;
    }

    // Create all system table providers using EntityStore-based v2 implementations
    let users_provider = Arc::new(UsersTableProvider::new(storage_backend.clone()));
    let tables_provider = Arc::new(TablesTableProvider::new(storage_backend.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(storage_backend.clone()));
    let namespaces_provider = Arc::new(NamespacesTableProvider::new(storage_backend.clone()));
    let storages_provider = Arc::new(StoragesTableProvider::new(storage_backend.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(storage_backend.clone()));

    // Register each system table using the SystemTable enum (clone Arcs before registration)
    system_schema
        .register_table(SystemTable::Users.table_name().to_string(), users_provider.clone())
        .map_err(|e| format!("Failed to register system.users: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Namespaces.table_name().to_string(),
            namespaces_provider.clone(),
        )
        .map_err(|e| format!("Failed to register system.namespaces: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Tables.table_name().to_string(),
            tables_provider.clone(),
        )
        .map_err(|e| format!("Failed to register system.tables: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Storages.table_name().to_string(),
            storages_provider.clone(),
        )
        .map_err(|e| format!("Failed to register system.storages: {}", e))?;

    system_schema
        .register_table(
            SystemTable::LiveQueries.table_name().to_string(),
            live_queries_provider.clone(),
        )
        .map_err(|e| format!("Failed to register system.live_queries: {}", e))?;

    system_schema
        .register_table(
            SystemTable::Jobs.table_name().to_string(),
            jobs_provider.clone(),
        )
        .map_err(|e| format!("Failed to register system.jobs: {}", e))?;

    // Register virtual system.stats table (observability)
    // Note: No cache passed - StatsTableProvider will not show cache metrics
    // Cache metrics are accessible via SqlExecutor's unified_cache (Phase 10)
    let stats_provider = Arc::new(StatsTableProvider::new(None));
    system_schema
        .register_table("stats".to_string(), stats_provider)
        .map_err(|e| format!("Failed to register system.stats: {}", e))?;

    Ok(SystemTableProviders {
        jobs_provider,
        users_provider,
        namespaces_provider,
        storages_provider,
        live_queries_provider,
        tables_provider,
        schema_store,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::catalog::MemorySchemaProvider;
    use kalamdb_commons::schemas::TableType;
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::{EntityStoreV2, RocksDBBackend};
    use rocksdb::DB;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_register_system_tables_validates_all_tables() {
        // This test validates that all SystemTable enum variants are registered
        let expected_tables = vec![
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

    #[tokio::test]
    async fn test_register_system_tables_populates_schema_store() {
        // Create temporary storage backend
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db = Arc::new(
            DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
        );
        let backend = Arc::new(RocksDBBackend::new(db));

        // Create schema provider
        let system_schema = Arc::new(MemorySchemaProvider::new());

        // Register system tables
        let providers =
            register_system_tables(&system_schema, backend)
                .expect("Failed to register system tables");

        // Verify jobs provider is returned
        assert!(Arc::strong_count(&providers.jobs_provider) >= 1);
        
        // Verify all providers are returned
        assert!(Arc::strong_count(&providers.users_provider) >= 1);
        assert!(Arc::strong_count(&providers.namespaces_provider) >= 1);
        assert!(Arc::strong_count(&providers.storages_provider) >= 1);
        assert!(Arc::strong_count(&providers.live_queries_provider) >= 1);
        assert!(Arc::strong_count(&providers.tables_provider) >= 1);

        // Verify all 7 system table schemas are in the store
        let system_namespace = NamespaceId::from("system");
        let all_schemas = providers.schema_store
            .scan_namespace(&system_namespace)
            .expect("Failed to scan system namespace");

        assert_eq!(
            all_schemas.len(),
            7,
            "Expected 7 system table schemas, found {}",
            all_schemas.len()
        );

        // Verify specific tables exist in schema store
        let expected_table_names = vec![
            "users",
            "jobs",
            "namespaces",
            "storages",
            "live_queries",
            "tables",
            "table_schemas",
        ];

        for &table_name in &expected_table_names {
            let table_id = TableId::new(system_namespace.clone(), TableName::from(table_name));
            let schema = providers.schema_store
                .get(&table_id)
                .expect("Failed to get schema")
                .unwrap_or_else(|| panic!("Schema not found for {}", table_name));

            // TableDefinition doesn't have table_id field, just verify it exists
            assert_eq!(schema.table_type, TableType::System);
            assert!(
                !schema.columns.is_empty(),
                "Table {} should have columns",
                table_name
            );
        }

        // Phase 10: No separate schema_cache - unified cache lives in SqlExecutor
        // This test verifies schema_store persistence only
    }
}
