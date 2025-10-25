// Integration tests for User Story 15: SQL Functions & Schema Integrity
// Tests DEFAULT NOW(), SNOWFLAKE_ID(), UUID_V7(), ULID() functions
// Tests PRIMARY KEY enforcement, NOT NULL validation, column order preservation

use crate::common::{TestServer, TestTable};
use serde_json::Value;

/// T466: test_default_now_server_side_evaluation
/// CREATE TABLE with DEFAULT NOW(), INSERT without timestamp, verify server-side value
#[tokio::test]
async fn test_default_now_server_side_evaluation() {
    let server = TestServer::new_in_memory().await;
    
    // Create table with DEFAULT NOW()
    server.execute_sql(
        "CREATE USER TABLE events (
            id BIGINT PRIMARY KEY,
            event_type TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )"
    ).await.expect("Failed to create table");
    
    // Insert without providing created_at
    let before_insert = std::time::SystemTime::now();
    server.execute_sql(
        "INSERT INTO events (id, event_type) VALUES (1, 'user_login')"
    ).await.expect("Failed to insert");
    let after_insert = std::time::SystemTime::now();
    
    // Query and verify server-side timestamp was applied
    let result = server.query_sql("SELECT id, event_type, created_at FROM events WHERE id = 1")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 1);
    let row = &result[0];
    assert_eq!(row["id"], 1);
    assert_eq!(row["event_type"], "user_login");
    
    // Verify created_at is within the time window
    let created_at_str = row["created_at"].as_str().expect("created_at should be string");
    let created_at = chrono::DateTime::parse_from_rfc3339(created_at_str)
        .expect("Invalid timestamp format");
    
    let before = chrono::DateTime::from(before_insert);
    let after = chrono::DateTime::from(after_insert);
    
    assert!(created_at >= before && created_at <= after, 
        "Server-side timestamp {} should be between {} and {}", 
        created_at, before, after);
}

/// T467: test_default_now_explicit_value_override
/// INSERT with explicit timestamp, verify DEFAULT NOW() not applied
#[tokio::test]
async fn test_default_now_explicit_value_override() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE events (
            id BIGINT PRIMARY KEY,
            event_type TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )"
    ).await.expect("Failed to create table");
    
    // Insert with explicit timestamp
    let explicit_time = "2024-01-15T10:30:00Z";
    server.execute_sql(&format!(
        "INSERT INTO events (id, event_type, created_at) VALUES (1, 'test', '{}')",
        explicit_time
    )).await.expect("Failed to insert");
    
    // Verify explicit value was used, not DEFAULT NOW()
    let result = server.query_sql("SELECT created_at FROM events WHERE id = 1")
        .await.expect("Failed to query");
    
    assert_eq!(result[0]["created_at"].as_str().unwrap(), explicit_time);
}

/// T468: test_primary_key_required_user_table
/// CREATE USER TABLE without PRIMARY KEY, verify error
#[tokio::test]
async fn test_primary_key_required_user_table() {
    let server = TestServer::new_in_memory().await;
    
    let result = server.execute_sql(
        "CREATE USER TABLE no_pk (
            name TEXT NOT NULL,
            email TEXT
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject table without PRIMARY KEY");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("PRIMARY KEY") || error.contains("primary key"),
        "Error should mention PRIMARY KEY requirement: {}", error);
}

/// T469: test_primary_key_required_shared_table
/// CREATE SHARED TABLE without PRIMARY KEY, verify error
#[tokio::test]
async fn test_primary_key_required_shared_table() {
    let server = TestServer::new_in_memory().await;
    
    let result = server.execute_sql(
        "CREATE SHARED TABLE config (
            key TEXT NOT NULL,
            value TEXT
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject shared table without PRIMARY KEY");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("PRIMARY KEY") || error.contains("primary key"),
        "Error should mention PRIMARY KEY requirement: {}", error);
}

/// T470: test_primary_key_required_stream_table
/// CREATE STREAM TABLE without PRIMARY KEY, verify error
#[tokio::test]
async fn test_primary_key_required_stream_table() {
    let server = TestServer::new_in_memory().await;
    
    let result = server.execute_sql(
        "CREATE STREAM TABLE logs (
            message TEXT NOT NULL,
            level TEXT
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject stream table without PRIMARY KEY");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("PRIMARY KEY") || error.contains("primary key"),
        "Error should mention PRIMARY KEY requirement: {}", error);
}

