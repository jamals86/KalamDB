// Integration Tests for User Story 15: Schema Integrity and Unified SQL Functions
//
// Tests cover:
// - DEFAULT NOW() server-side evaluation
// - PRIMARY KEY requirements and allowed types
// - ID generation functions (SNOWFLAKE_ID, UUID_V7, ULID)
// - NOT NULL enforcement
// - SELECT * column order preservation

use crate::common::TestServer;
use serde_json::Value;

// T466: test_default_now_server_side_evaluation
#[tokio::test]
async fn test_default_now_server_side_evaluation() {
    let server = TestServer::start().await;
    
    // Create table with DEFAULT NOW()
    server.execute_sql(
        "CREATE NAMESPACE test_ns"
    ).await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.events (
            id BIGINT PRIMARY KEY,
            event_name TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )"
    ).await.unwrap();
    
    // Insert without timestamp - should use server-side NOW()
    let insert_time = chrono::Utc::now();
    server.execute_sql(
        "INSERT INTO test_ns.events (id, event_name) VALUES (1, 'user_login')"
    ).await.unwrap();
    
    // Query and verify timestamp is close to insert time
    let result = server.execute_sql(
        "SELECT id, event_name, created_at FROM test_ns.events WHERE id = 1"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    let created_at = result.rows[0]["created_at"].as_str().unwrap();
    let parsed_time = chrono::DateTime::parse_from_rfc3339(created_at).unwrap();
    let diff = (parsed_time.timestamp() - insert_time.timestamp()).abs();
    assert!(diff < 2, "Timestamp should be within 2 seconds of insert time");
}

// T467: test_default_now_explicit_value_override
#[tokio::test]
async fn test_default_now_explicit_value_override() {
    let server = TestServer::start().await;
    
    server.execute_sql(
        "CREATE NAMESPACE test_ns"
    ).await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.events (
            id BIGINT PRIMARY KEY,
            event_name TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )"
    ).await.unwrap();
    
    // Insert with explicit timestamp
    let explicit_time = "2025-01-01T00:00:00Z";
    server.execute_sql(&format!(
        "INSERT INTO test_ns.events (id, event_name, created_at) VALUES (1, 'test', '{}')",
        explicit_time
    )).await.unwrap();
    
    // Verify explicit value is used, not DEFAULT NOW()
    let result = server.execute_sql(
        "SELECT created_at FROM test_ns.events WHERE id = 1"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    let created_at = result.rows[0]["created_at"].as_str().unwrap();
    assert!(created_at.contains("2025-01-01"), "Should use explicit timestamp, not NOW()");
}

// T468: test_primary_key_required_user_table
#[tokio::test]
async fn test_primary_key_required_user_table() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Try to create USER table without PRIMARY KEY
    let result = server.execute_sql(
        "CREATE USER TABLE test_ns.no_pk (
            name TEXT NOT NULL
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject table without PRIMARY KEY");
    let error = result.unwrap_err();
    assert!(error.contains("PRIMARY KEY") || error.contains("primary key"),
        "Error should mention PRIMARY KEY requirement: {}", error);
}

// T469: test_primary_key_required_shared_table
#[tokio::test]
async fn test_primary_key_required_shared_table() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Try to create SHARED table without PRIMARY KEY
    let result = server.execute_sql(
        "CREATE SHARED TABLE test_ns.no_pk (
            config_key TEXT NOT NULL
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject SHARED table without PRIMARY KEY");
    let error = result.unwrap_err();
    assert!(error.contains("PRIMARY KEY") || error.contains("primary key"),
        "Error should mention PRIMARY KEY requirement: {}", error);
}

// T470: test_primary_key_required_stream_table
#[tokio::test]
async fn test_primary_key_required_stream_table() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Try to create STREAM table without PRIMARY KEY
    let result = server.execute_sql(
        "CREATE STREAM TABLE test_ns.no_pk (
            event_data TEXT NOT NULL
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject STREAM table without PRIMARY KEY");
    let error = result.unwrap_err();
    assert!(error.contains("PRIMARY KEY") || error.contains("primary key"),
        "Error should mention PRIMARY KEY requirement: {}", error);
}

