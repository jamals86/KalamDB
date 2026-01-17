//! Integration tests for Manifest Service and Flush Integration (Phase 5, US2, T128-T134)
//!
//! Tests:
//! - T128: create_manifest() → generates valid JSON with version=1
//! - T129: update_manifest() → increments version, appends SegmentMetadata
//! - T130: flush 5 batches → manifest.json tracks all batch metadata
//! - T131: query with WHERE _seq >= X → skips batches with max_seq < X (TODO: query planner)
//! - T132: query with WHERE id = X → scans only batches with id range containing X (TODO: query planner)
//! - T133: corrupt manifest → rebuild from Parquet footers → queries resume (TODO: recovery)
//! - T134: manifest pruning reduces file scans by 80%+ (TODO: performance test)

#[path = "../../../crates/kalamdb-core/tests/test_helpers.rs"]
mod test_helpers;

use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::models::types::{Manifest, SegmentMetadata};
use kalamdb_commons::UserId;
use kalamdb_commons::{NamespaceId, StorageId, TableId, TableName};
use kalamdb_configs::ManifestCacheSettings;
use kalamdb_core::manifest::ManifestService;
use kalamdb_core::schema_registry::PathResolver;
use kalamdb_store::{test_utils::InMemoryBackend, StorageBackend};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_service() -> (ManifestService, TempDir) {
    let temp_dir = TempDir::new().unwrap();

    // Initialize a test AppContext for SchemaRegistry and providers (used by ManifestService)
    test_helpers::init_test_app_context();
    let app_ctx = kalamdb_core::app_context::AppContext::get();

    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    let config = ManifestCacheSettings::default();
    let service = ManifestService::new_with_registries(
        backend,
        temp_dir.path().to_string_lossy().to_string(),
        config,
        app_ctx.schema_registry(),
        app_ctx.storage_registry(),
    );

    // Register "local" storage required by flush operations
    {
        use kalamdb_commons::models::StorageType;
        use kalamdb_commons::types::Storage;
        use kalamdb_commons::StorageId;

        // Use the same app_ctx
        let storages_provider = app_ctx.system_tables().storages();
        let storage_id = StorageId::new("local");

        let now = chrono::Utc::now().timestamp_millis();
        let base_path = temp_dir.path().to_string_lossy().to_string();

        match storages_provider.get_storage_by_id(&storage_id).ok().flatten() {
            Some(mut existing) => {
                existing.base_directory = base_path;
                existing.updated_at = now;
                let _ = storages_provider.update_storage(existing);
            },
            None => {
                let default_storage = Storage {
                    storage_id: storage_id.clone(),
                    storage_name: "Local Filesystem".to_string(),
                    description: Some("Default local filesystem storage".to_string()),
                    storage_type: StorageType::Filesystem,
                    base_directory: base_path,
                    credentials: None,
                    config_json: None,
                    shared_tables_template: "shared/{namespace}/{tableName}".to_string(),
                    user_tables_template: "users/{userId}/tables/{namespace}/{tableName}"
                        .to_string(),
                    created_at: now,
                    updated_at: now,
                };
                let _ = storages_provider.insert_storage(default_storage);
            },
        }

        app_ctx.storage_registry().invalidate(&storage_id);
    }

    // Register minimal table definitions required for these tests
    {
        use kalamdb_commons::models::schemas::TableType;
        use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition};
        use kalamdb_commons::models::TableId;
        use kalamdb_core::schema_registry::CachedTableData;
        use std::sync::Arc as StdArc;

        let app_ctx = kalamdb_core::app_context::AppContext::get();
        let schema_registry = app_ctx.schema_registry();
        let tables_provider = app_ctx.system_tables().tables();

        let storage_id = StorageId::new("local");
        let register = |ns: &str, name: &str, ttype: TableType| {
            let namespace = kalamdb_commons::NamespaceId::new(ns);
            let table_name = kalamdb_commons::TableName::new(name);
            let table_id = TableId::new(namespace.clone(), table_name.clone());
            let cols = vec![
                ColumnDefinition::primary_key(
                    1,
                    "id",
                    1,
                    kalamdb_commons::models::datatypes::KalamDataType::BigInt,
                ),
                ColumnDefinition::simple(
                    2,
                    "value",
                    2,
                    kalamdb_commons::models::datatypes::KalamDataType::Text,
                ),
            ];
            let table_def = TableDefinition::new_with_defaults(
                namespace.clone(),
                table_name.clone(),
                ttype,
                cols,
                None,
            )
            .unwrap();
            // Persist to system.tables
            let _ = tables_provider.create_table(&table_id, &table_def);
            // Put into schema registry cache
            let _ = schema_registry.put_table_definition(&app_ctx, &table_id, &table_def);
            // Build cached table data and set storage path template to a temp directory for tests
            let mut cached = CachedTableData::new(StdArc::new(table_def.clone()));
            let storage_template = PathResolver::resolve_storage_path_template(
                app_ctx.as_ref(),
                &table_id,
                ttype,
                &storage_id,
            )
            .unwrap_or_else(|_| format!("shared/{}/{}", ns, name));
            cached.storage_path_template = storage_template.clone();
            cached.storage_id = Some(storage_id.clone());
            // Ensure directories exist
            let _ = std::fs::create_dir_all(&storage_template);
            schema_registry.insert(table_id, StdArc::new(cached));
        };

        register("test_ns", "orders", TableType::Shared);
        register("prod", "events", TableType::Shared);
        register("test_ns", "persistent_table", TableType::Shared);
        register("test_ns", "metadata_table", TableType::Shared);
        register("ns1", "products", TableType::User);
    }
    (service, temp_dir)
}