/// T471: test_primary_key_bigint_allowed
/// CREATE TABLE with PRIMARY KEY BIGINT, verify accepted
#[tokio::test]
async fn test_primary_key_bigint_allowed() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE users (
            id BIGINT PRIMARY KEY,
            username TEXT NOT NULL
        )"
    ).await.expect("Should accept BIGINT PRIMARY KEY");
    
    // Verify table was created
    let result = server.query_sql("SELECT * FROM information_schema.tables WHERE table_name = 'users'")
        .await.expect("Failed to query");
    assert_eq!(result.len(), 1);
}

/// T472: test_primary_key_string_allowed
/// CREATE TABLE with PRIMARY KEY TEXT, verify accepted
#[tokio::test]
async fn test_primary_key_string_allowed() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE sessions (
            session_id TEXT PRIMARY KEY,
            user_id BIGINT NOT NULL
        )"
    ).await.expect("Should accept TEXT PRIMARY KEY");
    
    // Verify table was created
    let result = server.query_sql("SELECT * FROM information_schema.tables WHERE table_name = 'sessions'")
        .await.expect("Failed to query");
    assert_eq!(result.len(), 1);
}

/// T473: test_primary_key_invalid_type_rejected
/// CREATE TABLE with PRIMARY KEY BOOLEAN, verify error
#[tokio::test]
async fn test_primary_key_invalid_type_rejected() {
    let server = TestServer::new_in_memory().await;
    
    let result = server.execute_sql(
        "CREATE USER TABLE invalid (
            is_active BOOLEAN PRIMARY KEY,
            name TEXT
        )"
    ).await;
    
    assert!(result.is_err(), "Should reject BOOLEAN PRIMARY KEY");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("PRIMARY KEY") || error.contains("BIGINT") || error.contains("TEXT"),
        "Error should mention valid PRIMARY KEY types: {}", error);
}

/// T474: test_default_snowflake_id_on_pk
/// CREATE TABLE with id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), INSERT 100 rows, verify time-ordered IDs
#[tokio::test]
async fn test_default_snowflake_id_on_pk() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE events (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            event_type TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Insert 100 rows without providing id
    for i in 1..=100 {
        server.execute_sql(&format!(
            "INSERT INTO events (event_type) VALUES ('event_{}')", i
        )).await.expect("Failed to insert");
        
        // Small delay to ensure time progression
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }
    
    // Query all rows ordered by id
    let result = server.query_sql("SELECT id FROM events ORDER BY id")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 100, "Should have 100 rows");
    
    // Verify IDs are monotonically increasing (time-ordered)
    let ids: Vec<i64> = result.iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect();
    
    for i in 1..ids.len() {
        assert!(ids[i] > ids[i-1], 
            "SNOWFLAKE_ID should be monotonically increasing: {} should be > {}",
            ids[i], ids[i-1]);
    }
    
    // Verify all IDs are unique
    let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique_ids.len(), 100, "All 100 IDs should be unique");
}

/// T475: test_default_uuid_v7_on_pk
/// CREATE TABLE with event_id STRING PRIMARY KEY DEFAULT UUID_V7(), INSERT rows, verify UUIDv7 format (RFC 9562)
#[tokio::test]
async fn test_default_uuid_v7_on_pk() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE events (
            event_id TEXT PRIMARY KEY DEFAULT UUID_V7(),
            event_type TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Insert multiple rows
    for i in 1..=10 {
        server.execute_sql(&format!(
            "INSERT INTO events (event_type) VALUES ('event_{}')", i
        )).await.expect("Failed to insert");
    }
    
    // Query and verify UUID v7 format
    let result = server.query_sql("SELECT event_id FROM events")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 10);
    
    for row in &result {
        let uuid_str = row["event_id"].as_str().expect("event_id should be string");
        
        // Verify UUID format: 8-4-4-4-12 hex digits
        assert_eq!(uuid_str.len(), 36, "UUID should be 36 characters");
        assert_eq!(&uuid_str[8..9], "-");
        assert_eq!(&uuid_str[13..14], "-");
        assert_eq!(&uuid_str[18..19], "-");
        assert_eq!(&uuid_str[23..24], "-");
        
        // Verify version 7 (4 bits at position 12-13 should be '7')
        let version_char = uuid_str.chars().nth(14).unwrap();
        assert_eq!(version_char, '7', "Should be UUID version 7");
        
        // Parse to verify valid UUID
        uuid::Uuid::parse_str(uuid_str).expect("Should be valid UUID");
    }
    
    // Verify all UUIDs are unique
    let unique_uuids: std::collections::HashSet<_> = result.iter()
        .map(|r| r["event_id"].as_str().unwrap())
        .collect();
    assert_eq!(unique_uuids.len(), 10, "All UUIDs should be unique");
}

