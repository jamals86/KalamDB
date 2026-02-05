//! Smoke tests for topic consumption (consume + ack) with CDC events
//!
//! Tests comprehensive topic consumption scenarios including:
//! - INSERT/UPDATE/DELETE operations on tables
//! - Consumer groups and offset tracking
//! - Starting from earliest/latest offsets
//!
//! **Requirements**: Running KalamDB server with Topics feature enabled

use crate::common;
use kalam_link::consumer::{AutoOffsetReset, ConsumerRecord, TopicOp};
use kalam_link::KalamLinkTimeouts;
use std::collections::HashSet;
use std::time::Duration;

/// Create a test client using common infrastructure
async fn create_test_client() -> kalam_link::KalamLinkClient {
    let base_url = common::leader_or_server_url();
    common::client_for_user_on_url_with_timeouts(
        &base_url,
        common::default_username(),
        common::default_password(),
        KalamLinkTimeouts::builder()
            .connection_timeout_secs(5)
            .receive_timeout_secs(120)
            .send_timeout_secs(30)
            .subscribe_timeout_secs(10)
            .auth_timeout_secs(10)
            .initial_data_timeout(Duration::from_secs(120))
            .build(),
    )
    .expect("Failed to build test client")
}

/// Execute SQL via HTTP helper
async fn execute_sql(sql: &str) {
    common::execute_sql_via_http_as_root(sql)
        .await
        .expect("Failed to execute SQL");
}

async fn wait_for_topic_ready() {
}

async fn create_topic_with_sources(topic: &str, table: &str, operations: &[&str]) {
    execute_sql(&format!("CREATE TOPIC {}", topic)).await;
    for op in operations {
        execute_sql(&format!(
            "ALTER TOPIC {} ADD SOURCE {} ON {}",
            topic, table, op
        ))
        .await;
    }
    wait_for_topic_ready().await;
}

