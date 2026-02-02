//! Integration tests for Topic Pub/Sub system
//!
//! These tests verify:
//! - Topic creation and management (CREATE TOPIC, ALTER TOPIC ADD SOURCE, DROP TOPIC)
//! - CDC integration (automatic message routing from table changes)
//! - Message consumption (CONSUME with different positions)
//! - Offset tracking (ACK and consumer groups)
//! - Authorization (role-based access control)
//! - Long polling behavior
//!
//! NOTE: These are simplified smoke tests. Full implementation will require:
//! 1. HTTP API test client for /v1/api/topics endpoints
//! 2. Async notification verification helpers
//! 3. Extended timeout handling for CDC workflows

use crate::test_support::*;
use kalam_link::models::ResponseStatus;

/// Test basic topic creation via SQL
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_create_topic_basic() {
    let server = TestServer::new_shared().await;

    let sql = "CREATE TOPIC user_events_topic PARTITIONS 1";
    let result = server.execute_sql(sql).await;
    
    // Basic smoke test - verify command executes (or already exists)
    assert!(
        result.status == ResponseStatus::Success 
            || result.error.as_ref().map(|e| e.message.contains("already exists")).unwrap_or(false),
        "CREATE TOPIC failed: {:?}",
        result.error
    );
}

/// Test ALTER TOPIC ADD SOURCE
#[tokio::test]
#[ntest::timeout(15000)]
async fn test_alter_topic_add_source() {
    let server = TestServer::new_shared().await;

    // Setup
    server.execute_sql("CREATE NAMESPACE test_alter_ns").await;
    server.execute_sql("CREATE TABLE test_alter_ns.data (id TEXT PRIMARY KEY, value TEXT)").await;
    server.execute_sql("CREATE TOPIC data_changes_tp PARTITIONS 1").await;

    // Add source - Syntax: ALTER TOPIC <name> ADD SOURCE <table> ON <operation>
    let sql = "ALTER TOPIC data_changes_tp ADD SOURCE test_alter_ns.data ON INSERT";
    let result = server.execute_sql(sql).await;
    
    assert!(
        result.status == ResponseStatus::Success,
        "ALTER TOPIC ADD SOURCE failed: {:?}",
        result.error
    );
}

/// Test CONSUME basic functionality
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_consume_from_topic() {
    let server = TestServer::new_shared().await;

    // Setup topic
    server.execute_sql("CREATE TOPIC test_consume_tp PARTITIONS 1").await;

    // Consume (should return empty initially or succeed)
    let sql = "CONSUME FROM test_consume_tp GROUP 'consumers' START EARLIEST LIMIT 10";
    let result = server.execute_sql(sql).await;
    
    // Should succeed (empty result set is still success)
    assert!(
        result.status == ResponseStatus::Success,
        "CONSUME should succeed even if no messages: {:?}",
        result.error
    );
}

/// Test ACK functionality
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_ack_offset() {
    let server = TestServer::new_shared().await;

    // Setup topic
    server.execute_sql("CREATE TOPIC test_ack_tp PARTITIONS 1").await;

    // Consume first to create offset record
    server.execute_sql("CONSUME FROM test_ack_tp GROUP 'test_ack_group' START EARLIEST LIMIT 10").await;

    // ACK offset (should succeed)
    let sql = "ACK test_ack_tp GROUP 'test_ack_group' UPTO OFFSET 0";
    let result = server.execute_sql(sql).await;
    
    assert!(
        result.status == ResponseStatus::Success,
        "ACK should succeed: {:?}",
        result.error
    );
}

/// Test DROP TOPIC
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_drop_topic() {
    let server = TestServer::new_shared().await;

    // Create and drop
    server.execute_sql("CREATE TOPIC temp_drop_tp PARTITIONS 1").await;
    
    let sql = "DROP TOPIC temp_drop_tp";
    let result = server.execute_sql(sql).await;
    
    assert!(
        result.status == ResponseStatus::Success,
        "DROP TOPIC should succeed: {:?}",
        result.error
    );
}

// ============================================================================
// Authorization Tests
// ============================================================================

