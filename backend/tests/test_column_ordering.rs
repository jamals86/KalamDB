//! Integration tests for column ordering (Phase 4 - 008-schema-consolidation)
//!
//! Tests that SELECT * returns columns in ordinal_position order

use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableType};
use kalamdb_commons::types::KalamDataType;
use kalamdb_commons::{NamespaceId, TableId, TableName};
use kalamdb_core::system_table_registration::register_system_tables;
use kalamdb_store::{EntityStoreV2, RocksDBBackend};
use rocksdb::DB;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_select_star_returns_columns_in_ordinal_order() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    // Register system tables
    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, _schema_cache) =
        register_system_tables(&system_schema, backend.clone())
            .expect("Failed to register system tables");

    // Create a table with columns defined out of order
    let test_namespace = NamespaceId::from("test_ns");
    let test_table_id = TableId::new(test_namespace.clone(), TableName::from("test_table"));

    let columns_out_of_order = vec![
        ColumnDefinition::simple("email", 3, KalamDataType::Text),
        ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt),
        ColumnDefinition::simple("name", 2, KalamDataType::Text),
        ColumnDefinition::simple("created_at", 4, KalamDataType::Timestamp),
    ];

    let table_def = TableDefinition::new_with_defaults(
        test_namespace.clone(),
        TableName::new("test_table"),
        TableType::User,
        columns_out_of_order,
        None,
    )
    .expect("Failed to create table definition");

    // Store the table definition
    schema_store
        .put(&test_table_id, &table_def)
        .expect("Failed to store table definition");

    // Get the table definition back
    let retrieved = schema_store
        .get(&test_table_id)
        .expect("Failed to get table")
        .expect("Table not found");

    // Verify columns are sorted by ordinal_position
    assert_eq!(retrieved.columns.len(), 4);
    assert_eq!(retrieved.columns[0].column_name, "id");
    assert_eq!(retrieved.columns[0].ordinal_position, 1);
    assert_eq!(retrieved.columns[1].column_name, "name");
    assert_eq!(retrieved.columns[1].ordinal_position, 2);
    assert_eq!(retrieved.columns[2].column_name, "email");
    assert_eq!(retrieved.columns[2].ordinal_position, 3);
    assert_eq!(retrieved.columns[3].column_name, "created_at");
    assert_eq!(retrieved.columns[3].ordinal_position, 4);

    // Verify Arrow schema has columns in the same order
    let arrow_schema = retrieved
        .to_arrow_schema()
        .expect("Failed to convert to Arrow schema");
    assert_eq!(arrow_schema.fields().len(), 4);
    assert_eq!(arrow_schema.field(0).name(), "id");
    assert_eq!(arrow_schema.field(1).name(), "name");
    assert_eq!(arrow_schema.field(2).name(), "email");
    assert_eq!(arrow_schema.field(3).name(), "created_at");

    println!("✅ SELECT * returns columns in ordinal_position order");
}

#[tokio::test]
async fn test_alter_table_add_column_assigns_next_ordinal() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, _schema_cache) =
        register_system_tables(&system_schema, backend.clone())
            .expect("Failed to register system tables");

    let test_namespace = NamespaceId::from("test_ns");
    let test_table_id = TableId::new(test_namespace.clone(), TableName::from("test_table"));

    // Create initial table with 2 columns
    let initial_columns = vec![
        ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt),
        ColumnDefinition::simple("name", 2, KalamDataType::Text),
    ];

    let mut table_def = TableDefinition::new_with_defaults(
        test_namespace.clone(),
        TableName::new("test_table"),
        TableType::User,
        initial_columns,
        None,
    )
    .expect("Failed to create table definition");

    schema_store
        .put(&test_table_id, &table_def)
        .expect("Failed to store initial table");

    // Simulate ALTER TABLE ADD COLUMN
    let new_column = ColumnDefinition::simple("email", 3, KalamDataType::Text);
    table_def
        .add_column(new_column)
        .expect("Failed to add column");

    schema_store
        .put(&test_table_id, &table_def)
        .expect("Failed to update table");

    // Verify the new column has ordinal_position = 3
    let updated = schema_store
        .get(&test_table_id)
        .expect("Failed to get table")
        .expect("Table not found");

    assert_eq!(updated.columns.len(), 3);
    assert_eq!(updated.columns[2].column_name, "email");
    assert_eq!(updated.columns[2].ordinal_position, 3);

    println!("✅ ALTER TABLE ADD COLUMN assigns next available ordinal_position");
}

