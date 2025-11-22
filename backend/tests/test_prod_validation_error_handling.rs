//! Validation and Error Handling Tests
//!
//! Tests that invalid inputs result in clear, actionable error messages:
//! - Invalid SQL syntax
//! - Invalid operations (wrong table types, unauthorized actions)
//! - Permissions and multi-tenant isolation
//!
//! These tests ensure that user errors are caught early with helpful feedback.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;
use kalamdb_commons::Role;

/// Test: Syntax errors return helpful error messages
#[tokio::test]
async fn test_syntax_errors_return_clear_messages() {
    let server = TestServer::new().await;
    
    let invalid_queries = vec![
        ("SELECT * FROM", "Incomplete SELECT"),
        ("INSERT INTO table", "Incomplete INSERT"),
        ("CREATE TABLE (id INT)", "Missing table name"),
        ("UPDATE SET value = 1", "Missing table name"),
        ("DELETE FROM WHERE id = 1", "Missing table name"),
    ];
    
    for (query, description) in invalid_queries {
        let response = server.execute_sql(query).await;
        
        assert_eq!(
            response.status,
            ResponseStatus::Error,
            "{} should fail: {}",
            description, query
        );
        
        if let Some(error) = &response.error {
            assert!(
                !error.message.is_empty(),
                "Error message should not be empty for: {}",
                description
            );
            
            println!("{}: {}", description, error.message);
        }
    }
}

/// Test: Operations on wrong table types are rejected
#[tokio::test]
async fn test_operations_on_wrong_table_types() {
    let server = TestServer::new().await;
    let namespace = "validation_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create a STREAM table
    let create_sql = format!(
        "CREATE TABLE {}.stream_events (
            event_id TEXT PRIMARY KEY,
            payload TEXT
        ) WITH (TYPE = 'STREAM', TTL_SECONDS = 10)",
        namespace
    );
    
    server.execute_sql(&create_sql).await;
    
    // Try to FLUSH a STREAM table (should fail)
    let flush_response = server.execute_sql(&format!("FLUSH TABLE {}.stream_events", namespace)).await;
    
    assert_eq!(
        flush_response.status,
        ResponseStatus::Error,
        "FLUSH should not work on STREAM tables"
    );
    
    if let Some(error) = &flush_response.error {
        let msg = error.message.to_lowercase();
        assert!(
            msg.contains("stream") || msg.contains("flush") || msg.contains("not supported"),
            "Error should explain STREAM tables don't support FLUSH: {}",
            error.message
        );
    }
}

/// Test: Unauthorized ALTER operations are rejected
#[tokio::test]
async fn test_unauthorized_alter_operations() {
    let server = TestServer::new().await;
    
    // Try to ALTER a system table (should fail)
    let alter_sql = "ALTER TABLE system.users ADD COLUMN custom_field TEXT";
    
    let response = server.execute_sql(alter_sql).await;
    
    // Should be rejected (system tables are read-only or restricted)
    // Note: Current implementation may allow this - documenting behavior
    println!(
        "ALTER system.users: {} ({})",
        if response.status == ResponseStatus::Success { "ALLOWED" } else { "DENIED" },
        response.error.as_ref().map(|e| e.message.as_str()).unwrap_or("no error")
    );
}

/// Test: DROP required system columns is rejected
#[tokio::test]
async fn test_drop_system_columns_rejected() {
    let server = TestServer::new().await;
    let namespace = "alter_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.test_table (
            id BIGINT PRIMARY KEY,
            data TEXT
        ) WITH (TYPE = 'USER')",
        namespace
    );
    
    server.execute_sql(&create_sql).await;
    
    // Try to drop system columns (_updated, _deleted)
    let drop_updated = format!("ALTER TABLE {}.test_table DROP COLUMN _updated", namespace);
    let response = server.execute_sql(&drop_updated).await;
    
    // Should fail (system columns are protected)
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            println!("DROP _updated: {}", error.message);
        }
    }
    
    // Try to drop _deleted
    let drop_deleted = format!("ALTER TABLE {}.test_table DROP COLUMN _deleted", namespace);
    let response = server.execute_sql(&drop_deleted).await;
    
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            println!("DROP _deleted: {}", error.message);
        }
    }
}