/// Test that user role is forbidden from consuming topics
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_consume_user_role_forbidden() {
    let server = TestServer::new_shared().await;

    // Create a regular user (not service/dba/system)
    server.execute_sql("CREATE USER test_user WITH PASSWORD 'testpass' ROLE user").await;
    
    // Create topic
    server.execute_sql("CREATE TOPIC forbidden_consume_tp PARTITIONS 1").await;

    // Try to consume as user role (should fail)
    let sql = "CONSUME FROM forbidden_consume_tp GROUP 'test_group' START EARLIEST LIMIT 10";
    let result = server.execute_sql_as_user(sql, "test_user").await;
    
    assert!(
        result.status == ResponseStatus::Error,
        "User role should be forbidden from consuming topics"
    );
    assert!(
        result.error.as_ref().map(|e| e.message.contains("service, dba, or system")).unwrap_or(false),
        "Error message should mention required roles: {:?}",
        result.error
    );
}

/// Test that service, dba, and system roles can consume topics
#[tokio::test]
#[ntest::timeout(15000)]
async fn test_consume_privileged_roles_allowed() {
    let server = TestServer::new_shared().await;

    // Create users with privileged roles
    server.execute_sql("CREATE USER test_service WITH PASSWORD 'pass' ROLE service").await;
    server.execute_sql("CREATE USER test_dba WITH PASSWORD 'pass' ROLE dba").await;
    
    // Create topic
    server.execute_sql("CREATE TOPIC privileged_consume_tp PARTITIONS 1").await;

    // Test service role
    let sql = "CONSUME FROM privileged_consume_tp GROUP 'service_group' START EARLIEST LIMIT 10";
    let result = server.execute_sql_as_user(sql, "test_service").await;
    assert!(
        result.status == ResponseStatus::Success,
        "Service role should be able to consume: {:?}",
        result.error
    );

    // Test dba role
    let sql = "CONSUME FROM privileged_consume_tp GROUP 'dba_group' START EARLIEST LIMIT 10";
    let result = server.execute_sql_as_user(sql, "test_dba").await;
    assert!(
        result.status == ResponseStatus::Success,
        "DBA role should be able to consume: {:?}",
        result.error
    );

    // Test system role (root user)
    let sql = "CONSUME FROM privileged_consume_tp GROUP 'system_group' START EARLIEST LIMIT 10";
    let result = server.execute_sql(sql).await;
    assert!(
        result.status == ResponseStatus::Success,
        "System role should be able to consume: {:?}",
        result.error
    );
}

/// Test user role is also forbidden from ACK
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_ack_user_role_forbidden() {
    let server = TestServer::new_shared().await;

    // Create a regular user
    server.execute_sql("CREATE USER test_user_ack WITH PASSWORD 'testpass' ROLE user").await;
    
    // Create topic
    server.execute_sql("CREATE TOPIC forbidden_ack_tp PARTITIONS 1").await;

    // Try to ACK as user role (should fail)
    let sql = "ACK forbidden_ack_tp GROUP 'test_group' UPTO OFFSET 0";
    let result = server.execute_sql_as_user(sql, "test_user_ack").await;
    
    assert!(
        result.status == ResponseStatus::Error,
        "User role should be forbidden from ACK: {:?}",
        result
    );
}

// ============================================================================
// End-to-End CDC + CONSUME Workflow Test
// ============================================================================

