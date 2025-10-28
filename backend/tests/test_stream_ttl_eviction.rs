//! Integration test for stream table TTL eviction and SELECT queries
//!
//! This test verifies that:
//! 1. Stream tables can be created with TTL
//! 2. SELECT queries work on stream tables
//! 3. TTL eviction automatically removes old events

use datafusion::arrow::array::{Int64Array, RecordBatch, StringBuilder};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::TableProvider;
use datafusion::execution::context::SessionState;
use datafusion::prelude::*;
use kalamdb_core::catalog::{NamespaceId, TableName, TableType};
use kalamdb_core::stores::StreamTableStore;
use kalamdb_core::tables::stream_table_provider::StreamTableProvider;
use kalamdb_store::test_utils::TestDb;
use kalamdb_store::RocksDBBackend;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn test_stream_table_ttl_eviction_with_select() {
    // Create test database
    let test_db = TestDb::new(&["test:test_events"]).expect("Failed to create test DB");
    let backend: Arc<dyn kalamdb_commons::storage::StorageBackend> =
        Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let stream_store = Arc::new(StreamTableStore::new(backend.clone()));

    // Create stream table schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
    ]));

    // Create stream table provider with 1-second TTL
    let table_metadata = kalamdb_core::catalog::TableMetadata::new(
        TableName::new("test_events"),
        TableType::Stream,
        NamespaceId::new("test"),
        "local".to_string(),
    );

    let provider = Arc::new(StreamTableProvider::new(
        table_metadata,
        schema.clone(),
        stream_store.clone(),
        Some(1), // 1-second TTL
        false,
        None,
    ));

    // Create DataFusion context and register table
    let ctx = SessionContext::new();
    ctx.register_table("test_events", provider.clone())
        .expect("Failed to register table");

    // Insert test data using the provider directly
    let event1 = serde_json::json!({
        "event_id": "evt1",
        "event_type": "click",
        "value": 100
    });
    let event2 = serde_json::json!({
        "event_id": "evt2",
        "event_type": "view",
        "value": 200
    });
    let event3 = serde_json::json!({
        "event_id": "evt3",
        "event_type": "purchase",
        "value": 300
    });

    provider
        .insert_event("evt1", event1)
        .expect("Failed to insert event1");
    provider
        .insert_event("evt2", event2)
        .expect("Failed to insert event2");
    provider
        .insert_event("evt3", event3)
        .expect("Failed to insert event3");

    // STEP 1: Verify SELECT works - should return 3 rows
    let df = ctx
        .sql("SELECT * FROM test_events")
        .await
        .expect("Failed to execute SELECT");
    let results = df.collect().await.expect("Failed to collect results");

    assert_eq!(results.len(), 1, "Expected 1 batch");
    let batch = &results[0];
    assert_eq!(batch.num_rows(), 3, "Expected 3 rows before eviction");

    println!("✅ SELECT returned 3 events before TTL expiration");

    // STEP 2: Wait slightly over TTL and evict expired
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let evicted_count = provider
        .evict_expired()
        .expect("Failed to evict expired events");

    println!("✅ Evicted {} events", evicted_count);
    assert_eq!(evicted_count, 3, "Expected all 3 events to be evicted");

    // STEP 3: Verify SELECT returns empty result set
    let df = ctx
        .sql("SELECT * FROM test_events")
        .await
        .expect("Failed to execute SELECT after eviction");
    let results = df.collect().await.expect("Failed to collect results");

    // Result can be either empty or have a batch with 0 rows
    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 0, "Expected 0 rows after eviction");

    println!("✅ SELECT returned 0 events after TTL eviction");
}

#[tokio::test]
async fn test_stream_table_select_with_projection() {
    // Create test database
    let test_db = TestDb::new(&["test:events"]).expect("Failed to create test DB");
    let backend: Arc<dyn kalamdb_commons::storage::StorageBackend> =
        Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let stream_store = Arc::new(StreamTableStore::new(backend.clone()));

    // Create stream table schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
    ]));

    // Create stream table provider
    let table_metadata = kalamdb_core::catalog::TableMetadata::new(
        TableName::new("events"),
        TableType::Stream,
        NamespaceId::new("test"),
        "local".to_string(),
    );

    let provider = Arc::new(StreamTableProvider::new(
        table_metadata,
        schema.clone(),
        stream_store.clone(),
        None, // No TTL for this test
        false,
        None,
    ));

    // Create DataFusion context and register table
    let ctx = SessionContext::new();
    ctx.register_table("events", provider.clone())
        .expect("Failed to register table");

    // Insert test data
    let event1 = serde_json::json!({
        "event_id": "evt1",
        "event_type": "click",
        "value": 100
    });

    provider
        .insert_event("evt1", event1)
        .expect("Failed to insert event");

    // Test projection (SELECT specific columns)
    let df = ctx
        .sql("SELECT event_id, value FROM events")
        .await
        .expect("Failed to execute SELECT with projection");
    let results = df.collect().await.expect("Failed to collect results");

    assert_eq!(results.len(), 1, "Expected 1 batch");
    let batch = &results[0];
    assert_eq!(batch.num_columns(), 2, "Expected 2 columns");
    assert_eq!(batch.num_rows(), 1, "Expected 1 row");

    println!("✅ SELECT with projection works");
}

#[tokio::test]
async fn test_stream_table_select_with_limit() {
    // Create test database
    let test_db = TestDb::new(&["test:events"]).expect("Failed to create test DB");
    let backend: Arc<dyn kalamdb_commons::storage::StorageBackend> =
        Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let stream_store = Arc::new(StreamTableStore::new(backend.clone()));

    // Create stream table schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
    ]));

    // Create stream table provider
    let table_metadata = kalamdb_core::catalog::TableMetadata::new(
        TableName::new("events"),
        TableType::Stream,
        NamespaceId::new("test"),
        "local".to_string(),
    );

    let provider = Arc::new(StreamTableProvider::new(
        table_metadata,
        schema.clone(),
        stream_store.clone(),
        None,
        false,
        None,
    ));

    // Create DataFusion context and register table
    let ctx = SessionContext::new();
    ctx.register_table("events", provider.clone())
        .expect("Failed to register table");

    // Insert multiple events
    for i in 1..=5 {
        let event = serde_json::json!({
            "event_id": format!("evt{}", i),
            "value": i * 100
        });
        provider
            .insert_event(&format!("evt{}", i), event)
            .expect("Failed to insert event");
    }

    // Test LIMIT
    let df = ctx
        .sql("SELECT * FROM events LIMIT 3")
        .await
        .expect("Failed to execute SELECT with LIMIT");
    let results = df.collect().await.expect("Failed to collect results");

    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 3, "Expected 3 rows with LIMIT 3");

    println!("✅ SELECT with LIMIT works");
}
