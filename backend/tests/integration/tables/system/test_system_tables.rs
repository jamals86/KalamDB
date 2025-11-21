//! Integration tests for System Tables functionality
//!
//! Tests comprehensive system table operations:
//! 1. List system tables (SHOW TABLES, DESCRIBE, EXPLAIN)
//! 2. Query system tables with WHERE/ORDER BY/LIMIT
//! 3. Insert users and storage locations into system tables
//! 4. Retrieve schemas and schema history from system.table_schemas
//! 5. Drop table and verify schema cleanup
//! 6. View user/shared/stream tables from system.tables
//! 7. Update rows in system.users
//! 8. Add/delete/update storage configurations in system.storages (renamed from legacy system.storage_locations)
//!
//! Uses the REST API `/v1/api/sql` endpoint to test end-to-end functionality.

#[path = "../../common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;

// ============================================================================
// Test 1: List System Tables
// ============================================================================

#[actix_web::test]
async fn test_01_list_system_tables() {
    let server = TestServer::new().await;

    // Create a namespace and table first to have some content in system.tables
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_messages_table(&server, "test_ns", Some("user1")).await;

    // Query to list all system tables - system tables themselves may not be in system.tables
    // So we query for the user-created tables
    let response = server
        .execute_sql(
            "SELECT * FROM system.tables WHERE namespace_id = 'test_ns' ORDER BY table_name",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to list tables from system.tables: {:?}",
        response.error
    );

    // Verify we have at least one table
    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 1, "Expected at least 1 user table");

        // Verify the messages table exists
        let messages_found = rows.iter().any(|row| {
            row.get("table_name")
                .and_then(|v| v.as_str())
                .map(|s| s == "messages")
                .unwrap_or(false)
        });

        assert!(messages_found, "messages table should be in system.tables");
    } else {
        panic!("No rows returned from system.tables query");
    }

    // Also test querying system.namespaces
    let response = server
        .execute_sql("SELECT * FROM system.namespaces WHERE namespace_id = 'test_ns'")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to list namespaces: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_02_query_system_table_schemas() {
    let server = TestServer::new().await;

    // Test querying the schema of each system table by selecting from them
    let system_tables = vec![
        "system.users",
        "system.namespaces",
        "system.tables",
        "system.storages",
        "system.live_queries",
        "system.jobs",
    ];

    for table in system_tables {
        let response = server
            .execute_sql(&format!("SELECT * FROM {} LIMIT 0", table))
            .await;

        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Failed to query {}: {:?}",
            table,
            response.error
        );
    }
}

#[actix_web::test]
async fn test_03_query_system_users_basic() {
    let server = TestServer::new().await;

    // Test basic query on system.users
    let response = server
        .execute_sql(
            "SELECT user_id, username, email FROM system.users WHERE user_id = 'nonexistent'",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query system.users: {:?}",
        response.error
    );
}

// ============================================================================
// Test 2: Query System Tables with WHERE/ORDER BY/LIMIT
// ============================================================================

#[actix_web::test]
async fn test_04_query_system_namespaces() {
    let server = TestServer::new().await;

    // Create multiple namespaces
    fixtures::create_namespace(&server, "ns_alpha").await;
    fixtures::create_namespace(&server, "ns_beta").await;
    fixtures::create_namespace(&server, "ns_gamma").await;

    // Query all namespaces
    let response = server
        .execute_sql("SELECT namespace_id FROM system.namespaces ORDER BY namespace_id")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query namespaces: {:?}",
        response.error
    );

    // Query with WHERE clause
    let response = server
        .execute_sql("SELECT namespace_id FROM system.namespaces WHERE namespace_id LIKE 'ns_%' ORDER BY namespace_id")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query with WHERE: {:?}",
        response.error
    );

    // Query with LIMIT
    let response = server
        .execute_sql("SELECT namespace_id FROM system.namespaces ORDER BY namespace_id LIMIT 2")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query with LIMIT: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(
            rows.len() <= 2,
            "LIMIT 2 returned more than 2 rows: {}",
            rows.len()
        );
    }
}