/// Test complete CDC workflow: INSERT → Topic → CONSUME
/// This verifies that table changes properly flow through topics and can be consumed.
#[tokio::test]
#[ntest::timeout(20000)]
async fn test_cdc_insert_to_consume_workflow() {
    let server = TestServer::new_shared().await;

    // 1. Setup namespace and table
    server.execute_sql("CREATE NAMESPACE test_cdc_ns").await;
    let create_table = "CREATE TABLE test_cdc_ns.events (
        id TEXT PRIMARY KEY,
        event_type TEXT,
        data TEXT
    )";
    server.execute_sql(create_table).await;

    // 2. Create topic and add CDC source
    server.execute_sql("CREATE TOPIC events_stream PARTITIONS 1").await;
    server.execute_sql("ALTER TOPIC events_stream ADD SOURCE test_cdc_ns.events ON INSERT").await;

    // 3. Insert data (should trigger CDC → topic)
    let insert = "INSERT INTO test_cdc_ns.events (id, event_type, data) VALUES 
        ('evt1', 'user_signup', 'John Doe'),
        ('evt2', 'user_login', 'Jane Smith')";
    let result = server.execute_sql(insert).await;
    assert!(
        result.status == ResponseStatus::Success,
        "INSERT failed: {:?}",
        result.error
    );

    // 4. Wait briefly for CDC processing (async)
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // 5. Consume from topic (should see the messages)
    let consume = "CONSUME FROM events_stream GROUP 'cdc_consumers' START EARLIEST LIMIT 10";
    let result = server.execute_sql(consume).await;
    
    assert!(
        result.status == ResponseStatus::Success,
        "CONSUME failed: {:?}",
        result.error
    );

    // 6. Verify we got results (schema: topic, partition, offset, key, payload, timestamp_ms)
    assert!(
        !result.results.is_empty(),
        "CONSUME should return batches"
    );
    
    if let Some(first_batch) = result.results.first() {
        // Check we have the expected 6 columns from topic_message_schema
        assert_eq!(
            first_batch.schema.len(), 6,
            "Should have 6 schema fields (topic, partition, offset, key, payload, timestamp_ms)"
        );
        
        // Verify column names match schema
        assert_eq!(first_batch.schema[0].name, "topic");
        assert_eq!(first_batch.schema[1].name, "partition");
        assert_eq!(first_batch.schema[2].name, "offset");
        assert_eq!(first_batch.schema[3].name, "key");
        assert_eq!(first_batch.schema[4].name, "payload");
        assert_eq!(first_batch.schema[5].name, "timestamp_ms");
        
        // Should have at least 2 rows (2 INSERTs)
        assert!(
            first_batch.row_count >= 2,
            "Should consume at least 2 messages, got {}",
            first_batch.row_count
        );
    }
}

/// Test CONSUME with schema validation
/// Verifies the response matches topic_message_schema structure
#[tokio::test]
#[ntest::timeout(10000)]
async fn test_consume_schema_structure() {
    let server = TestServer::new_shared().await;

    // Setup topic
    server.execute_sql("CREATE TOPIC schema_test_tp PARTITIONS 1").await;

    // Consume (empty is fine, we're testing schema)
    let sql = "CONSUME FROM schema_test_tp GROUP 'schema_group' START EARLIEST LIMIT 10";
    let result = server.execute_sql(sql).await;
    
    assert!(
        result.status == ResponseStatus::Success,
        "CONSUME should succeed: {:?}",
        result.error
    );

    // Verify schema structure matches topic_message_schema
    if !result.results.is_empty() {
        let batch = &result.results[0];
        
        // Must have exactly 6 schema fields
        assert_eq!(
            batch.schema.len(), 6,
            "Topic message schema must have 6 fields"
        );
        
        // Verify field names and order
        let expected_fields = vec!["topic", "partition", "offset", "key", "payload", "timestamp_ms"];
        for (i, expected_name) in expected_fields.iter().enumerate() {
            assert_eq!(
                &batch.schema[i].name, expected_name,
                "Field {} should be '{}', got '{}'",
                i, expected_name, batch.schema[i].name
            );
        }
    }
}

// ============================================================================
// TODO: Additional CDC Workflow Tests
// ============================================================================
//
// The following tests require:
// - HTTP API test client setup
// - Async notification verification
// - Extended timeout handling for CDC processing
//
// Completed tests:
// ✅ test_cdc_insert_to_consume_workflow() - CDC INSERT → CONSUME end-to-end
// ✅ test_consume_schema_structure() - Schema validation
// ✅ test_consume_user_role_forbidden() - Authorization checks
// ✅ test_consume_privileged_roles_allowed() - Service/DBA/System access
// ✅ test_ack_user_role_forbidden() - ACK authorization checks
//
// Planned tests (Phase 10 in IMPLEMENTATION_TASKS.md):
// - test_consume_multiple_consumer_groups()
// - test_consume_with_filter_expression()
// - test_long_polling_immediate_return()
// - test_long_polling_timeout_empty_response()
// - test_http_api_consume_endpoint() - REST API /v1/api/topics/consume
// - test_http_api_ack_endpoint() - REST API /v1/api/topics/ack
//
// These will be implemented once the HTTP API test client infrastructure
// is in place and CDC notification timing is stable.
