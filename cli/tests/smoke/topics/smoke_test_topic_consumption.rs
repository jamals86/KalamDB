//! Smoke tests for topic consumption (consume + ack) with CDC events
//!
//! Tests comprehensive topic consumption scenarios including:
//! - INSERT/UPDATE/DELETE operations on tables
//! - Consumer groups and offset tracking
//! - Starting from earliest/latest offsets
//!
//! **Requirements**: Running KalamDB server with Topics feature enabled

use crate::common;
use kalam_link::consumer::{AutoOffsetReset, TopicOp};

/// Create a test client using common infrastructure
async fn create_test_client() -> kalam_link::KalamLinkClient {
    let ctx = common::get_test_context();
    common::create_client_with_credentials(&ctx.username, &ctx.password).await
}

/// Execute SQL via HTTP helper
async fn execute_sql(sql: &str) {
    common::execute_sql_via_http_as_root(sql)
        .await
        .expect("Failed to execute SQL");
}

/// Helper to parse JSON payload from binary
fn parse_payload(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("Failed to parse payload")
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_insert_events() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("events_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);

    // Setup
    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, name TEXT, value DOUBLE) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    // Insert test data
    for i in 1..=5 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, name, value) VALUES ({}, 'test{}', {})",
            namespace,
            table,
            i,
            i,
            i as f64 * 1.5
        ))
        .await;
    }

    // Consume messages
    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-insert-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .max_poll_records(10)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");
    assert_eq!(records.len(), 5, "Should receive 5 INSERT events");

    for record in &records {
        assert_eq!(record.op, TopicOp::Insert);
        let payload = parse_payload(&record.payload);
        assert!(payload.get("id").is_some());
        consumer.mark_processed(record);
    }

    let result = consumer.commit_sync().await.expect("Failed to commit");
    assert_eq!(result.acknowledged_offset, 4);

    // Cleanup
    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_update_events() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("updates_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);

    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, status TEXT, counter INT) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    execute_sql(&format!(
        "INSERT INTO {}.{} (id, status, counter) VALUES (1, 'pending', 0)",
        namespace, table
    ))
    .await;

    for i in 1..=3 {
        execute_sql(&format!(
            "UPDATE {}.{} SET status = 'active', counter = {} WHERE id = 1",
            namespace, table, i
        ))
        .await;
    }

    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-update-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");
    assert_eq!(records.len(), 4, "Should receive 1 INSERT + 3 UPDATEs");

    let inserts = records.iter().filter(|r| r.op == TopicOp::Insert).count();
    let updates = records.iter().filter(|r| r.op == TopicOp::Update).count();
    assert_eq!(inserts, 1);
    assert_eq!(updates, 3);

    for record in &records {
        consumer.mark_processed(record);
    }
    consumer.commit_sync().await.expect("Failed to commit");

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_delete_events() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("deletes_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);

    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, name TEXT) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    for i in 1..=5 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, name) VALUES ({}, 'record{}')",
            namespace, table, i, i
        ))
        .await;
    }

    execute_sql(&format!(
        "DELETE FROM {}.{} WHERE id IN (2, 4)",
        namespace, table
    ))
    .await;

    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-delete-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");
    assert!(records.len() >= 7, "Should receive at least 5 INSERTs + 2 DELETEs");

    let deletes: Vec<_> = records
        .iter()
        .filter(|r| r.op == TopicOp::Delete)
        .collect();
    assert!(deletes.len() >= 2);

    for record in &records {
        consumer.mark_processed(record);
    }
    consumer.commit_sync().await.ok();

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_mixed_operations() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("mixed_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);

    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, data TEXT, version INT) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    // Mixed operations: INSERT, UPDATE, INSERT, DELETE, UPDATE
    execute_sql(&format!(
        "INSERT INTO {}.{} (id, data, version) VALUES (1, 'initial', 1)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "UPDATE {}.{} SET data = 'updated', version = 2 WHERE id = 1",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "INSERT INTO {}.{} (id, data, version) VALUES (2, 'second', 1)",
        namespace, table
    ))
    .await;
    execute_sql(&format!("DELETE FROM {}.{} WHERE id = 1", namespace, table)).await;
    execute_sql(&format!(
        "UPDATE {}.{} SET version = 2 WHERE id = 2",
        namespace, table
    ))
    .await;

    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-mixed-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");
    assert_eq!(records.len(), 5, "Should receive 5 events");

    let inserts = records.iter().filter(|r| r.op == TopicOp::Insert).count();
    let updates = records.iter().filter(|r| r.op == TopicOp::Update).count();
    let deletes = records.iter().filter(|r| r.op == TopicOp::Delete).count();

    assert_eq!(inserts, 2);
    assert_eq!(updates, 2);
    assert_eq!(deletes, 1);

    for record in &records {
        consumer.mark_processed(record);
    }
    consumer.commit_sync().await.ok();

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_offset_persistence() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("offsets_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);
    let group_id = format!("test-offset-group-{}", chrono::Utc::now().timestamp_millis());

    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, data TEXT) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    // Insert first batch
    for i in 1..=3 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, data) VALUES ({}, 'batch1-{}')",
            namespace, table, i, i
        ))
        .await;
    }

    // First consumer: consume and commit first batch
    {
        let client = create_test_client().await;
        let mut consumer = client
            .consumer()
            .topic(&topic)
            .group_id(&group_id)
            .auto_offset_reset(AutoOffsetReset::Earliest)
            .build()
            .expect("Failed to build consumer");

        let records = consumer.poll().await.expect("Failed to poll");
        assert_eq!(records.len(), 3);

        for record in &records {
            consumer.mark_processed(record);
        }
        consumer.commit_sync().await.expect("Failed to commit");
    }

    // Insert second batch
    for i in 4..=6 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, data) VALUES ({}, 'batch2-{}')",
            namespace, table, i, i
        ))
        .await;
    }

    // Second consumer with same group: should only see new messages
    {
        let client = create_test_client().await;
        let mut consumer = client
            .consumer()
            .topic(&topic)
            .group_id(&group_id)
            .auto_offset_reset(AutoOffsetReset::Latest)
            .build()
            .expect("Failed to build consumer");

        let records = consumer.poll().await.expect("Failed to poll");
        assert_eq!(records.len(), 3, "Should receive only batch 2");

        for record in &records {
            let payload = parse_payload(&record.payload);
            let id = payload.get("id").and_then(|v| v.as_i64()).unwrap();
            assert!(id >= 4 && id <= 6);
            consumer.mark_processed(record);
        }
        consumer.commit_sync().await.ok();
    }

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_from_earliest() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("earliest_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);

    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, msg TEXT) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    for i in 1..=10 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, msg) VALUES ({}, 'msg{}')",
            namespace, table, i, i
        ))
        .await;
    }

    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-earliest-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .max_poll_records(20)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");
    assert_eq!(records.len(), 10, "Should receive all 10 messages");

    for (i, record) in records.iter().enumerate() {
        assert_eq!(record.offset, i as u64);
    }

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_from_latest() {
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("latest_{}", chrono::Utc::now().timestamp_millis());
    let topic = format!("{}.{}", namespace, table);

    execute_sql(&format!("CREATE NAMESPACE {}", namespace)).await;
    execute_sql(&format!(
        "CREATE TABLE {}.{} (id INT, msg TEXT) PRIMARY KEY (id)",
        namespace, table
    ))
    .await;
    execute_sql(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await;

    // Insert old messages
    for i in 1..=5 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, msg) VALUES ({}, 'old{}')",
            namespace, table, i, i
        ))
        .await;
    }

    // Start consumer from latest
    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-latest-group")
        .auto_offset_reset(AutoOffsetReset::Latest)
        .build()
        .expect("Failed to build consumer");

    // Insert new messages
    for i in 6..=10 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, msg) VALUES ({}, 'new{}')",
            namespace, table, i, i
        ))
        .await;
    }

    let records = consumer.poll().await.expect("Failed to poll");
    assert!(records.len() >= 5);

    for record in &records {
        let payload = parse_payload(&record.payload);
        let msg = payload.get("msg").and_then(|v| v.as_str()).unwrap();
        assert!(msg.starts_with("new"));
    }

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}
//!
//! Tests comprehensive topic consumption scenarios including:
//! - INSERT/UPDATE/DELETE operations on tables
//! - Consumer groups and offset tracking
//! - Starting from earliest/latest offsets
//! - Limit and timeout handling
//! - Acknowledgment and offset commits
//!
//! **Requirements**: Running KalamDB server with Topics feature enabled
//!
//! Run with:
//!   cargo test --test smoke topic_consumption