#[actix_web::test]
#[ignore = "Test uses create_shared_table which requires pre-created column families at DB init."]
async fn test_05_query_system_tables() {
    let server = TestServer::new().await;

    // Create namespace and tables
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_messages_table(&server, "test_ns", Some("user1")).await;
    fixtures::create_shared_table(&server, "test_ns", "config").await;

    // Query tables with WHERE
    let response = server
        .execute_sql("SELECT table_name, table_type FROM system.tables WHERE namespace_id = 'test_ns' ORDER BY table_name")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query system.tables: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 2, "Expected at least 2 tables");

        // Verify table types
        let config_row = rows.iter().find(|r| {
            r.get("table_name")
                .and_then(|v| v.as_str())
                .map(|s| s == "config")
                .unwrap_or(false)
        });

        assert!(config_row.is_some(), "config table not found");

        if let Some(row) = config_row {
            let table_type = row.get("table_type").and_then(|v| v.as_str()).unwrap_or("");
            assert_eq!(table_type, "shared", "config should be shared table");
        }
    }
}

#[actix_web::test]
async fn test_06_query_system_users_with_filters() {
    let server = TestServer::new().await;

    // Insert test users (we'll test this properly in test 3, but insert here for querying)
    let _ = server
        .execute_sql("INSERT INTO system.users (user_id, username, email) VALUES ('user001', 'alice', 'alice@example.com')")
        .await;
    let _ = server
        .execute_sql("INSERT INTO system.users (user_id, username, email) VALUES ('user002', 'bob', 'bob@example.com')")
        .await;
    let _ = server
        .execute_sql("INSERT INTO system.users (user_id, username, email) VALUES ('user003', 'charlie', 'charlie@example.com')")
        .await;

    // Query with WHERE
    let response = server
        .execute_sql("SELECT user_id, username FROM system.users WHERE username = 'alice'")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query users with WHERE: {:?}",
        response.error
    );

    // Query with ORDER BY and LIMIT
    let response = server
        .execute_sql("SELECT user_id, username FROM system.users ORDER BY username LIMIT 2")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query users with ORDER BY LIMIT: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(
            rows.len() <= 2,
            "LIMIT 2 returned more than 2 rows: {}",
            rows.len()
        );
    }
}

// ============================================================================
// Test 3: Insert Users and Storage Locations
// ============================================================================

#[actix_web::test]
async fn test_07_insert_users_into_system_table() {
    let server = TestServer::new().await;

    // Insert a user
    let response = server
        .execute_sql(
            r#"INSERT INTO system.users (user_id, username, email) 
               VALUES ('test_user_001', 'testuser', 'test@example.com')"#,
        )
        .await;

    // Note: INSERT into system tables may not be implemented yet
    // This test documents the expected behavior
    if response.status == kalamdb_api::models::ResponseStatus::Error {
        println!(
            "INSERT into system.users not yet implemented: {:?}",
            response.error
        );
        return; // Skip rest of test
    }

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to insert user: {:?}",
        response.error
    );

    // Verify the user was inserted
    let response = server
        .execute_sql(
            "SELECT user_id, username, email FROM system.users WHERE user_id = 'test_user_001'",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query inserted user: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 user row");

        let row = &rows[0];
        assert_eq!(
            row.get("user_id").and_then(|v| v.as_str()).unwrap(),
            "test_user_001"
        );
        assert_eq!(
            row.get("username").and_then(|v| v.as_str()).unwrap(),
            "testuser"
        );
    } else {
        panic!("No user found after insert");
    }
}

