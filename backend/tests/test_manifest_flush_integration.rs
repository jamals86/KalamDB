//! Integration tests for Manifest Service and Flush Integration (Phase 5, US2, T128-T134)
//!
//! Tests:
//! - T128: create_manifest() → generates valid JSON with version=1, max_batch=0
//! - T129: update_manifest() → increments max_batch, appends BatchFileEntry
//! - T130: flush 5 batches → manifest.json tracks all batch metadata
//! - T131: query with WHERE _seq >= X → skips batches with max_seq < X (TODO: query planner)
//! - T132: query with WHERE id = X → scans only batches with id range containing X (TODO: query planner)
//! - T133: corrupt manifest → rebuild from Parquet footers → queries resume (TODO: recovery)
//! - T134: manifest pruning reduces file scans by 80%+ (TODO: performance test)

use kalamdb_commons::{NamespaceId, TableName};
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::UserId;
use kalamdb_core::manifest::ManifestService;
use kalamdb_commons::types::{BatchFileEntry, ManifestFile};
use kalamdb_store::{test_utils::InMemoryBackend, StorageBackend};
use std::sync::Arc;
use tempfile::TempDir;

fn create_test_service() -> (ManifestService, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());
    let service = ManifestService::new(backend, temp_dir.path().to_string_lossy().to_string());
    // Initialize a test AppContext for SchemaRegistry and providers (used by ManifestService)
    kalamdb_core::test_helpers::init_test_app_context();
    // Register minimal table definitions required for these tests
    {
        use kalamdb_commons::models::schemas::{TableDefinition, ColumnDefinition, TableOptions, TableType};
        use kalamdb_commons::models::{TableId};
        use kalamdb_core::schema_registry::CachedTableData;
        use std::sync::Arc as StdArc;

        let app_ctx = kalamdb_core::app_context::AppContext::get();
        let schema_registry = app_ctx.schema_registry();
        let tables_provider = app_ctx.system_tables().tables();

        let base_dir = temp_dir.path().to_string_lossy().to_string();
        let register = |ns: &str, name: &str, ttype: TableType| {
            let namespace = kalamdb_commons::NamespaceId::new(ns);
            let table_name = kalamdb_commons::TableName::new(name);
            let table_id = TableId::new(namespace.clone(), table_name.clone());
            let cols = vec![
                ColumnDefinition::primary_key("id", 1, kalamdb_commons::models::datatypes::KalamDataType::BigInt),
                ColumnDefinition::simple("value", 2, kalamdb_commons::models::datatypes::KalamDataType::Text),
            ];
            let table_def = TableDefinition::new_with_defaults(namespace.clone(), table_name.clone(), ttype, cols, None).unwrap();
            // Persist to system.tables
            let _ = tables_provider.create_table(&table_id, &table_def);
            // Put into schema registry cache
            let _ = schema_registry.put_table_definition(&table_id, &table_def);
            // Build cached table data and set storage path template to a temp directory for tests
            let mut cached = CachedTableData::new(StdArc::new(table_def.clone()));
            let storage_template = format!("{}/{}/{}", base_dir, ns, name);
            cached.storage_path_template = storage_template.clone();
            // Ensure directories exist
            let _ = std::fs::create_dir_all(&storage_template);
            schema_registry.insert(table_id, StdArc::new(cached));
        };

        register("test_ns", "orders", TableType::Shared);
        register("prod", "events", TableType::Shared);
        register("test_ns", "persistent_table", TableType::Shared);
        register("test_ns", "metadata_table", TableType::User);
        register("ns1", "products", TableType::User);
    }
    (service, temp_dir)
}