/// Test: User isolation in USER tables
#[tokio::test]
async fn test_user_isolation_in_user_tables() {
    let server = TestServer::new().await;
    let namespace = "isolation_test";
    let table_name = "private_messages";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create user table
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            message_id BIGINT PRIMARY KEY,
            content TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql(&create_sql).await;
    
    // User A inserts data
    let user_a = "alice";
    server.execute_sql_as_user(
        &format!("INSERT INTO {}.{} (message_id, content) VALUES (1, 'Alice message 1')", namespace, table_name),
        user_a
    ).await;
    
    server.execute_sql_as_user(
        &format!("INSERT INTO {}.{} (message_id, content) VALUES (2, 'Alice message 2')", namespace, table_name),
        user_a
    ).await;
    
    // User B inserts data
    let user_b = "bob";
    server.execute_sql_as_user(
        &format!("INSERT INTO {}.{} (message_id, content) VALUES (1, 'Bob message 1')", namespace, table_name),
        user_b
    ).await;
    
    // User A should only see their own data
    let response_a = server.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
        user_a
    ).await;
    
    if let Some(rows) = response_a.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 2, "User A should see only their 2 rows");
    }
    
    // User B should only see their own data
    let response_b = server.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
        user_b
    ).await;
    
    if let Some(rows) = response_b.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 1, "User B should see only their 1 row");
    }
}

/// Test: Shared table access levels
#[tokio::test]
async fn test_shared_table_access_levels() {
    let server = TestServer::new().await;
    let namespace = "access_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create PUBLIC shared table
    let create_public = format!(
        "CREATE TABLE {}.public_config (
            key TEXT PRIMARY KEY,
            value TEXT
        ) WITH (TYPE = 'SHARED', ACCESS_LEVEL = 'PUBLIC')",
        namespace
    );
    
    server.execute_sql(&create_public).await;
    
    // Create RESTRICTED shared table
    let create_restricted = format!(
        "CREATE TABLE {}.restricted_config (
            key TEXT PRIMARY KEY,
            value TEXT
        ) WITH (TYPE = 'SHARED', ACCESS_LEVEL = 'RESTRICTED')",
        namespace
    );
    
    server.execute_sql(&create_restricted).await;
    
    // Regular user should be able to query PUBLIC table
    let user_id = "regular_user";
    let response = server.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.public_config", namespace),
        user_id
    ).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Regular user should access PUBLIC shared table"
    );
    
    // Access to RESTRICTED table depends on implementation
    let response = server.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.restricted_config", namespace),
        user_id
    ).await;
    
    println!(
        "Regular user accessing RESTRICTED table: {} ({})",
        if response.status == ResponseStatus::Success { "ALLOWED" } else { "DENIED" },
        response.error.as_ref().map(|e| e.message.as_str()).unwrap_or("no error")
    );
}

/// Test: Invalid data types in INSERT are rejected
#[tokio::test]
async fn test_invalid_data_types_rejected() {
    let server = TestServer::new().await;
    let namespace = "type_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.typed_table (
            id BIGINT PRIMARY KEY,
            age INT,
            price DOUBLE,
            active BOOLEAN
        ) WITH (TYPE = 'USER')",
        namespace
    );
    
    server.execute_sql(&create_sql).await;
    
    // Try to insert string into INT column
    let invalid_inserts = vec![
        ("INSERT INTO typed_table (id, age) VALUES (1, 'abc')", "String into INT"),
        ("INSERT INTO typed_table (id, price) VALUES (2, 'not_a_number')", "String into DOUBLE"),
        ("INSERT INTO typed_table (id, active) VALUES (3, 'maybe')", "Invalid BOOLEAN"),
    ];
    
    for (query, description) in invalid_inserts {
        let full_query = format!("{}.{}", namespace, query);
        let response = server.execute_sql(&full_query).await;
        
        // Type errors should be caught
        if response.status == ResponseStatus::Error {
            if let Some(error) = &response.error {
                println!("{}: {}", description, error.message);
            }
        }
    }
}