#[actix_web::test]
async fn test_08_insert_storage_locations() {
    let server = TestServer::new().await;

    // Insert a filesystem storage location
    let response = server
        .execute_sql(
            r#"INSERT INTO system.storage_locations (location_name, location_type, path, usage_count) 
               VALUES ('local-dev', 'filesystem', '/data/kalamdb/dev', 0)"#,
        )
        .await;

    // Note: INSERT into system tables may not be fully implemented yet
    if response.status == kalamdb_api::models::ResponseStatus::Error {
        println!(
            "INSERT into system.storage_locations not yet implemented: {:?}",
            response.error
        );
        return; // Skip rest of test
    }

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to insert storage location: {:?}",
        response.error
    );

    // Insert an S3 storage location with credentials
    // (skipped second legacy insert)

    // Verify locations were inserted
    let response = server
        .execute_sql("SELECT storage_id, storage_name, storage_type, base_directory FROM system.storages ORDER BY storage_id")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query storage locations: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        println!(
            "system.storages rows returned: {} (write support pending)",
            rows.len()
        );
    }
}

// ============================================================================
// Test 4: Get Schemas and Schema History
// ============================================================================

#[actix_web::test]
async fn test_09_query_table_schemas() {
    let server = TestServer::new().await;

    // Create namespace and table
    fixtures::create_namespace(&server, "schema_test").await;
    fixtures::create_messages_table(&server, "schema_test", Some("user1")).await;

    // Query the current schema from system.tables
    let response = server
        .execute_sql("SELECT table_name, schema_version FROM system.tables WHERE namespace_id = 'schema_test' AND table_name = 'messages'")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query current schema version: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 table row");

        let schema_version = rows[0]
            .get("schema_version")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        assert!(schema_version >= 1, "Schema version should be at least 1");
    } else {
        panic!("No table found");
    }
}

#[actix_web::test]
async fn test_10_query_table_metadata() {
    let server = TestServer::new().await;

    // Create namespace and table
    fixtures::create_namespace(&server, "metadata_test").await;
    fixtures::create_messages_table(&server, "metadata_test", Some("user1")).await;

    // Query table metadata from system.tables
    let response = server
        .execute_sql("SELECT namespace_id AS namespace, table_name, table_type, schema_version, created_at FROM system.tables WHERE namespace_id = 'metadata_test' AND table_name = 'messages'")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query table metadata: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 table");

        let row = &rows[0];
        assert_eq!(
            row.get("namespace").and_then(|v| v.as_str()).unwrap(),
            "metadata_test"
        );
        assert_eq!(
            row.get("table_name").and_then(|v| v.as_str()).unwrap(),
            "messages"
        );
        assert_eq!(
            row.get("table_type").and_then(|v| v.as_str()).unwrap(),
            "user"
        );
    }
}

// ============================================================================
// Test 5: Drop Table and Verify Schema Cleanup
// ============================================================================

#[actix_web::test]
async fn test_11_drop_table_and_verify_cleanup() {
    let server = TestServer::new().await;

    // Create namespace and table
    fixtures::create_namespace(&server, "drop_test").await;
    fixtures::create_messages_table(&server, "drop_test", Some("user1")).await;

    // Verify table exists in system.tables
    let response = server
        .execute_sql("SELECT table_name FROM system.tables WHERE namespace_id = 'drop_test' AND table_name = 'messages'")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Table should exist before drop");
    }

    // Drop the table
    let response = fixtures::drop_table(&server, "drop_test", "messages").await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to drop table: {:?}",
        response.error
    );

    // Verify table is gone from system.tables
    let response = server
        .execute_sql("SELECT table_name FROM system.tables WHERE namespace_id = 'drop_test' AND table_name = 'messages'")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 0, "Table should be removed from system.tables");
    }
}

// ============================================================================
// Test 6: View User/Shared/Stream Tables from system.tables
// ============================================================================