// T128: create_manifest() → generates valid JSON with version=1, max_batch=0
#[test]
fn test_create_manifest_generates_valid_json() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("test_table");
    let manifest = service.create_manifest(&namespace, &table, None);
    
    // Verify structure
    assert_eq!(manifest.table_id, "test_ns.test_table");
    assert_eq!(manifest.scope, "shared");
    assert_eq!(manifest.version, 1, "Initial version should be 1");
    assert_eq!(manifest.max_batch, 0, "Initial max_batch should be 0");
    assert_eq!(manifest.batches.len(), 0, "Initial batches should be empty");
    
    // Verify it can be serialized to valid JSON
    let json = manifest.to_json().unwrap();
    println!("Generated JSON: {}", json); // Debug output
    assert!(json.contains("\"version\": 1") || json.contains("\"version\":1"));
    assert!(json.contains("\"max_batch\": 0") || json.contains("\"max_batch\":0"));
    
    // Verify it can be deserialized back
    let deserialized = ManifestFile::from_json(&json).unwrap();
    assert_eq!(deserialized.table_id, manifest.table_id);
    assert_eq!(deserialized.version, manifest.version);
}

// T129: update_manifest() → increments max_batch, appends BatchFileEntry
#[test]
fn test_update_manifest_increments_max_batch() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("orders");
    let scope = "shared";
    let scope_user: Option<UserId> = if scope == "shared" { None } else { Some(UserId::from(scope)) };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();
    
    // Create first batch entry
    let batch_entry_0 = BatchFileEntry::new(
        0,
        "batch-0.parquet".to_string(),
        1000,
        2000,
        std::collections::HashMap::new(),
        100,
        1024,
        1,
    );
    
    // First update (creates manifest if doesn't exist)
    let manifest_v1 = service
        .update_manifest(&namespace, &table, scope_user_ref, batch_entry_0)
        .unwrap();
    
    assert_eq!(manifest_v1.max_batch, 0, "First batch should be batch-0");
    assert_eq!(manifest_v1.batches.len(), 1, "Should have 1 batch");
    assert_eq!(manifest_v1.batches[0].batch_number, 0);
    
    // Create second batch entry
    let batch_entry_1 = BatchFileEntry::new(
        1,
        "batch-1.parquet".to_string(),
        2001,
        3000,
        std::collections::HashMap::new(),
        150,
        2048,
        1,
    );
    
    // Second update
    let manifest_v2 = service
        .update_manifest(&namespace, &table, scope_user_ref, batch_entry_1)
        .unwrap();
    
    assert_eq!(manifest_v2.max_batch, 1, "Second batch should increment max_batch to 1");
    assert_eq!(manifest_v2.batches.len(), 2, "Should have 2 batches");
    assert_eq!(manifest_v2.batches[1].batch_number, 1);
    
    // Create third batch entry
    let batch_entry_2 = BatchFileEntry::new(
        2,
        "batch-2.parquet".to_string(),
        3001,
        4000,
        std::collections::HashMap::new(),
        200,
        3072,
        1,
    );
    
    // Third update
    let manifest_v3 = service
        .update_manifest(&namespace, &table, scope_user_ref, batch_entry_2)
        .unwrap();
    
    assert_eq!(manifest_v3.max_batch, 2, "Third batch should increment max_batch to 2");
    assert_eq!(manifest_v3.batches.len(), 3, "Should have 3 batches");
    
    // Verify all batches are present
    assert_eq!(manifest_v3.batches[0].batch_number, 0);
    assert_eq!(manifest_v3.batches[1].batch_number, 1);
    assert_eq!(manifest_v3.batches[2].batch_number, 2);
}

