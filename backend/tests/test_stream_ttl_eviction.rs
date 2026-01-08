#![allow(unused_imports, unused_variables)]
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
use kalamdb_commons::models::datatypes::KalamDataType;
use kalamdb_commons::models::schemas::{
    ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::models::{NamespaceId, Role, TableId, TableName, UserId};
use kalamdb_core::app_context::AppContext;
use kalamdb_core::providers::arrow_json_conversion::json_to_row;
use kalamdb_core::providers::base::{BaseTableProvider, TableProviderCore};
use kalamdb_core::providers::StreamTableProvider;
use kalamdb_core::schema_registry::CachedTableData;
use kalamdb_core::sql::executor::models::ExecutionContext;
use kalamdb_store::test_utils::TestDb;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use kalamdb_tables::{new_stream_table_store, StreamTableStore};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_stream_table_ttl_eviction_with_select() {
    // Create test database
    let test_db = TestDb::new(&["test:test_events"]).expect("Failed to create test DB");
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let table_id = TableId::new(NamespaceId::new("test"), TableName::new("test_events"));
    let stream_store = Arc::new(kalamdb_tables::new_stream_table_store(&table_id));

    // Create stream table schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
    ]));

    // Create stream table provider with 1-second TTL
    // Initialize AppContext and TableProviderCore
    let node_id = kalamdb_commons::NodeId::new(1);
    let config = kalamdb_commons::ServerConfig::default();
    let app_ctx = AppContext::init(
        backend.clone(),
        node_id,
        "/tmp/kalamdb-test".to_string(),
        config,
    );
    let core = Arc::new(TableProviderCore::from_app_context(
        &app_ctx,
        table_id.clone(),
        TableType::Stream,
    ));

    // Prime SchemaRegistry with TableDefinition for this stream table (required by provider.schema_ref())
    let table_def = TableDefinition::new(
        table_id.namespace_id().clone(),
        table_id.table_name().clone(),
        TableType::Stream,
        vec![
            ColumnDefinition::new(
                1,
                "event_id".to_string(),
                1,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                2,
                "event_type".to_string(),
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                3,
                "value".to_string(),
                3,
                KalamDataType::Int,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
        ],
        TableOptions::stream(1),
        None,
    )
    .expect("Failed to build TableDefinition");
    app_ctx.schema_registry().insert(
        table_id.clone(),
        Arc::new(CachedTableData::new(Arc::new(table_def))),
    );

    let provider = Arc::new(StreamTableProvider::new(
        core.clone(),
        stream_store.clone(),
        Some(1), // 1-second TTL
        "event_id".to_string(),
    ));

    // Create per-user SessionContext with user injected
    let exec_ctx = ExecutionContext::new(
        UserId::new("u1"),
        Role::User,
        app_ctx.base_session_context(),
    );
    let ctx = exec_ctx.create_session_with_user();
    ctx.register_table("test_events", provider.clone())
        .expect("Failed to register table");

    // Insert test data using the provider directly
    let event1 = json_to_row(&serde_json::json!({
        "event_id": "evt1",
        "event_type": "click",
        "value": 100
    }))
    .unwrap();
    let event2 = json_to_row(&serde_json::json!({
        "event_id": "evt2",
        "event_type": "view",
        "value": 200
    }))
    .unwrap();
    let event3 = json_to_row(&serde_json::json!({
        "event_id": "evt3",
        "event_type": "purchase",
        "value": 300
    }))
    .unwrap();

    provider
        .insert(&UserId::new("u1"), event1)
        .expect("Failed to insert event1");
    provider
        .insert(&UserId::new("u1"), event2)
        .expect("Failed to insert event2");
    provider
        .insert(&UserId::new("u1"), event3)
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

    // STEP 2: Wait slightly over TTL and rely on TTL filtering in scans
    tokio::time::sleep(Duration::from_millis(1200)).await;

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
    let test_db = TestDb::new(&["test:events_proj"]).expect("Failed to create test DB");
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let table_id = TableId::new(NamespaceId::new("test"), TableName::new("events_proj"));
    let stream_store = Arc::new(kalamdb_tables::new_stream_table_store(&table_id));

    // Create stream table schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
    ]));

    // Create stream table provider
    // Initialize AppContext and TableProviderCore
    let node_id = kalamdb_commons::NodeId::new(1);
    let config = kalamdb_commons::ServerConfig::default();
    let app_ctx = AppContext::init(
        backend.clone(),
        node_id,
        "/tmp/kalamdb-test".to_string(),
        config,
    );
    let core = Arc::new(TableProviderCore::from_app_context(
        &app_ctx,
        table_id.clone(),
        TableType::Stream,
    ));

    // Prime SchemaRegistry with TableDefinition for this stream table
    let table_def = TableDefinition::new(
        table_id.namespace_id().clone(),
        table_id.table_name().clone(),
        TableType::Stream,
        vec![
            ColumnDefinition::new(
                1,
                "event_id".to_string(),
                1,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                2,
                "event_type".to_string(),
                2,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                3,
                "value".to_string(),
                3,
                KalamDataType::Int,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
        ],
        TableOptions::stream(3600),
        None,
    )
    .expect("Failed to build TableDefinition");
    app_ctx.schema_registry().insert(
        table_id.clone(),
        Arc::new(CachedTableData::new(Arc::new(table_def))),
    );

    let provider = Arc::new(StreamTableProvider::new(
        core.clone(),
        stream_store.clone(),
        None, // No TTL for this test
        "event_id".to_string(),
    ));

    // Create per-user SessionContext
    let exec_ctx = ExecutionContext::new(
        UserId::new("u1"),
        Role::User,
        app_ctx.base_session_context(),
    );
    let ctx = exec_ctx.create_session_with_user();
    ctx.register_table("events_proj", provider.clone())
        .expect("Failed to register table");

    // Insert test data
    let event1 = json_to_row(&serde_json::json!({
        "event_id": "evt1",
        "event_type": "click",
        "value": 100
    }))
    .unwrap();

    provider
        .insert(&UserId::new("u1"), event1)
        .expect("Failed to insert event");

    // Test projection (SELECT specific columns)
    let df = ctx
        .sql("SELECT event_id, value FROM events_proj")
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
    let test_db = TestDb::new(&["test:events_limit"]).expect("Failed to create test DB");
    let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));
    let table_id = TableId::new(NamespaceId::new("test"), TableName::new("events_limit"));
    let stream_store = Arc::new(kalamdb_tables::new_stream_table_store(&table_id));

    // Create stream table schema
    let schema = Arc::new(Schema::new(vec![
        Field::new("event_id", DataType::Utf8, false),
        Field::new("value", DataType::Int64, false),
    ]));

    // Create stream table provider
    // Initialize AppContext and TableProviderCore
    let node_id = kalamdb_commons::NodeId::new(1);
    let config = kalamdb_commons::ServerConfig::default();
    let app_ctx = AppContext::init(
        backend.clone(),
        node_id,
        "/tmp/kalamdb-test".to_string(),
        config,
    );
    let core = Arc::new(TableProviderCore::from_app_context(
        &app_ctx,
        table_id.clone(),
        TableType::Stream,
    ));

    // Prime SchemaRegistry with TableDefinition for this stream table
    let table_def = TableDefinition::new(
        table_id.namespace_id().clone(),
        table_id.table_name().clone(),
        TableType::Stream,
        vec![
            ColumnDefinition::new(
                1,
                "event_id".to_string(),
                1,
                KalamDataType::Text,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                2,
                "value".to_string(),
                2,
                KalamDataType::Int,
                false,
                false,
                false,
                kalamdb_commons::models::schemas::ColumnDefault::None,
                None,
            ),
        ],
        TableOptions::stream(3600),
        None,
    )
    .expect("Failed to build TableDefinition");
    app_ctx.schema_registry().insert(
        table_id.clone(),
        Arc::new(CachedTableData::new(Arc::new(table_def))),
    );

    let provider = Arc::new(StreamTableProvider::new(
        core.clone(),
        stream_store.clone(),
        None,
        "event_id".to_string(),
    ));

    let exec_ctx = ExecutionContext::new(
        UserId::new("u1"),
        Role::User,
        app_ctx.base_session_context(),
    );
    let ctx = exec_ctx.create_session_with_user();
    ctx.register_table("events_limit", provider.clone())
        .expect("Failed to register table");

    // Insert multiple events
    for i in 1..=5 {
        let event = json_to_row(&serde_json::json!({
            "event_id": format!("evt{}", i),
            "value": i * 100
        }))
        .unwrap();
        provider
            .insert(&UserId::new("u1"), event)
            .expect("Failed to insert event");
    }

    // Test LIMIT
    let df = ctx
        .sql("SELECT * FROM events_limit LIMIT 3")
        .await
        .expect("Failed to execute SELECT with LIMIT");
    let results = df.collect().await.expect("Failed to collect results");

    let total_rows: usize = results.iter().map(|b| b.num_rows()).sum();
    assert_eq!(total_rows, 3, "Expected 3 rows with LIMIT 3");

    println!("✅ SELECT with LIMIT works");
}