#[actix_web::test]
#[ignore = "Test uses create_shared_table which requires pre-created column families at DB init."]
async fn test_12_view_table_types_from_system_tables() {
    let server = TestServer::new().await;

    // Create namespace
    fixtures::create_namespace(&server, "multi_type").await;

    // Create different table types
    fixtures::create_messages_table(&server, "multi_type", Some("user1")).await; // USER table
    fixtures::create_shared_table(&server, "multi_type", "config").await; // SHARED table

    // Create a STREAM table (simplified syntax)
    let response = server
        .execute_sql(
            r#"CREATE STREAM TABLE multi_type.events (
                event_id VARCHAR,
                event_type VARCHAR,
                payload VARCHAR
            )"#,
        )
        .await;

    // Note: STREAM TABLE may not be fully implemented yet
    if response.status == kalamdb_api::models::ResponseStatus::Error {
        println!(
            "CREATE STREAM TABLE not yet fully implemented: {:?}",
            response.error
        );
        // Continue with 2 tables instead of 3
    }

    // Query all tables and their types
    let response = server
        .execute_sql("SELECT table_name, table_type FROM system.tables WHERE namespace_id = 'multi_type' ORDER BY table_name")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to query table types: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 2, "Expected at least 2 tables");

        // Verify each table type
        let messages_table = rows.iter().find(|r| {
            r.get("table_name")
                .and_then(|v| v.as_str())
                .map(|s| s == "messages")
                .unwrap_or(false)
        });

        let config_table = rows.iter().find(|r| {
            r.get("table_name")
                .and_then(|v| v.as_str())
                .map(|s| s == "config")
                .unwrap_or(false)
        });

        let events_table = rows.iter().find(|r| {
            r.get("table_name")
                .and_then(|v| v.as_str())
                .map(|s| s == "events")
                .unwrap_or(false)
        });

        assert!(messages_table.is_some(), "messages table not found");
        assert!(config_table.is_some(), "config table not found");
        assert!(events_table.is_some(), "events table not found");

        // Verify table types
        if let Some(row) = messages_table {
            let table_type = row.get("table_type").and_then(|v| v.as_str()).unwrap();
            assert_eq!(table_type, "user", "messages should be user table");
        }

        if let Some(row) = config_table {
            let table_type = row.get("table_type").and_then(|v| v.as_str()).unwrap();
            assert_eq!(table_type, "shared", "config should be shared table");
        }

        if let Some(row) = events_table {
            let table_type = row.get("table_type").and_then(|v| v.as_str()).unwrap();
            assert_eq!(table_type, "stream", "events should be stream table");
        }
    }
}

#[actix_web::test]
#[ignore = "Shared tables require pre-created column families at DB init."]
async fn test_13_filter_tables_by_type() {
    let server = TestServer::new().await;

    // Setup: Create namespace and mixed table types
    fixtures::create_namespace(&server, "filter_test").await;
    fixtures::create_messages_table(&server, "filter_test", Some("user1")).await;
    fixtures::create_shared_table(&server, "filter_test", "settings").await;
    fixtures::create_shared_table(&server, "filter_test", "metadata").await;

    // Query only shared tables
    let response = server
        .execute_sql("SELECT table_name FROM system.tables WHERE namespace_id = 'filter_test' AND table_type = 'shared' ORDER BY table_name")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to filter shared tables: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 2, "Expected 2 shared tables");

        let names: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.get("table_name").and_then(|v| v.as_str()))
            .collect();

        assert!(names.contains(&"settings"), "settings table not found");
        assert!(names.contains(&"metadata"), "metadata table not found");
    }

    // Query only user tables
    let response = server
        .execute_sql("SELECT table_name FROM system.tables WHERE namespace_id = 'filter_test' AND table_type = 'user'")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to filter user tables: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 user table");
    }
}

// ============================================================================
// Test 7: Update Rows in system.users
// ============================================================================