/// T476: test_default_ulid_on_pk
/// CREATE TABLE with request_id STRING PRIMARY KEY DEFAULT ULID(), INSERT rows, verify 26-char URL-safe format
#[tokio::test]
async fn test_default_ulid_on_pk() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE requests (
            request_id TEXT PRIMARY KEY DEFAULT ULID(),
            method TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Insert multiple rows
    for method in &["GET", "POST", "PUT", "DELETE", "PATCH"] {
        server.execute_sql(&format!(
            "INSERT INTO requests (method) VALUES ('{}')", method
        )).await.expect("Failed to insert");
    }
    
    // Query and verify ULID format
    let result = server.query_sql("SELECT request_id FROM requests")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 5);
    
    for row in &result {
        let ulid_str = row["request_id"].as_str().expect("request_id should be string");
        
        // Verify ULID format: 26 characters, Crockford base32
        assert_eq!(ulid_str.len(), 26, "ULID should be 26 characters");
        
        // Verify only valid Crockford base32 characters (0-9, A-Z excluding I, L, O, U)
        for c in ulid_str.chars() {
            assert!(
                ('0'..='9').contains(&c) || 
                ('A'..='H').contains(&c) || 
                ('J'..='K').contains(&c) || 
                ('M'..='N').contains(&c) || 
                ('P'..='T').contains(&c) || 
                ('V'..='Z').contains(&c),
                "Invalid ULID character: {}", c
            );
        }
        
        // Parse to verify valid ULID
        ulid::Ulid::from_string(ulid_str).expect("Should be valid ULID");
    }
    
    // Verify all ULIDs are unique
    let unique_ulids: std::collections::HashSet<_> = result.iter()
        .map(|r| r["request_id"].as_str().unwrap())
        .collect();
    assert_eq!(unique_ulids.len(), 5, "All ULIDs should be unique");
}

/// T477: test_snowflake_id_time_component
/// Generate 1000 IDs, verify 41-bit timestamp monotonic increase
#[tokio::test]
async fn test_snowflake_id_time_component() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE test (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID()
        )"
    ).await.expect("Failed to create table");
    
    // Insert 1000 rows rapidly
    for _ in 0..1000 {
        server.execute_sql("INSERT INTO test VALUES ()").await.expect("Failed to insert");
    }
    
    let result = server.query_sql("SELECT id FROM test ORDER BY id")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 1000);
    
    let ids: Vec<i64> = result.iter().map(|r| r["id"].as_i64().unwrap()).collect();
    
    // Extract 41-bit timestamp (top 41 bits) from each ID
    let timestamps: Vec<i64> = ids.iter().map(|id| id >> 23).collect();
    
    // Verify timestamps are monotonically non-decreasing
    for i in 1..timestamps.len() {
        assert!(timestamps[i] >= timestamps[i-1],
            "Timestamp should be monotonically increasing");
    }
}

/// T478: test_snowflake_id_uniqueness
/// Generate 10000 IDs concurrently, verify no duplicates
#[tokio::test]
async fn test_snowflake_id_uniqueness() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE concurrent_test (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID()
        )"
    ).await.expect("Failed to create table");
    
    // Insert 10000 rows (concurrent execution via batch)
    let mut tasks = vec![];
    for _ in 0..100 {
        let server_clone = server.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..100 {
                server_clone.execute_sql("INSERT INTO concurrent_test VALUES ()").await.ok();
            }
        }));
    }
    
    for task in tasks {
        task.await.ok();
    }
    
    // Verify all IDs are unique
    let result = server.query_sql("SELECT COUNT(DISTINCT id) as unique_count, COUNT(*) as total_count FROM concurrent_test")
        .await.expect("Failed to query");
    
    let unique_count = result[0]["unique_count"].as_i64().unwrap();
    let total_count = result[0]["total_count"].as_i64().unwrap();
    
    assert_eq!(unique_count, total_count, 
        "All IDs should be unique: {} unique out of {} total", unique_count, total_count);
}

/// T479: test_uuidv7_rfc9562_compliance
/// Generate UUID_V7(), verify 48-bit timestamp + 80-bit random format
#[tokio::test]
async fn test_uuidv7_rfc9562_compliance() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE test (
            id TEXT PRIMARY KEY DEFAULT UUID_V7()
        )"
    ).await.expect("Failed to create table");
    
    server.execute_sql("INSERT INTO test VALUES ()").await.expect("Failed to insert");
    
    let result = server.query_sql("SELECT id FROM test")
        .await.expect("Failed to query");
    
    let uuid_str = result[0]["id"].as_str().unwrap();
    let uuid = uuid::Uuid::parse_str(uuid_str).expect("Should be valid UUID");
    
    // Verify version 7
    assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand), "Should be UUID v7");
    
    // Verify variant (RFC 4122)
    assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122, "Should be RFC 4122 variant");
}

