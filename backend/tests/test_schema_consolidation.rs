//! Integration tests for Phase 15 (008-schema-consolidation)
//!
//! Tests the consolidated schema infrastructure:
//! - TableSchemaStore persistence
//! - SchemaCache performance
//! - Schema versioning

use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableType};
use kalamdb_commons::{NamespaceId, TableId, TableName};
use kalamdb_core::system_table_registration::register_system_tables;
use kalamdb_store::RocksDBBackend;
use rocksdb::DB;
use std::sync::Arc;
use tempfile::TempDir;

use datafusion::arrow::array::AsArray;
use datafusion::arrow::datatypes::DataType;
use kalamdb_commons::{NamespaceId, TableId, TableName};
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::sql::ExecutionResult;
use kalamdb_core::system_table_registration::register_system_tables;
use kalamdb_store::RocksDBBackend;
use rocksdb::DB;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test SqlExecutor with schema infrastructure
async fn create_test_executor() -> (SqlExecutor, TempDir, Arc<dyn kalamdb_store::StorageBackend>) {
    use datafusion::catalog::memory::MemorySchemaProvider;
    use datafusion::prelude::SessionContext;
    use kalamdb_core::services::{
        NamespaceService, SharedTableService, StreamTableService, UserTableService,
    };

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    // Register system tables and get schema infrastructure
    let system_schema = Arc::new(MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, schema_cache) =
        register_system_tables(&system_schema, backend.clone())
            .expect("Failed to register system tables");

    // Create services
    let session_context = Arc::new(SessionContext::new());
    let namespace_service = Arc::new(NamespaceService::new(backend.clone()));
    let user_table_service = Arc::new(UserTableService::new(backend.clone()));
    let shared_table_service = Arc::new(SharedTableService::new(backend.clone()));
    let stream_table_service = Arc::new(StreamTableService::new(backend.clone()));

    // Create SqlExecutor with schema infrastructure
    let executor = SqlExecutor::new(
        namespace_service,
        session_context,
        user_table_service,
        shared_table_service,
        stream_table_service,
    )
    .with_schema_infrastructure(schema_store, schema_cache)
    .with_storage_backend(backend.clone());

    (executor, temp_dir, backend)
}

#[tokio::test]
async fn test_describe_table_returns_column_schema() {
    let (executor, _temp_dir, _backend) = create_test_executor().await;

    // DESCRIBE a system table (users) which has schema defined
    let result = executor
        .execute("DESCRIBE TABLE system.users")
        .await
        .expect("Failed to execute DESCRIBE TABLE");

    match result {
        ExecutionResult::RecordBatch(batch) => {
            // Verify schema has expected columns
            let schema = batch.schema();
            assert_eq!(schema.fields().len(), 8, "Should have 8 columns in output");

            let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
            assert_eq!(
                field_names,
                vec![
                    "column_name",
                    "ordinal_position",
                    "data_type",
                    "is_nullable",
                    "is_primary_key",
                    "is_partition_key",
                    "default_value",
                    "column_comment"
                ],
                "Schema should match expected column definition format"
            );

            // Verify data types
            assert_eq!(schema.field(0).data_type(), &DataType::Utf8); // column_name
            assert_eq!(schema.field(1).data_type(), &DataType::Int32); // ordinal_position
            assert_eq!(schema.field(2).data_type(), &DataType::Utf8); // data_type
            assert_eq!(schema.field(3).data_type(), &DataType::Boolean); // is_nullable
            assert_eq!(schema.field(4).data_type(), &DataType::Boolean); // is_primary_key
            assert_eq!(schema.field(5).data_type(), &DataType::Boolean); // is_partition_key

            // Verify we have rows (system.users should have 8 columns defined)
            assert!(
                batch.num_rows() > 0,
                "Should have at least one column in system.users"
            );

            // Verify first column data
            let column_names = batch.column(0).as_string::<i32>();
            assert!(
                column_names.iter().any(|name| name == Some("user_id")),
                "Should include user_id column"
            );
            assert!(
                column_names.iter().any(|name| name == Some("username")),
                "Should include username column"
            );

            println!("✅ DESCRIBE TABLE returns correct column schema format");
        }
        _ => panic!("Expected RecordBatch result from DESCRIBE TABLE"),
    }
}

#[tokio::test]
async fn test_describe_table_history_shows_versions() {
    let (executor, _temp_dir, _backend) = create_test_executor().await;

    // DESCRIBE TABLE HISTORY should show schema versions
    let result = executor
        .execute("DESCRIBE TABLE system.users HISTORY")
        .await
        .expect("Failed to execute DESCRIBE TABLE HISTORY");

    match result {
        ExecutionResult::RecordBatch(batch) => {
            // Verify schema has expected columns for history
            let schema = batch.schema();
            assert_eq!(
                schema.fields().len(),
                4,
                "History output should have 4 columns"
            );

            let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
            assert_eq!(
                field_names,
                vec!["version", "created_at", "changes", "column_count"],
                "History schema should match expected format"
            );

            // Verify data types
            assert_eq!(schema.field(0).data_type(), &DataType::Int32); // version
            assert_eq!(schema.field(1).data_type(), &DataType::Int64); // created_at
            assert_eq!(schema.field(2).data_type(), &DataType::Utf8); // changes
            assert_eq!(schema.field(3).data_type(), &DataType::Int32); // column_count

            println!("✅ DESCRIBE TABLE HISTORY returns correct version history format");
        }
        _ => panic!("Expected RecordBatch result from DESCRIBE TABLE HISTORY"),
    }
}