use crate::common;
use kalam_link::consumer::AutoOffsetReset;
use serde_json::json;

/// Create a test client using common infrastructure
async fn create_test_client() -> kalam_link::KalamLinkClient {
    let ctx = common::get_test_context();
    common::create_client_with_credentials(&ctx.username, &ctx.password).await
}

/// Execute SQL via HTTP helper
async fn execute_sql(sql: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    common::execute_sql_via_http_as_root(sql).await
}

/// Helper to parse JSON payload from binary
fn parse_payload(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("Failed to parse payload")
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_insert_events() {
    let _client = create_test_client().await;

    // Create namespace and table
    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("events_{}", chrono::Utc::now().timestamp_millis());

    common::execute_sql_via_http_as_root(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    common::execute_sql_via_http_as_root(&format!(
        "CREATE TABLE {}.{} (id INT, name TEXT, value DOUBLE) PRIMARY KEY (id)",
        namespace, table
    ))
    .await
    .expect("Failed to create table");

    // Create topic
    let topic = format!("{}.{}", namespace, table);

    common::execute_sql_via_http_as_root(&format!(
        "CREATE TOPIC {} SOURCE TABLE {}.{}",
        topic, namespace, table
    ))
    .await
    .expect("Failed to create topic");

    // Insert some test data
    for i in 1..=5 {
        common::execute_sql_via_http_as_root(&format!(
            "INSERT INTO {}.{} (id, name, value) VALUES ({}, 'test{}', {})",
            namespace, table, i, i, i as f64 * 1.5
        ))
        .await
        .expect("Failed to insert");
    }

    // Consume messages using consumer
    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-insert-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .max_poll_records(10)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");

    assert_eq!(records.len(), 5, "Should receive 5 INSERT events");

    // Verify all are INSERT operations
    for record in &records {
        assert_eq!(record.op, kalam_link::consumer::TopicOp::Insert);
        
        // Parse payload as JSON
        let payload: serde_json::Value = serde_json::from_slice(&record.payload)
            .expect("Failed to parse payload");
        
        assert!(payload.get("id").is_some());
        assert!(payload.get("name").is_some());
        assert!(payload.get("value").is_some());
        consumer.mark_processed(record);
    }

    // Commit offsets
    let result = consumer.commit_sync().await.expect("Failed to commit");
    assert_eq!(result.acknowledged_offset, 4, "Should commit offset 4 (0-based)");

    // Cleanup
    common::execute_sql_via_http_as_root(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    common::execute_sql_via_http_as_root(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    common::execute_sql_via_http_as_root(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_update_events() {
    let client = create_test_client().await;

    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("updates_{}", chrono::Utc::now().timestamp_millis());

    client
        .execute(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    client
        .execute(&format!(
            "CREATE TABLE {}.{} (id INT, status TEXT, counter INT) PRIMARY KEY (id)",
            namespace, table
        ))
        .await
        .expect("Failed to create table");

    let topic = format!("{}.{}", namespace, table);

    client
        .execute(&format!(
            "CREATE TOPIC {} SOURCE TABLE {}.{}",
            topic, namespace, table
        ))
        .await
        .expect("Failed to create topic");

    // Insert initial record
    client
        .execute(&format!(
            "INSERT INTO {}.{} (id, status, counter) VALUES (1, 'pending', 0)",
            namespace, table
        ))
        .await
        .expect("Failed to insert");

    // Update the record multiple times
    for i in 1..=3 {
        client
            .execute(&format!(
                "UPDATE {}.{} SET status = 'active', counter = {} WHERE id = 1",
                namespace, table, i
            ))
            .await
            .expect("Failed to update");
    }

    // Consume all events
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-update-group")
        .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Earliest)
        .max_poll_records(10)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");

    // Should have 1 INSERT + 3 UPDATEs = 4 events
    assert_eq!(records.len(), 4, "Should receive 1 INSERT + 3 UPDATE events");

    let insert_count = records.iter().filter(|r| r.operation == "INSERT").count();
    let update_count = records.iter().filter(|r| r.operation == "UPDATE").count();

    assert_eq!(insert_count, 1, "Should have 1 INSERT event");
    assert_eq!(update_count, 3, "Should have 3 UPDATE events");

    // Verify last update has counter = 3
    let last_update = records
        .iter()
        .filter(|r| r.operation == "UPDATE")
        .last()
        .expect("Should have UPDATE event");

    assert_eq!(
        last_update.payload.get("counter").and_then(|v| v.as_i64()),
        Some(3),
        "Last update should have counter = 3"
    );

    for record in &records {
        consumer.mark_processed(record);
    }

    consumer.commit_sync().await.expect("Failed to commit");

    // Cleanup
    client
        .execute(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    client
        .execute(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    client
        .execute(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_delete_events() {
    let client = create_test_client().await;

    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("deletes_{}", chrono::Utc::now().timestamp_millis());

    client
        .execute(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    client
        .execute(&format!(
            "CREATE TABLE {}.{} (id INT, name TEXT) PRIMARY KEY (id)",
            namespace, table
        ))
        .await
        .expect("Failed to create table");

    let topic = format!("{}.{}", namespace, table);

    client
        .execute(&format!(
            "CREATE TOPIC {} SOURCE TABLE {}.{}",
            topic, namespace, table
        ))
        .await
        .expect("Failed to create topic");

    // Insert records
    for i in 1..=5 {
        client
            .execute(&format!(
                "INSERT INTO {}.{} (id, name) VALUES ({}, 'record{}')",
                namespace, table, i, i
            ))
            .await
            .expect("Failed to insert");
    }

    // Delete some records
    client
        .execute(&format!(
            "DELETE FROM {}.{} WHERE id IN (2, 4)",
            namespace, table
        ))
        .await
        .expect("Failed to delete");

    // Consume all events
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-delete-group")
        .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Earliest)
        .max_poll_records(20)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");

    // Should have 5 INSERTs + 2 DELETEs = 7 events
    assert!(
        records.len() >= 7,
        "Should receive at least 5 INSERT + 2 DELETE events"
    );

    let insert_count = records.iter().filter(|r| r.operation == "INSERT").count();
    let delete_count = records.iter().filter(|r| r.operation == "DELETE").count();

    assert!(insert_count >= 5, "Should have at least 5 INSERT events");
    assert!(delete_count >= 2, "Should have at least 2 DELETE events");

    // Verify deleted IDs
    let deleted_ids: Vec<i64> = records
        .iter()
        .filter(|r| r.operation == "DELETE")
        .filter_map(|r| r.payload.get("id").and_then(|v| v.as_i64()))
        .collect();

    assert!(deleted_ids.contains(&2), "Should have deleted id=2");
    assert!(deleted_ids.contains(&4), "Should have deleted id=4");

    for record in &records {
        consumer.mark_processed(record);
    }

    consumer.commit_sync().await.expect("Failed to commit");

    // Cleanup
    client
        .execute(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    client
        .execute(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    client
        .execute(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_mixed_operations() {
    let client = create_test_client().await;

    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("mixed_{}", chrono::Utc::now().timestamp_millis());

    client
        .execute(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    client
        .execute(&format!(
            "CREATE TABLE {}.{} (id INT, data TEXT, version INT) PRIMARY KEY (id)",
            namespace, table
        ))
        .await
        .expect("Failed to create table");

    let topic = format!("{}.{}", namespace, table);

    client
        .execute(&format!(
            "CREATE TOPIC {} SOURCE TABLE {}.{}",
            topic, namespace, table
        ))
        .await
        .expect("Failed to create topic");

    // Execute mixed operations
    // INSERT id=1
    client
        .execute(&format!(
            "INSERT INTO {}.{} (id, data, version) VALUES (1, 'initial', 1)",
            namespace, table
        ))
        .await
        .expect("Failed to insert");

    // UPDATE id=1
    client
        .execute(&format!(
            "UPDATE {}.{} SET data = 'updated', version = 2 WHERE id = 1",
            namespace, table
        ))
        .await
        .expect("Failed to update");

    // INSERT id=2
    client
        .execute(&format!(
            "INSERT INTO {}.{} (id, data, version) VALUES (2, 'second', 1)",
            namespace, table
        ))
        .await
        .expect("Failed to insert");

    // DELETE id=1
    client
        .execute(&format!("DELETE FROM {}.{} WHERE id = 1", namespace, table))
        .await
        .expect("Failed to delete");

    // UPDATE id=2
    client
        .execute(&format!(
            "UPDATE {}.{} SET version = 2 WHERE id = 2",
            namespace, table
        ))
        .await
        .expect("Failed to update");

    // Consume all events
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-mixed-group")
        .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Earliest)
        .max_poll_records(20)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");

    // Should have 2 INSERTs + 2 UPDATEs + 1 DELETE = 5 events
    assert_eq!(records.len(), 5, "Should receive 5 mixed events");

    let insert_count = records.iter().filter(|r| r.operation == "INSERT").count();
    let update_count = records.iter().filter(|r| r.operation == "UPDATE").count();
    let delete_count = records.iter().filter(|r| r.operation == "DELETE").count();

    assert_eq!(insert_count, 2, "Should have 2 INSERT events");
    assert_eq!(update_count, 2, "Should have 2 UPDATE events");
    assert_eq!(delete_count, 1, "Should have 1 DELETE event");

    // Verify event sequence
    assert_eq!(records[0].operation, "INSERT", "First should be INSERT");
    assert_eq!(records[1].operation, "UPDATE", "Second should be UPDATE");
    assert_eq!(records[2].operation, "INSERT", "Third should be INSERT");
    assert_eq!(records[3].operation, "DELETE", "Fourth should be DELETE");
    assert_eq!(records[4].operation, "UPDATE", "Fifth should be UPDATE");

    for record in &records {
        consumer.mark_processed(record);
    }

    consumer.commit_sync().await.expect("Failed to commit");

    // Cleanup
    client
        .execute(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    client
        .execute(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    client
        .execute(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_offset_persistence() {
    let client = create_test_client().await;

    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("offsets_{}", chrono::Utc::now().timestamp_millis());
    let group_id = format!("test-offset-group-{}", chrono::Utc::now().timestamp_millis());

    client
        .execute(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    client
        .execute(&format!(
            "CREATE TABLE {}.{} (id INT, data TEXT) PRIMARY KEY (id)",
            namespace, table
        ))
        .await
        .expect("Failed to create table");

    let topic = format!("{}.{}", namespace, table);

    client
        .execute(&format!(
            "CREATE TOPIC {} SOURCE TABLE {}.{}",
            topic, namespace, table
        ))
        .await
        .expect("Failed to create topic");

    // Insert first batch
    for i in 1..=3 {
        client
            .execute(&format!(
                "INSERT INTO {}.{} (id, data) VALUES ({}, 'batch1-{}')",
                namespace, table, i, i
            ))
            .await
            .expect("Failed to insert");
    }

    // First consumer: consume and commit first batch
    {
        let mut consumer = client
            .consumer()
            .topic(&topic)
            .group_id(&group_id)
            .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Earliest)
            .build()
            .expect("Failed to build consumer");

        let records = consumer.poll().await.expect("Failed to poll");
        assert_eq!(records.len(), 3, "Should receive 3 records");

        for record in &records {
            consumer.mark_processed(record);
        }

        consumer.commit_sync().await.expect("Failed to commit");
    }

    // Insert second batch
    for i in 4..=6 {
        client
            .execute(&format!(
                "INSERT INTO {}.{} (id, data) VALUES ({}, 'batch2-{}')",
                namespace, table, i, i
            ))
            .await
            .expect("Failed to insert");
    }

    // Second consumer with same group: should only see new messages
    {
        let mut consumer = client
            .consumer()
            .topic(&topic)
            .group_id(&group_id)
            .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Latest)
            .build()
            .expect("Failed to build consumer");

        let records = consumer.poll().await.expect("Failed to poll");

        // Should only receive batch 2 (3 new records)
        assert_eq!(
            records.len(),
            3,
            "Should receive only 3 new records from batch 2"
        );

        // Verify they are from batch 2
        for record in &records {
            let id = record.payload.get("id").and_then(|v| v.as_i64()).unwrap();
            assert!(id >= 4 && id <= 6, "Should be from batch 2");
            consumer.mark_processed(record);
        }

        consumer.commit_sync().await.expect("Failed to commit");
    }

    // Cleanup
    client
        .execute(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    client
        .execute(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    client
        .execute(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_from_earliest() {
    let client = create_test_client().await;

    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("earliest_{}", chrono::Utc::now().timestamp_millis());

    client
        .execute(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    client
        .execute(&format!(
            "CREATE TABLE {}.{} (id INT, msg TEXT) PRIMARY KEY (id)",
            namespace, table
        ))
        .await
        .expect("Failed to create table");

    let topic = format!("{}.{}", namespace, table);

    client
        .execute(&format!(
            "CREATE TOPIC {} SOURCE TABLE {}.{}",
            topic, namespace, table
        ))
        .await
        .expect("Failed to create topic");

    // Insert messages before consumer starts
    for i in 1..=10 {
        client
            .execute(&format!(
                "INSERT INTO {}.{} (id, msg) VALUES ({}, 'msg{}')",
                namespace, table, i, i
            ))
            .await
            .expect("Failed to insert");
    }

    // Start consumer from earliest
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-earliest-group")
        .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Earliest)
        .max_poll_records(20)
        .build()
        .expect("Failed to build consumer");

    let records = consumer.poll().await.expect("Failed to poll");

    // Should receive all 10 messages
    assert_eq!(records.len(), 10, "Should receive all 10 messages");

    // Verify sequential offsets
    for (i, record) in records.iter().enumerate() {
        assert_eq!(record.offset, i as u64, "Offset should be sequential");
    }

    // Cleanup
    client
        .execute(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    client
        .execute(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    client
        .execute(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}

#[tokio::test]
#[ntest::timeout(120000)]
async fn test_topic_consume_from_latest() {
    let client = create_test_client().await;

    let namespace = format!("smoke_topic_ns_{}", chrono::Utc::now().timestamp());
    let table = format!("latest_{}", chrono::Utc::now().timestamp_millis());

    client
        .execute(&format!("CREATE NAMESPACE {}", namespace))
        .await
        .expect("Failed to create namespace");

    client
        .execute(&format!(
            "CREATE TABLE {}.{} (id INT, msg TEXT) PRIMARY KEY (id)",
            namespace, table
        ))
        .await
        .expect("Failed to create table");

    let topic = format!("{}.{}", namespace, table);

    client
        .execute(&format!(
            "CREATE TOPIC {} SOURCE TABLE {}.{}",
            topic, namespace, table
        ))
        .await
        .expect("Failed to create topic");

    // Insert messages before consumer starts
    for i in 1..=5 {
        client
            .execute(&format!(
                "INSERT INTO {}.{} (id, msg) VALUES ({}, 'old{}')",
                namespace, table, i, i
            ))
            .await
            .expect("Failed to insert");
    }

    // Start consumer from latest (should skip old messages)
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-latest-group")
        .auto_offset_reset(kalam_link::consumer::AutoOffsetReset::Latest)
        .max_poll_records(20)
        .build()
        .expect("Failed to build consumer");

    // Insert new messages after consumer starts
    for i in 6..=10 {
        client
            .execute(&format!(
                "INSERT INTO {}.{} (id, msg) VALUES ({}, 'new{}')",
                namespace, table, i, i
            ))
            .await
            .expect("Failed to insert");
    }

    let records = consumer.poll().await.expect("Failed to poll");

    // Should only receive new messages (5 messages: 6-10)
    assert!(
        records.len() >= 5,
        "Should receive at least 5 new messages"
    );

    // Verify messages are from the new batch
    for record in &records {
        let msg = record
            .payload
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap();
        assert!(
            msg.starts_with("new"),
            "Should only receive new messages"
        );
    }

    // Cleanup
    client
        .execute(&format!("DROP TOPIC {}", topic))
        .await
        .ok();
    client
        .execute(&format!("DROP TABLE {}.{}", namespace, table))
        .await
        .ok();
    client
        .execute(&format!("DROP NAMESPACE {}", namespace))
        .await
        .ok();
}