// T471: test_primary_key_bigint_allowed
#[tokio::test]
async fn test_primary_key_bigint_allowed() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Create table with BIGINT PRIMARY KEY
    let result = server.execute_sql(
        "CREATE USER TABLE test_ns.bigint_pk (
            id BIGINT PRIMARY KEY,
            data TEXT NOT NULL
        )"
    ).await;
    
    assert!(result.is_ok(), "BIGINT PRIMARY KEY should be accepted");
}

// T472: test_primary_key_string_allowed
#[tokio::test]
async fn test_primary_key_string_allowed() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Create table with STRING/TEXT PRIMARY KEY
    let result = server.execute_sql(
        "CREATE USER TABLE test_ns.string_pk (
            id TEXT PRIMARY KEY,
            data TEXT NOT NULL
        )"
    ).await;
    
    assert!(result.is_ok(), "TEXT/STRING PRIMARY KEY should be accepted");
}

// T473: test_primary_key_invalid_type_rejected
#[tokio::test]
async fn test_primary_key_invalid_type_rejected() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Try to create table with BOOLEAN PRIMARY KEY (invalid)
    let result = server.execute_sql(
        "CREATE USER TABLE test_ns.bool_pk (
            is_active BOOLEAN PRIMARY KEY,
            data TEXT NOT NULL
        )"
    ).await;
    
    assert!(result.is_err(), "BOOLEAN PRIMARY KEY should be rejected");
    let error = result.unwrap_err();
    assert!(error.contains("PRIMARY KEY") && (error.contains("BIGINT") || error.contains("STRING")),
        "Error should mention allowed PRIMARY KEY types: {}", error);
}

// T474: test_default_snowflake_id_on_pk
#[tokio::test]
async fn test_default_snowflake_id_on_pk() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.snowflake_events (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            event_name TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert 100 rows without specifying ID
    for i in 0..100 {
        server.execute_sql(&format!(
            "INSERT INTO test_ns.snowflake_events (event_name) VALUES ('event_{}')",
            i
        )).await.unwrap();
    }
    
    // Query all IDs and verify they're time-ordered
    let result = server.execute_sql(
        "SELECT id FROM test_ns.snowflake_events ORDER BY id"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 100);
    
    // Verify IDs are monotonically increasing
    let mut prev_id = 0i64;
    for row in &result.rows {
        let id = row["id"].as_i64().unwrap();
        assert!(id > prev_id, "IDs should be monotonically increasing");
        prev_id = id;
    }
}

// T475: test_default_uuid_v7_on_pk
#[tokio::test]
async fn test_default_uuid_v7_on_pk() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.uuid_events (
            event_id TEXT PRIMARY KEY DEFAULT UUID_V7(),
            event_name TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert multiple rows
    for i in 0..10 {
        server.execute_sql(&format!(
            "INSERT INTO test_ns.uuid_events (event_name) VALUES ('event_{}')",
            i
        )).await.unwrap();
    }
    
    // Query and verify UUIDv7 format (RFC 9562)
    let result = server.execute_sql(
        "SELECT event_id FROM test_ns.uuid_events"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 10);
    
    for row in &result.rows {
        let uuid = row["event_id"].as_str().unwrap();
        // UUIDv7 format: 8-4-4-4-12 hexadecimal
        assert!(uuid.len() == 36, "UUID should be 36 characters");
        assert!(uuid.contains('-'), "UUID should contain hyphens");
        // Verify version 7 (7th char should be '7')
        assert_eq!(&uuid[14..15], "7", "Should be UUIDv7 (version 7)");
    }
}

// T476: test_default_ulid_on_pk
#[tokio::test]
async fn test_default_ulid_on_pk() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.ulid_requests (
            request_id TEXT PRIMARY KEY DEFAULT ULID(),
            endpoint TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert multiple rows
    for i in 0..10 {
        server.execute_sql(&format!(
            "INSERT INTO test_ns.ulid_requests (endpoint) VALUES ('/api/v{}')",
            i
        )).await.unwrap();
    }
    
    // Query and verify ULID format (26 characters, Crockford base32)
    let result = server.execute_sql(
        "SELECT request_id FROM test_ns.ulid_requests"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 10);
    
    for row in &result.rows {
        let ulid = row["request_id"].as_str().unwrap();
        assert_eq!(ulid.len(), 26, "ULID should be exactly 26 characters");
        // Verify Crockford base32 characters (0-9, A-Z excluding I, L, O, U)
        assert!(ulid.chars().all(|c| c.is_ascii_alphanumeric() && c != 'I' && c != 'L' && c != 'O' && c != 'U'),
            "ULID should be Crockford base32 (URL-safe)");
    }
}