// T130: flush 5 batches → manifest.json tracks all batch metadata
#[test]
fn test_flush_five_batches_manifest_tracking() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("prod");
    let table = TableName::new("events");
    let scope = "shared";
    let scope_user: Option<UserId> = if scope == "shared" { None } else { Some(UserId::from(scope)) };
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
        let mut column_stats = std::collections::HashMap::new();
        column_stats.insert(
            SystemColumnNames::SEQ.to_string(),
            (serde_json::json!(min_seq), serde_json::json!(max_seq)),
        );
        column_stats.insert(
            "id".to_string(),
            (serde_json::json!(batch_num * 1000), serde_json::json!((batch_num + 1) * 1000 - 1)),
        );
        
        let batch_entry = BatchFileEntry::new(
            batch_num,
            file_name.to_string(),
            min_seq,
            max_seq,
            column_stats,
            row_count,
            size_bytes,
            1,
        );
        
        service
            .update_manifest(&namespace, &table, scope_user_ref, batch_entry)
            .unwrap();
    }
    
    // Read final manifest
    let final_manifest = service.read_manifest(&namespace, &table, scope_user_ref).unwrap();
    
    // Verify all batches are tracked
    assert_eq!(final_manifest.max_batch, 4, "Final max_batch should be 4");
    assert_eq!(final_manifest.batches.len(), 5, "Should have 5 batches");
    
    // Verify batch metadata is preserved
    for (idx, batch) in final_manifest.batches.iter().enumerate() {
        assert_eq!(batch.batch_number, idx as u64, "Batch number should match index");
        assert_eq!(batch.file_path, format!("batch-{}.parquet", idx));
        
        // Verify sequence ranges
        let expected_min = 1000 + (idx as i64 * 1000);
        let expected_max = 1999 + (idx as i64 * 1000);
        assert_eq!(batch.min_seq, expected_min);
        assert_eq!(batch.max_seq, expected_max);
        
        // Verify column stats
        assert!(batch.column_min_max.contains_key(SystemColumnNames::SEQ));
        assert!(batch.column_min_max.contains_key("id"));
        
        // Verify row count increases
        let expected_row_count = 100 + (idx * 50);
        assert_eq!(batch.row_count, expected_row_count as u64);
    }
    
    // Verify manifest can be validated
    service.validate_manifest(&final_manifest).unwrap();
}

// T128 (additional): Verify manifest persistence across reads
#[test]
fn test_manifest_persistence_across_reads() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("persistent_table");
    let scope = "shared";
    let scope_user: Option<UserId> = if scope == "shared" { None } else { Some(UserId::from(scope)) };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();
    
    // Create and write manifest
    let batch_entry = BatchFileEntry::new(
        0,
        "batch-0.parquet".to_string(),
        1000,
        2000,
        std::collections::HashMap::new(),
        100,
        1024,
        1,
    );
    
    service
        .update_manifest(&namespace, &table, scope_user_ref, batch_entry)
        .unwrap();
    
    // Read it back multiple times
    let read1 = service.read_manifest(&namespace, &table, scope_user_ref).unwrap();
    let read2 = service.read_manifest(&namespace, &table, scope_user_ref).unwrap();
    
    assert_eq!(read1.max_batch, read2.max_batch);
    assert_eq!(read1.batches.len(), read2.batches.len());
    assert_eq!(read1.table_id, read2.table_id);
}