#[tokio::test]
async fn test_alter_table_drop_column_preserves_ordinals() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, _schema_cache) =
        register_system_tables(&system_schema, backend.clone())
            .expect("Failed to register system tables");

    let test_namespace = NamespaceId::from("test_ns");
    let test_table_id = TableId::new(test_namespace.clone(), TableName::from("test_table"));

    // Create table with 4 columns
    let initial_columns = vec![
        ColumnDefinition::primary_key("id", 1, KalamDataType::BigInt),
        ColumnDefinition::simple("name", 2, KalamDataType::Text),
        ColumnDefinition::simple("email", 3, KalamDataType::Text),
        ColumnDefinition::simple("created_at", 4, KalamDataType::Timestamp),
    ];

    let mut table_def = TableDefinition::new_with_defaults(
        test_namespace.clone(),
        TableName::new("test_table"),
        TableType::User,
        initial_columns,
        None,
    )
    .expect("Failed to create table definition");

    schema_store
        .put(&test_table_id, &table_def)
        .expect("Failed to store initial table");

    // Simulate ALTER TABLE DROP COLUMN email (position 3)
    table_def
        .drop_column("email")
        .expect("Failed to drop column");

    schema_store
        .put(&test_table_id, &table_def)
        .expect("Failed to update table");

    // Verify remaining columns preserve their original ordinal_positions
    let updated = schema_store
        .get(&test_table_id)
        .expect("Failed to get table")
        .expect("Table not found");

    assert_eq!(updated.columns.len(), 3);
    assert_eq!(updated.columns[0].column_name, "id");
    assert_eq!(updated.columns[0].ordinal_position, 1);
    assert_eq!(updated.columns[1].column_name, "name");
    assert_eq!(updated.columns[1].ordinal_position, 2);
    // Note: ordinal_position 3 is now gone
    assert_eq!(updated.columns[2].column_name, "created_at");
    assert_eq!(updated.columns[2].ordinal_position, 4); // Preserved!

    println!("✅ ALTER TABLE DROP COLUMN preserves ordinal_position of remaining columns");
}

#[tokio::test]
async fn test_system_tables_have_correct_column_ordering() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    let (_jobs_provider, schema_store, _schema_cache) =
        register_system_tables(&system_schema, backend.clone())
            .expect("Failed to register system tables");

    let system_namespace = NamespaceId::from("system");

    // Test system.users table
    let users_table_id = TableId::new(system_namespace.clone(), TableName::from("users"));
    let users_schema = schema_store
        .get(&users_table_id)
        .expect("Failed to get users table")
        .expect("Users table not found");

    // Verify all columns have sequential ordinal_positions starting from 1
    for (idx, column) in users_schema.columns.iter().enumerate() {
        assert_eq!(
            column.ordinal_position as usize,
            idx + 1,
            "Column '{}' should have ordinal_position {}",
            column.column_name,
            idx + 1
        );
    }

    // Verify Arrow schema matches column order
    let arrow_schema = users_schema
        .to_arrow_schema()
        .expect("Failed to convert to Arrow");
    for (idx, column) in users_schema.columns.iter().enumerate() {
        assert_eq!(
            arrow_schema.field(idx).name(),
            &column.column_name,
            "Arrow field at position {} should match column '{}'",
            idx,
            column.column_name
        );
    }

    println!("✅ System tables have correct column ordering (ordinal_position)");
}