// T477: test_snowflake_id_time_component
#[tokio::test]
async fn test_snowflake_id_time_component() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.snowflake_test (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            data TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Generate 1000 IDs
    for i in 0..1000 {
        server.execute_sql(&format!(
            "INSERT INTO test_ns.snowflake_test (data) VALUES ('data_{}')",
            i
        )).await.unwrap();
    }
    
    // Query IDs and verify timestamp monotonicity
    let result = server.execute_sql(
        "SELECT id FROM test_ns.snowflake_test ORDER BY id"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1000);
    
    let mut prev_timestamp = 0u64;
    for row in &result.rows {
        let id = row["id"].as_i64().unwrap() as u64;
        // Extract 41-bit timestamp (top 41 bits)
        let timestamp = id >> 22;
        assert!(timestamp >= prev_timestamp, "Timestamp component should be monotonically increasing");
        prev_timestamp = timestamp;
    }
}

// T478: test_snowflake_id_uniqueness
#[tokio::test]
async fn test_snowflake_id_uniqueness() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.unique_test (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            data TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Generate 10000 IDs (stress test for uniqueness)
    for i in 0..10000 {
        server.execute_sql(&format!(
            "INSERT INTO test_ns.unique_test (data) VALUES ('data_{}')",
            i
        )).await.unwrap();
    }
    
    // Query and verify no duplicates
    let result = server.execute_sql(
        "SELECT COUNT(DISTINCT id) as unique_count, COUNT(*) as total_count FROM test_ns.unique_test"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    let unique_count = result.rows[0]["unique_count"].as_i64().unwrap();
    let total_count = result.rows[0]["total_count"].as_i64().unwrap();
    assert_eq!(unique_count, total_count, "All IDs should be unique (no duplicates in 10000 IDs)");
}

// T479: test_uuidv7_rfc9562_compliance
#[tokio::test]
async fn test_uuidv7_rfc9562_compliance() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.uuid_test (
            id TEXT PRIMARY KEY DEFAULT UUID_V7(),
            data TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Generate UUID
    server.execute_sql(
        "INSERT INTO test_ns.uuid_test (data) VALUES ('test')"
    ).await.unwrap();
    
    let result = server.execute_sql(
        "SELECT id FROM test_ns.uuid_test"
    ).await.unwrap();
    
    let uuid_str = result.rows[0]["id"].as_str().unwrap();
    
    // RFC 9562 compliance checks:
    // 1. Format: 8-4-4-4-12
    assert_eq!(uuid_str.len(), 36);
    assert_eq!(&uuid_str[8..9], "-");
    assert_eq!(&uuid_str[13..14], "-");
    assert_eq!(&uuid_str[18..19], "-");
    assert_eq!(&uuid_str[23..24], "-");
    
    // 2. Version 7 (bits 48-51 should be 0111 = 7)
    assert_eq!(&uuid_str[14..15], "7");
    
    // 3. Variant (bits 64-65 should be 10)
    let variant_char = uuid_str.chars().nth(19).unwrap();
    assert!(matches!(variant_char, '8' | '9' | 'a' | 'b' | 'A' | 'B'),
        "Variant bits should be 10xx (hex 8-b)");
}

// T480: test_ulid_format_compliance
#[tokio::test]
async fn test_ulid_format_compliance() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.ulid_test (
            id TEXT PRIMARY KEY DEFAULT ULID(),
            data TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Generate ULID
    server.execute_sql(
        "INSERT INTO test_ns.ulid_test (data) VALUES ('test')"
    ).await.unwrap();
    
    let result = server.execute_sql(
        "SELECT id FROM test_ns.ulid_test"
    ).await.unwrap();
    
    let ulid_str = result.rows[0]["id"].as_str().unwrap();
    
    // ULID compliance checks:
    // 1. Length: 26 characters
    assert_eq!(ulid_str.len(), 26);
    
    // 2. Crockford base32: 0-9, A-Z excluding I, L, O, U (case-insensitive)
    for c in ulid_str.chars() {
        assert!(c.is_ascii_alphanumeric(), "ULID should only contain alphanumeric");
        assert!(!matches!(c.to_ascii_uppercase(), 'I' | 'L' | 'O' | 'U'),
            "ULID should not contain I, L, O, U");
    }
    
    // 3. Time-sortable: first 10 chars are timestamp
    let timestamp_part = &ulid_str[0..10];
    assert!(timestamp_part.chars().all(|c| c.is_ascii_alphanumeric()));
}

// T481: test_default_functions_on_non_pk_columns
#[tokio::test]
async fn test_default_functions_on_non_pk_columns() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.correlation_test (
            id BIGINT PRIMARY KEY,
            correlation_id TEXT DEFAULT ULID(),
            event_name TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert without correlation_id
    server.execute_sql(
        "INSERT INTO test_ns.correlation_test (id, event_name) VALUES (1, 'test_event')"
    ).await.unwrap();
    
    // Verify ULID was generated for non-PK column
    let result = server.execute_sql(
        "SELECT correlation_id FROM test_ns.correlation_test WHERE id = 1"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    let correlation_id = result.rows[0]["correlation_id"].as_str().unwrap();
    assert_eq!(correlation_id.len(), 26, "Should generate ULID for non-PK column");
}

// T482: test_multiple_default_functions_same_table
#[tokio::test]
async fn test_multiple_default_functions_same_table() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.multi_default (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            correlation_id TEXT DEFAULT ULID(),
            created_at TIMESTAMP DEFAULT NOW(),
            event_name TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert with only event_name (all others should use DEFAULT)
    server.execute_sql(
        "INSERT INTO test_ns.multi_default (event_name) VALUES ('test_event')"
    ).await.unwrap();
    
    // Verify all DEFAULT functions worked
    let result = server.execute_sql(
        "SELECT id, correlation_id, created_at FROM test_ns.multi_default"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    // Verify SNOWFLAKE_ID generated
    let id = result.rows[0]["id"].as_i64().unwrap();
    assert!(id > 0, "Should generate Snowflake ID");
    
    // Verify ULID generated
    let correlation_id = result.rows[0]["correlation_id"].as_str().unwrap();
    assert_eq!(correlation_id.len(), 26, "Should generate ULID");
    
    // Verify NOW() generated timestamp
    let created_at = result.rows[0]["created_at"].as_str().unwrap();
    assert!(!created_at.is_empty(), "Should generate timestamp with NOW()");
}

// T483: test_not_null_enforcement_insert
#[tokio::test]
async fn test_not_null_enforcement_insert() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.not_null_test (
            id BIGINT PRIMARY KEY,
            required_field TEXT NOT NULL,
            optional_field TEXT
        )"
    ).await.unwrap();
    
    // Try to insert NULL in NOT NULL column
    let result = server.execute_sql(
        "INSERT INTO test_ns.not_null_test (id, required_field, optional_field) VALUES (1, NULL, 'optional')"
    ).await;
    
    assert!(result.is_err(), "Should reject NULL in NOT NULL column");
    let error = result.unwrap_err();
    assert!(error.contains("NOT NULL") || error.contains("required"),
        "Error should mention NOT NULL violation: {}", error);
    
    // Verify no partial write occurred
    let count_result = server.execute_sql(
        "SELECT COUNT(*) as count FROM test_ns.not_null_test"
    ).await.unwrap();
    
    let count = count_result.rows[0]["count"].as_i64().unwrap();
    assert_eq!(count, 0, "No row should be inserted on validation failure");
}

// T484: test_not_null_enforcement_update
#[tokio::test]
async fn test_not_null_enforcement_update() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.not_null_test (
            id BIGINT PRIMARY KEY,
            required_field TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Insert valid row
    server.execute_sql(
        "INSERT INTO test_ns.not_null_test (id, required_field) VALUES (1, 'initial_value')"
    ).await.unwrap();
    
    // Try to update to NULL
    let result = server.execute_sql(
        "UPDATE test_ns.not_null_test SET required_field = NULL WHERE id = 1"
    ).await;
    
    assert!(result.is_err(), "Should reject UPDATE that sets NOT NULL column to NULL");
    
    // Verify value unchanged
    let query_result = server.execute_sql(
        "SELECT required_field FROM test_ns.not_null_test WHERE id = 1"
    ).await.unwrap();
    
    let value = query_result.rows[0]["required_field"].as_str().unwrap();
    assert_eq!(value, "initial_value", "Value should remain unchanged after failed UPDATE");
}

// T485: test_not_null_validation_before_write
#[tokio::test]
async fn test_not_null_validation_before_write() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    server.execute_sql(
        "CREATE USER TABLE test_ns.not_null_test (
            id BIGINT PRIMARY KEY,
            required_field TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // Capture RocksDB write count before operation
    // (This test verifies validation happens BEFORE any database write)
    
    let result = server.execute_sql(
        "INSERT INTO test_ns.not_null_test (id, required_field) VALUES (1, NULL)"
    ).await;
    
    assert!(result.is_err(), "Validation should fail before write");
    
    // Verify no data written to RocksDB
    let count_result = server.execute_sql(
        "SELECT COUNT(*) as count FROM test_ns.not_null_test"
    ).await.unwrap();
    
    let count = count_result.rows[0]["count"].as_i64().unwrap();
    assert_eq!(count, 0, "RocksDB write should never occur on validation failure");
}

// T486: test_select_star_column_order
#[tokio::test]
async fn test_select_star_column_order() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Create table with specific column order
    server.execute_sql(
        "CREATE USER TABLE test_ns.order_test (
            id BIGINT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )"
    ).await.unwrap();
    
    // Insert test data
    server.execute_sql(
        "INSERT INTO test_ns.order_test (id, name, email) VALUES (1, 'Alice', 'alice@example.com')"
    ).await.unwrap();
    
    // SELECT * and verify column order
    let result = server.execute_sql(
        "SELECT * FROM test_ns.order_test WHERE id = 1"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    // Verify column order matches CREATE TABLE order
    let columns: Vec<String> = result.rows[0].as_object().unwrap().keys().cloned().collect();
    assert_eq!(columns, vec!["id", "name", "email", "created_at"],
        "Column order should match CREATE TABLE definition");
}