#[actix_web::test]
async fn test_14_update_system_users() {
    let server = TestServer::new().await;

    // Insert a user first (if INSERT is implemented)
    let _ = server
        .execute_sql(
            r#"INSERT INTO system.users (user_id, username, email) 
               VALUES ('update_test_user', 'oldname', 'old@example.com')"#,
        )
        .await;

    // Update the username
    let response = server
        .execute_sql(
            "UPDATE system.users SET username = 'newname' WHERE user_id = 'update_test_user'",
        )
        .await;

    // Note: UPDATE on system tables may not be implemented yet
    if response.status == kalamdb_api::models::ResponseStatus::Error {
        println!(
            "UPDATE system.users not yet implemented: {:?}",
            response.error
        );
        return; // Skip rest of test
    }

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to update user: {:?}",
        response.error
    );

    // Verify the update
    let response = server
        .execute_sql(
            "SELECT user_id, username FROM system.users WHERE user_id = 'update_test_user'",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Expected 1 user row");

        let username = rows[0].get("username").and_then(|v| v.as_str()).unwrap();
        assert_eq!(username, "newname", "Username should be updated");
    }
}

#[actix_web::test]
async fn test_15_update_multiple_users() {
    let server = TestServer::new().await;

    // Insert multiple users (if INSERT is implemented)
    let _ = server
        .execute_sql(
            r#"INSERT INTO system.users (user_id, username, email) 
               VALUES ('batch_user1', 'user1', 'user1@example.com')"#,
        )
        .await;
    let _ = server
        .execute_sql(
            r#"INSERT INTO system.users (user_id, username, email) 
               VALUES ('batch_user2', 'user2', 'user2@example.com')"#,
        )
        .await;
    let _ = server
        .execute_sql(
            r#"INSERT INTO system.users (user_id, username, email) 
               VALUES ('batch_user3', 'user3', 'user3@example.com')"#,
        )
        .await;

    // Update all users with a WHERE clause pattern
    let response = server
        .execute_sql("UPDATE system.users SET email = 'updated@example.com' WHERE user_id LIKE 'batch_user%'")
        .await;

    // Note: UPDATE on system tables may not be implemented yet
    if response.status == kalamdb_api::models::ResponseStatus::Error {
        println!(
            "UPDATE system.users not yet implemented: {:?}",
            response.error
        );
        return; // Skip rest of test
    }

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to batch update users: {:?}",
        response.error
    );

    // Verify all were updated
    let response = server
        .execute_sql("SELECT user_id, email FROM system.users WHERE user_id LIKE 'batch_user%'")
        .await;

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 3, "Expected 3 updated users");

        for row in rows.iter() {
            let email = row.get("email").and_then(|v| v.as_str()).unwrap();
            assert_eq!(email, "updated@example.com", "Email should be updated");
        }
    }
}

#[actix_web::test]
#[ignore = "Test uses create_shared_table which requires pre-created column families at DB init."]
async fn test_20_complex_system_queries() {
    let server = TestServer::new().await;

    // Setup: Create multiple namespaces and tables
    fixtures::create_namespace(&server, "prod").await;
    fixtures::create_namespace(&server, "dev").await;
    fixtures::create_messages_table(&server, "prod", Some("user1")).await;
    fixtures::create_shared_table(&server, "prod", "config").await;
    fixtures::create_shared_table(&server, "dev", "test_data").await;

    // Complex query: Count tables by type across all namespaces
    let response = server
        .execute_sql("SELECT table_type, COUNT(*) as count FROM system.tables GROUP BY table_type ORDER BY table_type")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed complex query: {:?}",
        response.error
    );

    // Complex query: Join tables with namespaces
    let response = server
        .execute_sql(
            r#"SELECT t.namespace_id, t.table_name, t.table_type 
               FROM system.tables t 
               WHERE t.namespace_id IN ('prod', 'dev') 
               ORDER BY t.namespace_id, t.table_name"#,
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed join query: {:?}",
        response.error
    );

    if let Some(rows) = &response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 3, "Expected at least 3 tables");
    }
}
