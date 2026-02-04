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
use kalam_link::{KalamLinkClient, KalamLinkTimeouts};
use std::time::Duration;

/// Create a test client using common infrastructure
async fn create_test_client() -> kalam_link::KalamLinkClient {
    let base_url = common::leader_or_server_url();
    KalamLinkClient::builder()
        .base_url(&base_url)
        .auth(common::auth_provider_for_user_on_url(
            &base_url,
            "root",
            common::root_password(),
        ))
        .timeouts(
            KalamLinkTimeouts::builder()
                .connection_timeout_secs(5)
                .receive_timeout_secs(120)
                .send_timeout_secs(30)
                .subscribe_timeout_secs(10)
                .auth_timeout_secs(10)
                .initial_data_timeout(Duration::from_secs(120))
                .build(),
        )
        .build()
        .expect("Failed to build test client")
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, name TEXT, value DOUBLE)",
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, status TEXT, counter INT)",
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, name TEXT)",
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, data TEXT, version INT)",
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, data TEXT)",
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, msg TEXT)",
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
        "CREATE TABLE {}.{} (id INT PRIMARY KEY, msg TEXT)",
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