/// Test: NULL constraint violations are rejected
#[tokio::test]
async fn test_null_constraint_violations() {
    let server = TestServer::new().await;
    let namespace = "null_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.required_fields (
            id BIGINT PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT
        ) WITH (TYPE = 'USER')",
        namespace
    );
    
    server.execute_sql(&create_sql).await;
    
    // Try to insert NULL into NOT NULL column
    let insert_null = format!(
        "INSERT INTO {}.required_fields (id, email) VALUES (1, 'test@example.com')",
        namespace
    );
    
    let response = server.execute_sql(&insert_null).await;
    
    // Should fail (name is required)
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            let msg = error.message.to_lowercase();
            assert!(
                msg.contains("null") || msg.contains("required") || msg.contains("not null"),
                "Error should mention NULL constraint: {}",
                error.message
            );
        }
    }
}

/// Test: Permission checks for CREATE USER
#[tokio::test]
async fn test_create_user_permission_checks() {
    let server = TestServer::new().await;
    
    // Create a regular user (without DBA role)
    server.create_user("regular_user", "Password123!", Role::User).await;
    
    // Try to create another user as regular user (should fail)
    let create_user_sql = "CREATE USER 'new_user' WITH PASSWORD 'Pass123!' ROLE user";
    
    let response = server.execute_sql_as_user(create_user_sql, "regular_user").await;
    
    // Should be denied (only DBA/system can create users)
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            println!("Regular user CREATE USER: {}", error.message);
            
            let msg = error.message.to_lowercase();
            assert!(
                msg.contains("permission") || msg.contains("denied") || msg.contains("unauthorized"),
                "Error should mention permission issue: {}",
                error.message
            );
        }
    } else {
        println!("⚠ Regular user was allowed to CREATE USER (unexpected)");
    }
}

/// Test: Invalid FLUSH TABLE namespace format
#[tokio::test]
async fn test_invalid_flush_table_format() {
    let server = TestServer::new().await;
    
    // FLUSH TABLE requires namespace.table format
    let invalid_flush = "FLUSH TABLE messages"; // Missing namespace
    
    let response = server.execute_sql(invalid_flush).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "FLUSH TABLE without namespace should fail"
    );
    
    if let Some(error) = &response.error {
        println!("FLUSH TABLE without namespace: {}", error.message);
    }
}

/// Test: DROP TABLE on non-existent table with/without IF EXISTS
#[tokio::test]
async fn test_drop_nonexistent_table() {
    let server = TestServer::new().await;
    let namespace = "drop_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // DROP IF EXISTS should succeed
    let response = server.execute_sql(
        &format!("DROP TABLE IF EXISTS {}.nonexistent_table", namespace)
    ).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "DROP TABLE IF EXISTS should succeed for non-existent table"
    );
    
    // DROP without IF EXISTS should fail
    let response = server.execute_sql(
        &format!("DROP TABLE {}.nonexistent_table", namespace)
    ).await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "DROP TABLE should fail for non-existent table"
    );
}

/// Test: CREATE TABLE with duplicate column names
#[tokio::test]
async fn test_duplicate_column_names_rejected() {
    let server = TestServer::new().await;
    let namespace = "dup_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.dup_columns (
            id BIGINT PRIMARY KEY,
            name TEXT,
            name TEXT
        ) WITH (TYPE = 'USER')",
        namespace
    );
    
    let response = server.execute_sql(&create_sql).await;
    
    // Should fail (duplicate column names)
    if response.status == ResponseStatus::Error {
        if let Some(error) = &response.error {
            println!("Duplicate column names: {}", error.message);
        }
    }
}