/// T480: test_ulid_format_compliance
/// Generate ULID(), verify 26-char Crockford base32, time-sortable
#[tokio::test]
async fn test_ulid_format_compliance() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE test (
            id TEXT PRIMARY KEY DEFAULT ULID()
        )"
    ).await.expect("Failed to create table");
    
    // Insert multiple with delays to ensure time ordering
    for _ in 0..5 {
        server.execute_sql("INSERT INTO test VALUES ()").await.expect("Failed to insert");
        tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
    }
    
    let result = server.query_sql("SELECT id FROM test")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 5);
    
    let ulids: Vec<ulid::Ulid> = result.iter()
        .map(|r| ulid::Ulid::from_string(r["id"].as_str().unwrap()).unwrap())
        .collect();
    
    // Verify ULIDs are time-sortable (lexicographically ordered by creation time)
    let ulid_strings: Vec<&str> = result.iter().map(|r| r["id"].as_str().unwrap()).collect();
    let mut sorted_ulids = ulid_strings.clone();
    sorted_ulids.sort();
    
    assert_eq!(ulid_strings, sorted_ulids, "ULIDs should be naturally time-sorted");
}

/// T481: test_default_functions_on_non_pk_columns
/// CREATE TABLE with correlation_id STRING DEFAULT ULID() (non-PK), INSERT rows, verify generation
#[tokio::test]
async fn test_default_functions_on_non_pk_columns() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE events (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            event_type TEXT NOT NULL,
            correlation_id TEXT DEFAULT ULID()
        )"
    ).await.expect("Failed to create table");
    
    // Insert without providing correlation_id
    server.execute_sql("INSERT INTO events (event_type) VALUES ('user_action')")
        .await.expect("Failed to insert");
    
    let result = server.query_sql("SELECT correlation_id FROM events")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 1);
    let correlation_id = result[0]["correlation_id"].as_str().expect("Should have correlation_id");
    
    // Verify it's a valid ULID
    assert_eq!(correlation_id.len(), 26);
    ulid::Ulid::from_string(correlation_id).expect("Should be valid ULID");
}

/// T482: test_multiple_default_functions_same_table
/// CREATE TABLE with DEFAULT NOW(), DEFAULT SNOWFLAKE_ID(), DEFAULT ULID(), verify all work
#[tokio::test]
async fn test_multiple_default_functions_same_table() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE audit_log (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            correlation_id TEXT DEFAULT ULID(),
            created_at TIMESTAMP DEFAULT NOW(),
            action TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Insert without providing any default columns
    server.execute_sql("INSERT INTO audit_log (action) VALUES ('user_login')")
        .await.expect("Failed to insert");
    
    let result = server.query_sql("SELECT id, correlation_id, created_at FROM audit_log")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 1);
    let row = &result[0];
    
    // Verify all DEFAULT functions were evaluated
    assert!(row["id"].as_i64().is_some(), "SNOWFLAKE_ID should generate BIGINT");
    
    let corr_id = row["correlation_id"].as_str().expect("ULID should generate TEXT");
    assert_eq!(corr_id.len(), 26, "Should be valid ULID");
    
    let created_at = row["created_at"].as_str().expect("NOW should generate TIMESTAMP");
    chrono::DateTime::parse_from_rfc3339(created_at).expect("Should be valid timestamp");
}

/// T483: test_not_null_enforcement_insert
/// INSERT with NULL in NOT NULL column, verify error and no partial write
#[tokio::test]
async fn test_not_null_enforcement_insert() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE users (
            id BIGINT PRIMARY KEY,
            username TEXT NOT NULL,
            email TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Attempt to insert with NULL in NOT NULL column
    let result = server.execute_sql(
        "INSERT INTO users (id, username, email) VALUES (1, 'john', NULL)"
    ).await;
    
    assert!(result.is_err(), "Should reject NULL in NOT NULL column");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("NOT NULL") || error.contains("email"),
        "Error should mention NOT NULL violation: {}", error);
    
    // Verify no partial write occurred
    let count_result = server.query_sql("SELECT COUNT(*) as count FROM users")
        .await.expect("Failed to query");
    assert_eq!(count_result[0]["count"].as_i64().unwrap(), 0, 
        "No rows should be inserted on NOT NULL violation");
}