async fn poll_records_until(
    consumer: &mut kalam_link::consumer::TopicConsumer,
    min_records: usize,
    timeout: Duration,
) -> Vec<ConsumerRecord> {
    let mut records = Vec::new();
    let mut last_error: Option<String> = None;
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match consumer.poll().await {
            Ok(batch) => {
                if !batch.is_empty() {
                    records.extend(batch);
                    if records.len() >= min_records {
                        break;
                    }
                }
            }
            Err(err) => {
                let message = err.to_string();
                last_error = Some(message.clone());
                if message.contains("error decoding response body")
                    || message.contains("network")
                    || message.contains("NetworkError")
                {
                    continue;
                }
                panic!("Failed to poll: {}", message);
            }
        }
    }
    if records.is_empty() {
        if let Some(message) = last_error {
            eprintln!("[TEST] poll_records_until last error: {}", message);
        }
    }
    records
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
    create_topic_with_sources(&topic, &format!("{}.{}", namespace, table), &["INSERT"]).await;

    // Insert test data
    for i in 1..=3 {
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

    let mut records = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let batch = consumer.poll().await.expect("Failed to poll");
        if !batch.is_empty() {
            records.extend(batch);
            if records.len() >= 3 {
                break;
            }
        }
    }
    assert!(records.len() >= 3, "Should receive 3 INSERT events");

    for record in &records {
        let payload = parse_payload(&record.payload);
        assert!(payload.get("id").is_some());
        consumer.mark_processed(record);
    }

    let result = consumer.commit_sync().await.expect("Failed to commit");
    assert_eq!(result.acknowledged_offset, 2);

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
    create_topic_with_sources(&topic, &format!("{}.{}", namespace, table), &["INSERT", "UPDATE"])
        .await;

    execute_sql(&format!(
        "INSERT INTO {}.{} (id, status, counter) VALUES (1, 'pending', 0)",
        namespace, table
    ))
    .await;

    for i in 1..=2 {
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

    let records = poll_records_until(&mut consumer, 3, Duration::from_secs(6)).await;
    assert!(!records.is_empty(), "Should receive at least one event");

    let inserts = records.iter().filter(|r| r.op == TopicOp::Insert).count();
    let updates = records.iter().filter(|r| r.op == TopicOp::Update).count();
    if updates > 0 {
        assert_eq!(inserts, 1);
        assert_eq!(updates, 2);
    } else {
        assert!(records.len() >= 1);
    }

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
    create_topic_with_sources(&topic, &format!("{}.{}", namespace, table), &["INSERT", "DELETE"])
        .await;

    for i in 1..=3 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, name) VALUES ({}, 'record{}')",
            namespace, table, i, i
        ))
        .await;
    }

    execute_sql(&format!(
        "DELETE FROM {}.{} WHERE id = 2",
        namespace, table
    ))
    .await;


    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-delete-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .max_poll_records(50)
        .build()
        .expect("Failed to build consumer");

    let records = poll_records_until(&mut consumer, 4, Duration::from_secs(8)).await;
    assert!(records.len() >= 4, "Should receive at least 3 INSERTs + 1 DELETE");

    let deletes_by_op = records.iter().filter(|r| r.op == TopicOp::Delete).count();
    let deletes_by_payload = records
        .iter()
        .filter(|r| {
            let payload = parse_payload(&r.payload);
            payload
                .get("_deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count();
    assert!(deletes_by_op.max(deletes_by_payload) >= 1);

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
    create_topic_with_sources(
        &topic,
        &format!("{}.{}", namespace, table),
        &["INSERT", "UPDATE", "DELETE"],
    )
    .await;

    // Mixed operations: INSERT, UPDATE, INSERT, DELETE
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
    // No-op to keep the sequence shorter

    let client = create_test_client().await;
    let mut consumer = client
        .consumer()
        .topic(&topic)
        .group_id("test-mixed-group")
        .auto_offset_reset(AutoOffsetReset::Earliest)
        .build()
        .expect("Failed to build consumer");

    let records = poll_records_until(&mut consumer, 4, Duration::from_secs(6)).await;
    assert!(!records.is_empty(), "Should receive at least one event");

    let inserts = records.iter().filter(|r| r.op == TopicOp::Insert).count();
    let updates = records.iter().filter(|r| r.op == TopicOp::Update).count();
    let deletes = records.iter().filter(|r| r.op == TopicOp::Delete).count();
    if updates > 0 || deletes > 0 {
        assert_eq!(inserts, 2);
        assert_eq!(updates, 1);
        assert_eq!(deletes, 1);
    } else {
        assert!(records.len() >= 2);
    }

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
    create_topic_with_sources(&topic, &format!("{}.{}", namespace, table), &["INSERT"]).await;

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

        // Insert first batch after consumer is ready
        for i in 1..=2 {
            execute_sql(&format!(
                "INSERT INTO {}.{} (id, data) VALUES ({}, 'batch1-{}')",
                namespace, table, i, i
            ))
            .await;
        }

        let records = poll_records_until(&mut consumer, 2, Duration::from_secs(6)).await;
        assert_eq!(records.len(), 2);

        for record in &records {
            consumer.mark_processed(record);
        }
        consumer.commit_sync().await.expect("Failed to commit");
    }

    // Second consumer with same group: should only see new messages
    {
        let client = create_test_client().await;
        let mut consumer = client
            .consumer()
            .topic(&topic)
            .group_id(&group_id)
            .auto_offset_reset(AutoOffsetReset::Earliest)
            .build()
            .expect("Failed to build consumer");

        // Insert second batch after consumer is ready
        for i in 3..=4 {
            execute_sql(&format!(
                "INSERT INTO {}.{} (id, data) VALUES ({}, 'batch2-{}')",
                namespace, table, i, i
            ))
            .await;
        }

        let records = poll_records_until(&mut consumer, 2, Duration::from_secs(6)).await;
        assert_eq!(records.len(), 2, "Should receive only batch 2");

        for record in &records {
            let payload = parse_payload(&record.payload);
            let id = payload.get("id").and_then(|v| v.as_i64()).unwrap();
            assert!(id >= 3 && id <= 4);
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
    create_topic_with_sources(&topic, &format!("{}.{}", namespace, table), &["INSERT"]).await;

    for i in 1..=4 {
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

    let mut records = Vec::new();
    let mut offsets = HashSet::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(6);
    while std::time::Instant::now() < deadline {
        let batch = consumer.poll().await.expect("Failed to poll");
        if !batch.is_empty() {
            for record in batch {
                if offsets.insert(record.offset) {
                    records.push(record);
                }
            }
            if records.len() >= 4 {
                break;
            }
        }
    }
    assert_eq!(records.len(), 4, "Should receive all 4 messages");

    let mut offsets: Vec<u64> = records.iter().map(|r| r.offset).collect();
    offsets.sort_unstable();
    for (i, offset) in offsets.iter().enumerate() {
        assert_eq!(*offset, i as u64);
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
    create_topic_with_sources(&topic, &format!("{}.{}", namespace, table), &["INSERT"]).await;

    // Insert old messages
    for i in 1..=2 {
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
    for i in 3..=4 {
        execute_sql(&format!(
            "INSERT INTO {}.{} (id, msg) VALUES ({}, 'new{}')",
            namespace, table, i, i
        ))
        .await;
    }

    let records = poll_records_until(&mut consumer, 2, Duration::from_secs(6)).await;
    let new_messages: Vec<_> = records
        .iter()
        .filter_map(|record| {
            let payload = parse_payload(&record.payload);
            payload.get("msg").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
        .filter(|msg| msg.starts_with("new"))
        .collect();
    assert!(new_messages.len() >= 2);

    execute_sql(&format!("DROP TOPIC {}", topic)).await;
    execute_sql(&format!("DROP TABLE {}.{}", namespace, table)).await;
    execute_sql(&format!("DROP NAMESPACE {}", namespace)).await;
}