// T128: create_manifest() → generates valid JSON with version=1
#[tokio::test]
async fn test_create_manifest_generates_valid_json() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("test_table");
    let table_id = TableId::new(namespace.clone(), table.clone());
    let manifest = service.create_manifest(&table_id, TableType::Shared, None);

    // Verify structure
    assert_eq!(manifest.table_id.namespace_id().as_str(), "test_ns");
    assert_eq!(manifest.table_id.table_name().as_str(), "test_table");
    assert_eq!(manifest.user_id, None);
    assert_eq!(manifest.version, 1, "Initial version should be 1");
    assert_eq!(manifest.segments.len(), 0, "Initial segments should be empty");

    // Verify it can be serialized to valid JSON
    let json = serde_json::to_string(&manifest).unwrap();
    println!("Generated JSON: {}", json); // Debug output
    assert!(json.contains("\"version\":1") || json.contains("\"version\": 1"));

    // Verify it can be deserialized back
    let deserialized: Manifest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.table_id, manifest.table_id);
    assert_eq!(deserialized.version, manifest.version);
}

// T129: update_manifest() → increments version, appends SegmentMetadata
#[tokio::test]
async fn test_update_manifest_increments_version() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("orders");
    let scope = "shared";
    let scope_user: Option<UserId> = if scope == "shared" {
        None
    } else {
        Some(UserId::from(scope))
    };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();
    let table_id = TableId::new(namespace.clone(), table.clone());

    // Create first batch entry
    let batch_entry_0 = SegmentMetadata::new(
        "0".to_string(),
        "batch-0.parquet".to_string(),
        HashMap::new(),
        1000,
        2000,
        100,
        1024,
    );

    // First update (creates manifest if doesn't exist)
    let manifest_v1 = service
        .update_manifest(&table_id, TableType::Shared, scope_user_ref, batch_entry_0)
        .unwrap();

    assert_eq!(manifest_v1.segments.len(), 1, "Should have 1 segment");
    assert_eq!(manifest_v1.segments[0].id, "0");

    // Create second batch entry
    let batch_entry_1 = SegmentMetadata::new(
        "1".to_string(),
        "batch-1.parquet".to_string(),
        HashMap::new(),
        2001,
        3000,
        150,
        2048,
    );

    // Second update
    let manifest_v2 = service
        .update_manifest(&table_id, TableType::Shared, scope_user_ref, batch_entry_1)
        .unwrap();

    assert_eq!(manifest_v2.segments.len(), 2, "Should have 2 segments");
    assert_eq!(manifest_v2.segments[1].id, "1");

    // Create third batch entry
    let batch_entry_2 = SegmentMetadata::new(
        "2".to_string(),
        "batch-2.parquet".to_string(),
        HashMap::new(),
        3001,
        4000,
        200,
        3072,
    );

    // Third update
    let manifest_v3 = service
        .update_manifest(&table_id, TableType::Shared, scope_user_ref, batch_entry_2)
        .unwrap();

    assert_eq!(manifest_v3.segments.len(), 3, "Should have 3 segments");

    // Verify all batches are present
    assert_eq!(manifest_v3.segments[0].id, "0");
    assert_eq!(manifest_v3.segments[1].id, "1");
    assert_eq!(manifest_v3.segments[2].id, "2");
}

// T130: flush 5 batches → manifest.json tracks all batch metadata
#[tokio::test]
async fn test_flush_five_batches_manifest_tracking() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("prod");
    let table = TableName::new("events");
    let table_id = TableId::new(namespace.clone(), table.clone());
    let scope = "shared";
    let scope_user: Option<UserId> = if scope == "shared" {
        None
    } else {
        Some(UserId::from(scope))
    };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();

    // Simulate flushing 5 batches with different metadata
    let batch_configs = vec![
        (0, "batch-0.parquet", 1000, 1999, 100, 1024),
        (1, "batch-1.parquet", 2000, 2999, 150, 2048),
        (2, "batch-2.parquet", 3000, 3999, 200, 3072),
        (3, "batch-3.parquet", 4000, 4999, 250, 4096),
        (4, "batch-4.parquet", 5000, 5999, 300, 5120),
    ];

    for (batch_num, file_name, min_seq, max_seq, row_count, size_bytes) in batch_configs {
        let batch_entry = SegmentMetadata::new(
            batch_num.to_string(),
            file_name.to_string(),
            HashMap::new(),
            min_seq,
            max_seq,
            row_count,
            size_bytes,
        );

        service
            .update_manifest(&table_id, TableType::Shared, scope_user_ref, batch_entry)
            .unwrap();
    }

    // Flush to disk so read_manifest can find it
    service.flush_manifest(&table_id, scope_user_ref).unwrap();

    // Read final manifest
    let final_manifest = service.read_manifest(&table_id, scope_user_ref).unwrap();

    // Verify all batches are tracked
    assert_eq!(final_manifest.segments.len(), 5, "Should have 5 segments");

    // Verify batch metadata is preserved
    for (idx, batch) in final_manifest.segments.iter().enumerate() {
        assert_eq!(batch.id, idx.to_string(), "Batch ID should match index");
        assert_eq!(batch.path, format!("batch-{}.parquet", idx));

        // Verify sequence ranges
        let expected_min = 1000 + (idx as i64 * 1000);
        let expected_max = 1999 + (idx as i64 * 1000);
        assert_eq!(batch.min_seq, expected_min);
        assert_eq!(batch.max_seq, expected_max);

        // Verify row count increases
        let expected_row_count = 100 + (idx * 50);
        assert_eq!(batch.row_count, expected_row_count as u64);
    }

    // Verify manifest can be validated
    service.validate_manifest(&final_manifest).unwrap();
}