/// T484: test_not_null_enforcement_update
/// UPDATE to set NOT NULL column to NULL, verify error and no change
#[tokio::test]
async fn test_not_null_enforcement_update() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE users (
            id BIGINT PRIMARY KEY,
            username TEXT NOT NULL,
            email TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Insert valid row
    server.execute_sql(
        "INSERT INTO users (id, username, email) VALUES (1, 'john', 'john@example.com')"
    ).await.expect("Failed to insert");
    
    // Attempt to UPDATE to NULL
    let result = server.execute_sql(
        "UPDATE users SET email = NULL WHERE id = 1"
    ).await;
    
    assert!(result.is_err(), "Should reject UPDATE to NULL");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("NOT NULL") || error.contains("email"),
        "Error should mention NOT NULL violation: {}", error);
    
    // Verify original value unchanged
    let query_result = server.query_sql("SELECT email FROM users WHERE id = 1")
        .await.expect("Failed to query");
    assert_eq!(query_result[0]["email"].as_str().unwrap(), "john@example.com",
        "Email should remain unchanged");
}

/// T485: test_not_null_validation_before_write
/// Trigger NOT NULL violation, verify RocksDB write never occurs
#[tokio::test]
async fn test_not_null_validation_before_write() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE test (
            id BIGINT PRIMARY KEY,
            required_field TEXT NOT NULL
        )"
    ).await.expect("Failed to create table");
    
    // Attempt invalid insert
    let result = server.execute_sql(
        "INSERT INTO test (id, required_field) VALUES (1, NULL)"
    ).await;
    
    assert!(result.is_err(), "Should fail validation");
    
    // Verify table is completely empty (no partial state)
    let count_result = server.query_sql("SELECT COUNT(*) as count FROM test")
        .await.expect("Failed to query");
    assert_eq!(count_result[0]["count"].as_i64().unwrap(), 0,
        "Validation should prevent any RocksDB write");
}

/// T486: test_select_star_column_order
/// CREATE TABLE with columns (id, name, email, created_at), SELECT *, verify exact order
#[tokio::test]
async fn test_select_star_column_order() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE users (
            id BIGINT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW()
        )"
    ).await.expect("Failed to create table");
    
    server.execute_sql(
        "INSERT INTO users (id, name, email) VALUES (1, 'John Doe', 'john@example.com')"
    ).await.expect("Failed to insert");
    
    let result = server.query_sql("SELECT * FROM users")
        .await.expect("Failed to query");
    
    assert_eq!(result.len(), 1);
    let row = &result[0];
    
    // Verify column order matches creation order
    let keys: Vec<&String> = row.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["id", "name", "email", "created_at"],
        "SELECT * should return columns in creation order");
}

/// T487: test_column_order_preserved_after_alter
/// ALTER TABLE ADD COLUMN, SELECT *, verify new column at end
#[tokio::test]
#[ignore = "ALTER TABLE not yet implemented - deferred to future user story"]
async fn test_column_order_preserved_after_alter() {
    // This test is deferred until ALTER TABLE feature is implemented
    // Will verify that new columns appear at the end in SELECT * queries
}

/// T488: test_column_order_metadata_storage
/// Query information_schema.columns, verify ordinal_position matches creation order
#[tokio::test]
async fn test_column_order_metadata_storage() {
    let server = TestServer::new_in_memory().await;
    
    server.execute_sql(
        "CREATE USER TABLE test (
            first_col BIGINT PRIMARY KEY,
            second_col TEXT NOT NULL,
            third_col TIMESTAMP,
            fourth_col BOOLEAN
        )"
    ).await.expect("Failed to create table");
    
    let result = server.query_sql(
        "SELECT column_name, ordinal_position 
         FROM information_schema.columns 
         WHERE table_name = 'test' 
         ORDER BY ordinal_position"
    ).await.expect("Failed to query");
    
    assert_eq!(result.len(), 4);
    
    // Verify ordinal_position matches creation order
    assert_eq!(result[0]["column_name"].as_str().unwrap(), "first_col");
    assert_eq!(result[0]["ordinal_position"].as_i64().unwrap(), 1);
    
    assert_eq!(result[1]["column_name"].as_str().unwrap(), "second_col");
    assert_eq!(result[1]["ordinal_position"].as_i64().unwrap(), 2);
    
    assert_eq!(result[2]["column_name"].as_str().unwrap(), "third_col");
    assert_eq!(result[2]["ordinal_position"].as_i64().unwrap(), 3);
    
    assert_eq!(result[3]["column_name"].as_str().unwrap(), "fourth_col");
    assert_eq!(result[3]["ordinal_position"].as_i64().unwrap(), 4);
}
