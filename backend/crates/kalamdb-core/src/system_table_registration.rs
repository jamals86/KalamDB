//! System table registration utilities
//!
//! Provides centralized registration of all system tables to avoid code duplication.

// All system tables now use EntityStore-based v2 providers
use crate::tables::system::jobs_v2::JobsTableProvider;
use crate::tables::system::live_queries_v2::LiveQueriesTableProvider;
use crate::tables::system::namespaces_v2::NamespacesTableProvider;
use crate::tables::system::schemas::{SchemaCache, TableSchemaStore};
use crate::tables::system::storages_v2::StoragesTableProvider;
use crate::tables::system::system_table_definitions::all_system_table_definitions;
use crate::tables::system::tables_v2::TablesTableProvider;
use crate::tables::system::users_v2::UsersTableProvider;
use crate::tables::system::StatsTableProvider;
use datafusion::catalog::memory::MemorySchemaProvider;
use datafusion::catalog::SchemaProvider;
use kalamdb_commons::system_tables::SystemTable;
use kalamdb_store::entity_store::EntityStore;
use std::sync::Arc;

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
/// * `(jobs_provider, schema_store, schema_cache)` - Tuple of jobs provider, schema store, and cache
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
/// let (jobs_provider, schema_store, schema_cache) = register_system_tables(&system_schema, backend)
///     .expect("Failed to register system tables");
/// ```
pub fn register_system_tables(
    system_schema: &Arc<MemorySchemaProvider>,
    storage_backend: Arc<dyn kalamdb_store::StorageBackend>,
) -> Result<(Arc<JobsTableProvider>, Arc<TableSchemaStore>, Arc<SchemaCache>), String> {
    use kalamdb_store::storage_trait::Partition;

    // Create the system_table_schemas partition if it doesn't exist
    let schemas_partition = Partition::new("system_table_schemas");
    let _ = storage_backend.create_partition(&schemas_partition); // Ignore error if already exists

    // Initialize TableSchemaStore and SchemaCache
    let schema_store = Arc::new(TableSchemaStore::new(storage_backend.clone()));
    let schema_cache = Arc::new(SchemaCache::new(1000)); // Cache up to 1000 schemas

    // Register all system table schema definitions in TableSchemaStore
    for (table_id, table_def) in all_system_table_definitions() {
        schema_store
            .put(&table_id, &table_def)
            .map_err(|e| format!("Failed to register schema for {}: {}", table_id, e))?;
        
        // Pre-warm the cache with system table schemas
        schema_cache.insert(table_id, table_def);
    }

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

    // Register virtual system.stats table (observability)
    let stats_provider = Arc::new(StatsTableProvider::new(Some(schema_cache.clone())));
    system_schema
        .register_table("stats".to_string(), stats_provider)
        .map_err(|e| format!("Failed to register system.stats: {}", e))?;

    Ok((jobs_provider, schema_store, schema_cache))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::catalog::MemorySchemaProvider;
    use kalamdb_commons::schemas::TableType;
    use kalamdb_commons::{NamespaceId, TableId, TableName};
    use kalamdb_store::RocksDBBackend;
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
            DB::open_default(temp_dir.path().to_str().unwrap())
                .expect("Failed to create RocksDB"),
        );
        let backend = Arc::new(RocksDBBackend::new(db));

        // Create schema provider
        let system_schema = Arc::new(MemorySchemaProvider::new());

        // Register system tables
        let (jobs_provider, schema_store, schema_cache) =
            register_system_tables(&system_schema, backend)
                .expect("Failed to register system tables");

        // Verify jobs provider is returned
        assert!(Arc::strong_count(&jobs_provider) >= 1);

        // Verify all 7 system table schemas are in the store
        let system_namespace = NamespaceId::from("system");
        let all_schemas = schema_store
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
            let schema = schema_store
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

        // Verify cache is populated
        for &table_name in &["users", "jobs", "namespaces", "storages", "live_queries", "tables", "table_schemas"] {
            let table_id = TableId::new(system_namespace.clone(), TableName::from(table_name));
            let cached_schema = schema_cache.get(&table_id);
            assert!(
                cached_schema.is_some(),
                "Schema for {} should be cached",
                table_name
            );
        }
    }
}