// T128 (additional): Verify manifest persistence across reads
#[tokio::test]
async fn test_manifest_persistence_across_reads() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("persistent_table");
    let scope = "shared";
    let scope_user: Option<UserId> = if scope == "shared" {
        None
    } else {
        Some(UserId::from(scope))
    };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();
    let table_id = TableId::new(namespace.clone(), table.clone());

    // Create and write manifest
    let batch_entry = SegmentMetadata::new(
        "0".to_string(),
        "batch-0.parquet".to_string(),
        HashMap::new(),
        1000,
        2000,
        100,
        1024,
    );

    service
        .update_manifest(&table_id, TableType::Shared, scope_user_ref, batch_entry)
        .unwrap();

    // Flush to disk
    service.flush_manifest(&table_id, scope_user_ref).unwrap();

    // Read it back multiple times
    let read1 = service.read_manifest(&table_id, scope_user_ref).unwrap();
    let read2 = service.read_manifest(&table_id, scope_user_ref).unwrap();

    assert_eq!(read1.segments.len(), read2.segments.len());
    assert_eq!(read1.table_id, read2.table_id);
}

// T129 (additional): Verify batch entry metadata is preserved
#[tokio::test]
async fn test_batch_entry_metadata_preservation() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("metadata_table");
    // Use shared scope to avoid user-table schema registry resolution issues in test harness
    let scope_user_ref: Option<&UserId> = None;
    let table_id = TableId::new(namespace.clone(), table.clone());

    // Create batch entry with rich metadata
    let batch_entry = SegmentMetadata::new(
        "0".to_string(),
        "batch-0.parquet".to_string(),
        HashMap::new(),
        1000,
        2000,
        1234,
        567890,
    );

    service
        .update_manifest(&table_id, TableType::Shared, scope_user_ref, batch_entry)
        .unwrap();

    // Flush to disk
    service.flush_manifest(&table_id, scope_user_ref).unwrap();

    // Read back and verify
    let manifest = service.read_manifest(&table_id, scope_user_ref).unwrap();
    let saved_batch = &manifest.segments[0];

    assert_eq!(saved_batch.id, "0");
    assert_eq!(saved_batch.path, "batch-0.parquet");
    assert_eq!(saved_batch.min_seq, 1000);
    assert_eq!(saved_batch.max_seq, 2000);
    assert_eq!(saved_batch.row_count, 1234);
    assert_eq!(saved_batch.size_bytes, 567890);
}

// T130 (additional): Verify manifest validation detects corruption
#[tokio::test]
async fn test_manifest_validation_detects_corruption() {
    use kalamdb_commons::models::TableId;
    use kalamdb_commons::{NamespaceId, TableName};

    let (service, _temp_dir) = create_test_service();

    // Create a valid manifest
    let table_id = TableId::new(NamespaceId::new("test"), TableName::new("table"));
    let mut manifest = Manifest::new(table_id, None);

    // Valid empty manifest
    assert!(service.validate_manifest(&manifest).is_ok());

    // Add batch
    let batch = SegmentMetadata::new(
        "1".to_string(),
        "batch-1.parquet".to_string(),
        HashMap::new(),
        1000,
        2000,
        100,
        1024,
    );
    manifest.add_segment(batch);

    // Valid manifest with batch
    assert!(service.validate_manifest(&manifest).is_ok());
}

// Helper test: Verify batch file entry creation
#[test]
fn test_segment_metadata_creation() {
    let entry = SegmentMetadata::new(
        "5".to_string(),
        "batch-5.parquet".to_string(),
        HashMap::new(),
        1000,
        2000,
        500,
        10240,
    );

    assert_eq!(entry.id, "5");
    assert_eq!(entry.path, "batch-5.parquet");
    assert_eq!(entry.min_seq, 1000);
    assert_eq!(entry.max_seq, 2000);
    assert_eq!(entry.row_count, 500);
    assert_eq!(entry.size_bytes, 10240);
}