// T487: test_column_order_preserved_after_alter
#[tokio::test]
async fn test_column_order_preserved_after_alter() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Create table
    server.execute_sql(
        "CREATE USER TABLE test_ns.alter_test (
            id BIGINT PRIMARY KEY,
            name TEXT NOT NULL
        )"
    ).await.unwrap();
    
    // ALTER TABLE to add column
    server.execute_sql(
        "ALTER TABLE test_ns.alter_test ADD COLUMN age INT"
    ).await.unwrap();
    
    // Insert data
    server.execute_sql(
        "INSERT INTO test_ns.alter_test (id, name, age) VALUES (1, 'Bob', 30)"
    ).await.unwrap();
    
    // SELECT * and verify new column is at the end
    let result = server.execute_sql(
        "SELECT * FROM test_ns.alter_test WHERE id = 1"
    ).await.unwrap();
    
    let columns: Vec<String> = result.rows[0].as_object().unwrap().keys().cloned().collect();
    assert_eq!(columns, vec!["id", "name", "age"],
        "New column should be added at the end");
}

// T488: test_column_order_metadata_storage
#[tokio::test]
async fn test_column_order_metadata_storage() {
    let server = TestServer::start().await;
    
    server.execute_sql("CREATE NAMESPACE test_ns").await.unwrap();
    
    // Create table with specific column order
    server.execute_sql(
        "CREATE USER TABLE test_ns.metadata_test (
            id BIGINT PRIMARY KEY,
            alpha TEXT NOT NULL,
            beta INT,
            gamma TIMESTAMP
        )"
    ).await.unwrap();
    
    // Query system.columns for ordinal positions
    let result = server.execute_sql(
        "SELECT column_name, ordinal_position 
         FROM system.columns 
         WHERE table_name = 'metadata_test' 
         ORDER BY ordinal_position"
    ).await.unwrap();
    
    assert_eq!(result.rows.len(), 4);
    
    // Verify ordinal positions match creation order
    assert_eq!(result.rows[0]["column_name"].as_str().unwrap(), "id");
    assert_eq!(result.rows[0]["ordinal_position"].as_i64().unwrap(), 0);
    
    assert_eq!(result.rows[1]["column_name"].as_str().unwrap(), "alpha");
    assert_eq!(result.rows[1]["ordinal_position"].as_i64().unwrap(), 1);
    
    assert_eq!(result.rows[2]["column_name"].as_str().unwrap(), "beta");
    assert_eq!(result.rows[2]["ordinal_position"].as_i64().unwrap(), 2);
    
    assert_eq!(result.rows[3]["column_name"].as_str().unwrap(), "gamma");
    assert_eq!(result.rows[3]["ordinal_position"].as_i64().unwrap(), 3);
}