// T129 (additional): Verify batch entry metadata is preserved
#[test]
fn test_batch_entry_metadata_preservation() {
    let (service, _temp_dir) = create_test_service();
    let namespace = NamespaceId::new("test_ns");
    let table = TableName::new("metadata_table");
    let scope = "u_123";
    let scope_user: Option<UserId> = if scope == "shared" { None } else { Some(UserId::from(scope)) };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();
    
    // Create batch entry with rich metadata
    let mut column_stats = std::collections::HashMap::new();
    column_stats.insert(
        SystemColumnNames::SEQ.to_string(),
        (serde_json::json!(1000), serde_json::json!(2000)),
    );
    column_stats.insert(
        "id".to_string(),
        (serde_json::json!(1), serde_json::json!(100)),
    );
    column_stats.insert(
        "name".to_string(),
        (serde_json::json!("alice"), serde_json::json!("zara")),
    );
    column_stats.insert(
        "score".to_string(),
        (serde_json::json!(0), serde_json::json!(100)),
    );
    
    let batch_entry = BatchFileEntry::new(
        0,
        "batch-0.parquet".to_string(),
        1000,
        2000,
        column_stats.clone(),
        1234,
        567890,
        1,
    );
    
    let scope_user: Option<UserId> = if scope == "shared" { None } else { Some(UserId::from(scope)) };
    let scope_user_ref: Option<&UserId> = scope_user.as_ref();
    service
        .update_manifest(&namespace, &table, scope_user_ref, batch_entry)
        .unwrap();
    
    // Read back and verify
    let manifest = service.read_manifest(&namespace, &table, scope_user_ref).unwrap();
    let saved_batch = &manifest.batches[0];
    
    assert_eq!(saved_batch.batch_number, 0);
    assert_eq!(saved_batch.file_path, "batch-0.parquet");
    assert_eq!(saved_batch.min_seq, 1000);
    assert_eq!(saved_batch.max_seq, 2000);
    assert_eq!(saved_batch.row_count, 1234);
    assert_eq!(saved_batch.size_bytes, 567890);
    assert_eq!(saved_batch.schema_version, 1);
    
    // Verify column stats
    assert_eq!(saved_batch.column_min_max.len(), 4);
    assert!(saved_batch.column_min_max.contains_key(SystemColumnNames::SEQ));
    assert!(saved_batch.column_min_max.contains_key("id"));
    assert!(saved_batch.column_min_max.contains_key("name"));
    assert!(saved_batch.column_min_max.contains_key("score"));
    
    let seq_stats = saved_batch.column_min_max.get(SystemColumnNames::SEQ).unwrap();
    assert_eq!(seq_stats.0, serde_json::json!(1000));
    assert_eq!(seq_stats.1, serde_json::json!(2000));
}

// T130 (additional): Verify manifest validation detects corruption
#[test]
fn test_manifest_validation_detects_corruption() {
    let (service, _temp_dir) = create_test_service();
    
    // Create a valid manifest
    let mut manifest = ManifestFile::new("test.table".to_string(), "shared".to_string());
    
    // Valid empty manifest
    assert!(service.validate_manifest(&manifest).is_ok());
    
    // Add batch
    let batch = BatchFileEntry::new(
        1,
        "batch-1.parquet".to_string(),
        1000,
        2000,
        std::collections::HashMap::new(),
        100,
        1024,
        1,
    );
    manifest.add_batch(batch);
    
    // Valid manifest with batch
    assert!(service.validate_manifest(&manifest).is_ok());
    
    // Corrupt max_batch (should detect mismatch)
    manifest.max_batch = 99;
    assert!(
        service.validate_manifest(&manifest).is_err(),
        "Should detect max_batch corruption"
    );
}

// Helper test: Verify batch file entry creation
#[test]
fn test_batch_file_entry_creation() {
    let mut stats = std::collections::HashMap::new();
    stats.insert(
        SystemColumnNames::SEQ.to_string(),
        (serde_json::json!(1000), serde_json::json!(2000)),
    );
    
    let entry = BatchFileEntry::new(
        5,
        "batch-5.parquet".to_string(),
        1000,
        2000,
        stats,
        500,
        10240,
        1,
    );
    
    assert_eq!(entry.batch_number, 5);
    assert_eq!(entry.file_path, "batch-5.parquet");
    assert_eq!(entry.min_seq, 1000);
    assert_eq!(entry.max_seq, 2000);
    assert_eq!(entry.row_count, 500);
    assert_eq!(entry.size_bytes, 10240);
    assert_eq!(entry.schema_version, 1);
    
    // Test sequence range overlap detection
    assert!(entry.overlaps_seq_range(1500, 2500)); // Overlaps
    assert!(entry.overlaps_seq_range(500, 1500));  // Overlaps
    assert!(!entry.overlaps_seq_range(2001, 3000)); // No overlap (after)
    assert!(!entry.overlaps_seq_range(0, 999));     // No overlap (before)
}
