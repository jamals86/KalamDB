#![cfg(test)]
#![cfg(any())] // Disabled - SchemaCache moved to catalog::SchemaCache, tested in unit tests

//! Integration tests for schema cache invalidation on DDL operations
//!
//! Verifies that the schema cache is properly invalidated when tables are
//! created, altered, or dropped, ensuring stale schemas are never served.

#[path = "integration/common/mod.rs"]
mod common;

use kalamdb_commons::models::TableId;
use kalamdb_core::system_table_registration::register_system_tables;
use kalamdb_core::schema_registry::SchemaRegistry;
use kalamdb_core::tables::system::schemas::TableSchemaStore;
use kalamdb_store::RocksDBBackend;
use rocksdb::DB;
use std::sync::Arc;
use tempfile::TempDir;

/// Test helper to create schema store and cache
async fn setup_schema_infrastructure() -> Arc<TableSchemaStore> {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db = Arc::new(
        DB::open_default(temp_dir.path().to_str().unwrap()).expect("Failed to create RocksDB"),
    );
    let backend: Arc<dyn kalamdb_store::StorageBackend> = Arc::new(RocksDBBackend::new(db));

    // Register system tables to get populated schema store and cache
    let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    let (_jobs_provider, schema_store) =
        register_system_tables(&system_schema, backend).expect("Failed to register system tables");

    schema_store
}

#[tokio::test]
async fn test_cache_invalidation_removes_entry() {
    let (_schema_store, schema_registry) = setup_schema_infrastructure().await;

    // Note: register_system_tables pre-warms cache with system table schemas
    let initial_cache_size = schema_registry.len();

    // Create a test table ID
    let table_id = TableId::from_strings("default", "test_table");

    // Simulate CREATE TABLE by inserting into cache
    // (In real scenario, this would happen through DESCRIBE TABLE query)
    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::models::datatypes::KalamDataType;

    let columns = vec![
        ColumnDefinition::new(
            "id",
            1,
            KalamDataType::Int,
            false,
            true,
            false,
            kalamdb_commons::schemas::ColumnDefault::None,
            None,
        ),
        ColumnDefinition::new(
            "name",
            2,
            KalamDataType::Text,
            true,
            false,
            false,
            kalamdb_commons::schemas::ColumnDefault::None,
            None,
        ),
    ];

    let table_def = TableDefinition::new(
        kalamdb_commons::NamespaceId::new("default"),
        kalamdb_commons::TableName::new("test_table"),
        TableType::User,
        columns,
        TableOptions::user(),
        None,
    )
    .expect("Failed to create table definition");

    // Insert into cache
    schema_registry.insert(table_id.clone(), table_def);

    // Verify entry exists
    assert!(
        schema_registry.get(&table_id).is_some(),
        "Table should be in cache"
    );
    assert_eq!(
        schema_registry.len(),
        initial_cache_size + 1,
        "Cache should have 1 additional entry"
    );

    // Simulate ALTER TABLE by invalidating cache
    schema_registry.invalidate(&table_id);

    // Verify entry was removed
    assert!(
        schema_registry.get(&table_id).is_none(),
        "Table should be removed from cache after invalidation"
    );
    assert_eq!(
        schema_registry.len(),
        initial_cache_size,
        "Cache should return to initial size after invalidation"
    );
}

#[tokio::test]
async fn test_cache_invalidation_forces_cache_miss() {
    let (_schema_store, schema_registry) = setup_schema_infrastructure().await;

    // Create a test table
    let table_id = TableId::from_strings("default", "test_table");

    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::models::datatypes::KalamDataType;

    let columns = vec![ColumnDefinition::new(
        "id",
        1,
        KalamDataType::Int,
        false,
        true,
        false,
        kalamdb_commons::schemas::ColumnDefault::None,
        None,
    )];

    let table_def = TableDefinition::new(
        kalamdb_commons::NamespaceId::new("default"),
        kalamdb_commons::TableName::new("test_table"),
        TableType::User,
        columns,
        TableOptions::user(),
        None,
    )
    .expect("Failed to create table definition");

    // Insert and access multiple times to establish cache hits
    schema_registry.insert(table_id.clone(), table_def.clone());
    for _ in 0..10 {
        let _schema = schema_registry.get(&table_id);
    }

    // Get cache stats before invalidation
    let (hits_before, misses_before, _, _) = schema_registry.stats();
    assert_eq!(hits_before, 10, "Should have 10 cache hits");
    assert_eq!(misses_before, 0, "Should have 0 cache misses");

    // Invalidate cache (simulate ALTER TABLE)
    schema_registry.invalidate(&table_id);

    // Access again - should be a miss
    let result = schema_registry.get(&table_id);
    assert!(
        result.is_none(),
        "Should be a cache miss after invalidation"
    );

    // Verify miss count increased
    let (hits_after, misses_after, _, _) = schema_registry.stats();
    assert_eq!(hits_after, 10, "Hit count should not change");
    assert_eq!(misses_after, 1, "Miss count should increase by 1");
}

