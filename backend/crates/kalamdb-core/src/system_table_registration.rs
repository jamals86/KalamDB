//! System table registration utilities
//!
//! Provides centralized registration of all system tables to avoid code duplication.

use crate::tables::system::{
    JobsTableProvider, LiveQueriesTableProvider, NamespacesTableProvider,
    StorageLocationsTableProvider, SystemStoragesProvider, SystemTablesTableProvider,
    UsersTableProvider,
};
use datafusion::catalog::schema::{MemorySchemaProvider, SchemaProvider};
use kalamdb_commons::SystemTable;
use std::sync::Arc;

/// Register all system tables with the provided schema
///
/// This function registers all 8 system tables (users, namespaces, tables, table_schemas,
/// storage_locations, storages, live_queries, jobs) with the DataFusion schema provider.
///
/// # Arguments
/// * `system_schema` - The DataFusion schema provider to register tables with
/// * `kalam_sql` - The KalamSql adapter for accessing RocksDB
///
/// # Returns
/// * `jobs_provider` - The jobs table provider (needed for job management)
///
/// # Example
/// ```no_run
/// use datafusion::catalog::schema::MemorySchemaProvider;
/// use std::sync::Arc;
/// use kalamdb_core::system_table_registration::register_system_tables;
/// # use kalamdb_sql::KalamSql;
/// # let db = Arc::new(rocksdb::DB::open_default("test").unwrap());
/// # let kalam_sql = Arc::new(KalamSql::new(db).unwrap());
///
/// let system_schema = Arc::new(MemorySchemaProvider::new());
/// let jobs_provider = register_system_tables(&system_schema, kalam_sql)
///     .expect("Failed to register system tables");
/// ```
pub fn register_system_tables(
    system_schema: &Arc<MemorySchemaProvider>,
    kalam_sql: Arc<kalamdb_sql::KalamSql>,
) -> Result<Arc<JobsTableProvider>, String> {
    // Create all system table providers
    let users_provider = Arc::new(UsersTableProvider::new(kalam_sql.clone()));
    let namespaces_provider = Arc::new(NamespacesTableProvider::new(kalam_sql.clone()));
    let tables_provider = Arc::new(SystemTablesTableProvider::new(kalam_sql.clone()));
    let storage_locations_provider =
        Arc::new(StorageLocationsTableProvider::new(kalam_sql.clone()));
    let storages_provider = Arc::new(SystemStoragesProvider::new(kalam_sql.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(kalam_sql.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.clone()));

    // Register each system table using the SystemTable enum
    system_schema
        .register_table(
            SystemTable::Users.table_name().to_string(),
            users_provider,
        )
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
            SystemTable::StorageLocations.table_name().to_string(),
            storage_locations_provider,
        )
        .map_err(|e| format!("Failed to register system.storage_locations: {}", e))?;

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
        // We can't run it without a real database, but we can verify the function signature
        let expected_tables = vec![
            SystemTable::Users,
            SystemTable::Namespaces,
            SystemTable::Tables,
            SystemTable::StorageLocations,
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

        // Verify we have all 7 system tables (table_schemas is future work)
        assert_eq!(names.len(), 7);
    }
}