#[tokio::test]
async fn test_schema_store_persistence() {
    use kalamdb_core::tables::system::schemas::TableSchemaStore;
    use kalamdb_store::entity_store::EntityStore;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    // Register system tables - this populates the schema store
    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, _schema_cache) =
        register_system_tables(&system_schema, backend.clone())
            .expect("Failed to register system tables");

    // Verify all 7 system table schemas are persisted
    let system_namespace = NamespaceId::from("system");
    let all_schemas = schema_store
        .scan_namespace(&system_namespace)
        .expect("Failed to scan namespace");

    assert_eq!(
        all_schemas.len(),
        7,
        "Should have 7 system table schemas"
    );

    // Verify specific table schemas
    let table_names = vec![
        "users",
        "jobs",
        "namespaces",
        "storages",
        "live_queries",
        "tables",
        "table_schemas",
    ];

    for table_name in table_names {
        let table_id = TableId::new(system_namespace.clone(), TableName::from(table_name));
        let schema = schema_store
            .get(&table_id)
            .expect("Failed to get schema")
            .unwrap_or_else(|| panic!("Schema not found for {}", table_name));

        assert!(
            !schema.columns.is_empty(),
            "Table {} should have columns",
            table_name
        );
        assert!(
            !schema.schema_history.is_empty(),
            "Table {} should have schema history",
            table_name
        );

        println!("✅ Schema persisted for system.{}", table_name);
    }
}

#[tokio::test]
async fn test_schema_cache_hit_rate() {
    use kalamdb_core::tables::system::schemas::SchemaCache;

    let cache = Arc::new(SchemaCache::new(100));
    let namespace = NamespaceId::from("test");

    // Insert 10 schemas
    for i in 0..10 {
        let table_id = TableId::new(namespace.clone(), TableName::from(format!("table{}", i)));
        let table_def = kalamdb_commons::schemas::TableDefinition::new_with_defaults(
            table_id.clone(),
            vec![],
            kalamdb_commons::schemas::TableType::User,
        );
        cache.insert(table_id, table_def);
    }

    // Query each schema 10 times
    for _ in 0..10 {
        for i in 0..10 {
            let table_id = TableId::new(namespace.clone(), TableName::from(format!("table{}", i)));
            let schema = cache.get(&table_id);
            assert!(schema.is_some(), "Schema should be cached");
        }
    }

    // Check cache statistics
    let stats = cache.stats();
    assert_eq!(stats.size, 10, "Cache should have 10 entries");
    assert_eq!(stats.hits, 100, "Should have 100 cache hits");
    assert_eq!(stats.misses, 0, "Should have 0 cache misses");

    let hit_rate = cache.hit_rate();
    assert_eq!(hit_rate, 100.0, "Hit rate should be 100%");

    println!(
        "✅ Cache performance: {} hits, {} misses, {:.2}% hit rate",
        stats.hits, stats.misses, hit_rate
    );
}

#[tokio::test]
async fn test_namespace_isolation() {
    use kalamdb_core::tables::system::schemas::TableSchemaStore;
    use kalamdb_store::entity_store::EntityStore;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    let schema_store = Arc::new(TableSchemaStore::new(backend.clone()));

    // Create schemas in different namespaces
    let ns1 = NamespaceId::from("app1");
    let ns2 = NamespaceId::from("app2");

    let table1_id = TableId::new(ns1.clone(), TableName::from("users"));
    let table2_id = TableId::new(ns2.clone(), TableName::from("users"));

    let table1_def = kalamdb_commons::schemas::TableDefinition::new_with_defaults(
        table1_id.clone(),
        vec![],
        kalamdb_commons::schemas::TableType::User,
    );
    let table2_def = kalamdb_commons::schemas::TableDefinition::new_with_defaults(
        table2_id.clone(),
        vec![],
        kalamdb_commons::schemas::TableType::User,
    );

    schema_store
        .put(&table1_id, &table1_def)
        .expect("Failed to put table1");
    schema_store
        .put(&table2_id, &table2_def)
        .expect("Failed to put table2");

    // Scan each namespace separately
    let ns1_schemas = schema_store
        .scan_namespace(&ns1)
        .expect("Failed to scan ns1");
    let ns2_schemas = schema_store
        .scan_namespace(&ns2)
        .expect("Failed to scan ns2");

    assert_eq!(ns1_schemas.len(), 1, "ns1 should have 1 table");
    assert_eq!(ns2_schemas.len(), 1, "ns2 should have 1 table");

    // Verify correct tables in each namespace
    assert_eq!(ns1_schemas[0].0, table1_id);
    assert_eq!(ns2_schemas[0].0, table2_id);

    println!("✅ Namespace isolation verified");
}