#[tokio::test]
async fn test_selective_invalidation_preserves_other_entries() {
    let (_schema_store, schema_registry) = setup_schema_infrastructure().await;

    // Note: register_system_tables pre-warms cache with system table schemas
    let initial_cache_size = schema_registry.len();

    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::models::datatypes::KalamDataType;

    // Create 3 test tables
    for i in 1..=3 {
        let table_id = TableId::from_strings("default", &format!("test_table_{}", i));
        let columns = vec![ColumnDefinition::new(
            "id",
            1,
            KalamDataType::Int,
            false,
            true,
            false,
            kalamdb_commons::schemas::ColumnDefault::None,
            None,
        )];

        let table_def = TableDefinition::new(
            kalamdb_commons::NamespaceId::new("default"),
            kalamdb_commons::TableName::new(format!("test_table_{}", i)),
            TableType::User,
            columns,
            TableOptions::user(),
            None,
        )
        .expect("Failed to create table definition");

        schema_registry.insert(table_id, table_def);
    }

    // Verify all 3 are cached
    assert_eq!(
        schema_registry.len(),
        initial_cache_size + 3,
        "Should have 3 additional entries in cache"
    );

    // Invalidate table_2
    let table_2_id = TableId::from_strings("default", "test_table_2");
    schema_registry.invalidate(&table_2_id);

    // Verify only table_2 was removed
    assert_eq!(
        schema_registry.len(),
        initial_cache_size + 2,
        "Should have 2 additional entries after invalidation"
    );

    let table_1_id = TableId::from_strings("default", "test_table_1");
    let table_3_id = TableId::from_strings("default", "test_table_3");

    assert!(
        schema_registry.get(&table_1_id).is_some(),
        "Table 1 should still be cached"
    );
    assert!(
        schema_registry.get(&table_2_id).is_none(),
        "Table 2 should be invalidated"
    );
    assert!(
        schema_registry.get(&table_3_id).is_some(),
        "Table 3 should still be cached"
    );
}

#[tokio::test]
async fn test_invalidation_idempotent() {
    let (_schema_store, schema_registry) = setup_schema_infrastructure().await;

    // Note: register_system_tables pre-warms cache with system table schemas
    let initial_cache_size = schema_registry.len();

    let table_id = TableId::from_strings("default", "test_table");

    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::models::datatypes::KalamDataType;

    let columns = vec![ColumnDefinition::new(
        "id",
        1,
        KalamDataType::Int,
        false,
        true,
        false,
        kalamdb_commons::schemas::ColumnDefault::None,
        None,
    )];

    let table_def = TableDefinition::new(
        kalamdb_commons::NamespaceId::new("default"),
        kalamdb_commons::TableName::new("test_table"),
        TableType::User,
        columns,
        TableOptions::user(),
        None,
    )
    .expect("Failed to create table definition");

    schema_registry.insert(table_id.clone(), table_def);

    // Invalidate multiple times - should not panic
    schema_registry.invalidate(&table_id);
    schema_registry.invalidate(&table_id); // Second invalidation on non-existent entry
    schema_registry.invalidate(&table_id); // Third invalidation

    // Cache should be back to initial size
    assert_eq!(
        schema_registry.len(),
        initial_cache_size,
        "Cache should return to initial size after multiple invalidations"
    );
}

#[tokio::test]
async fn test_cache_stats_track_invalidation_behavior() {
    let (_schema_store, schema_registry) = setup_schema_infrastructure().await;

    // Note: register_system_tables pre-warms cache with system table schemas
    let initial_cache_size = schema_registry.len();

    let table_id = TableId::from_strings("default", "test_table");

    use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
    use kalamdb_commons::models::datatypes::KalamDataType;

    let columns = vec![ColumnDefinition::new(
        "id",
        1,
        KalamDataType::Int,
        false,
        true,
        false,
        kalamdb_commons::schemas::ColumnDefault::None,
        None,
    )];

    let table_def = TableDefinition::new(
        kalamdb_commons::NamespaceId::new("default"),
        kalamdb_commons::TableName::new("test_table"),
        TableType::User,
        columns,
        TableOptions::user(),
        None,
    )
    .expect("Failed to create table definition");

    // Pattern: INSERT → HIT → INVALIDATE → MISS → INSERT → HIT
    schema_registry.insert(table_id.clone(), table_def.clone());
    let _ = schema_registry.get(&table_id); // Hit
    schema_registry.invalidate(&table_id);
    let _ = schema_registry.get(&table_id); // Miss
    schema_registry.insert(table_id.clone(), table_def);
    let _ = schema_registry.get(&table_id); // Hit

    let (hits, misses, _, size) = schema_registry.stats();
    assert_eq!(hits, 2, "Should have 2 hits");
    assert_eq!(misses, 1, "Should have 1 miss");
    assert_eq!(
        size,
        initial_cache_size + 1,
        "Should have 1 additional entry"
    );
}
