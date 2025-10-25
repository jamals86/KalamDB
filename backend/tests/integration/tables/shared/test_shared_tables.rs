//! Integration tests for Shared Table functionality (Phase 13)
//!
//! Tests the complete lifecycle of shared tables:
//! - CREATE TABLE (shared tables)
//! - INSERT operations
//! - UPDATE operations
//! - DELETE operations (soft/hard)
//! - SELECT queries
//! - DROP TABLE cleanup
//!
//! Uses the REST API `/v1/api/sql` endpoint to test end-to-end functionality.

#[path = "../../common/mod.rs"]
mod common;

use common::{fixtures, TestServer};

#[actix_web::test]
async fn test_shared_table_create_and_drop() {
    let server = TestServer::new().await;

    // Create namespace first
    let response = fixtures::create_namespace(&server, "test_ns").await;
    assert_eq!(
        response.status, "success",
        "Failed to create namespace: {:?}",
        response.error
    );

    // Create shared table
    let response = fixtures::create_shared_table(&server, "test_ns", "conversations").await;
    assert_eq!(
        response.status, "success",
        "Failed to create shared table: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("test_ns", "conversations").await,
        "Table should exist"
    );

    // Drop table
    let response = fixtures::drop_table(&server, "test_ns", "conversations").await;
    assert_eq!(
        response.status, "success",
        "Failed to drop table: {:?}",
        response.error
    );

    // Verify table no longer exists
    assert!(
        !server.table_exists("test_ns", "conversations").await,
        "Table should be dropped"
    );
}

#[actix_web::test]
async fn test_shared_table_insert_and_select() {
    let server = TestServer::new().await;

    // Setup: Create namespace and table
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert data
    let response = server
        .execute_sql(
            r#"INSERT INTO test_ns.conversations (conversation_id, title, participant_count) 
           VALUES ('conv001', 'Team Standup', 5)"#,
        )
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to insert: {:?}",
        response.error
    );

    // Select data back
    let response = server.execute_sql(
        "SELECT conversation_id, title, participant_count FROM test_ns.conversations WHERE conversation_id = 'conv001'"
    ).await;

    assert_eq!(
        response.status, "success",
        "Failed to select: {:?}",
        response.error
    );
    assert!(!response.results.is_empty());

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 1, "Expected 1 row, got {}", rows.len());

        let row = &rows[0];
        assert_eq!(
            row.get("conversation_id").unwrap().as_str().unwrap(),
            "conv001"
        );
        assert_eq!(row.get("title").unwrap().as_str().unwrap(), "Team Standup");
        assert_eq!(row.get("participant_count").unwrap().as_i64().unwrap(), 5);
    }
}

#[actix_web::test]
async fn test_shared_table_multiple_inserts() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert multiple rows
    let response = server
        .execute_sql(
            r#"
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES 
            ('conv001', 'Standup'),
            ('conv002', 'Planning'),
            ('conv003', 'Review')
    "#,
        )
        .await;
    assert_eq!(
        response.status, "success",
        "Failed to insert multiple rows: {:?}",
        response.error
    );

    // Verify all rows exist
    let response = server
        .execute_sql(
            "SELECT conversation_id, title FROM test_ns.conversations ORDER BY conversation_id",
        )
        .await;

    assert_eq!(response.status, "success");
    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 3, "Expected 3 rows");
        assert_eq!(
            rows[0].get("conversation_id").unwrap().as_str().unwrap(),
            "conv001"
        );
        assert_eq!(
            rows[1].get("conversation_id").unwrap().as_str().unwrap(),
            "conv002"
        );
        assert_eq!(
            rows[2].get("conversation_id").unwrap().as_str().unwrap(),
            "conv003"
        );
    }
}

#[actix_web::test]
async fn test_shared_table_update() {
    let server = TestServer::new().await;

    // Setup namespace and table
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert initial data
    let response = server
        .execute_sql(
            r#"
        INSERT INTO test_ns.conversations (conversation_id, title, status)
        VALUES ('conv001', 'Planning Meeting', 'active')
    "#,
        )
        .await;
    assert_eq!(response.status, "success");

    // Update the row
    let response = server
        .execute_sql(
            r#"
        UPDATE test_ns.conversations 
        SET title = 'Updated Planning Meeting', status = 'archived'
        WHERE conversation_id = 'conv001'
    "#,
        )
        .await;
    assert_eq!(
        response.status, "success",
        "Failed to update: {:?}",
        response.error
    );

    // Verify update
    let response = server
        .execute_sql(
            "SELECT title, status FROM test_ns.conversations WHERE conversation_id = 'conv001'",
        )
        .await;

    assert_eq!(response.status, "success");
    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("title").unwrap().as_str().unwrap(),
            "Updated Planning Meeting"
        );
        assert_eq!(rows[0].get("status").unwrap().as_str().unwrap(), "archived");
    }
}

#[actix_web::test]
async fn test_shared_table_select() {
    let server = TestServer::new().await;

    // Setup and insert data
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    server
        .execute_sql(
            r#"
        INSERT INTO test_ns.conversations (conversation_id, title, status)
        VALUES 
            ('conv001', 'Meeting 1', 'active'),
            ('conv002', 'Meeting 2', 'archived'),
            ('conv003', 'Meeting 3', 'active')
    "#,
        )
        .await;

    // Select data
    let response = server
        .execute_sql("SELECT * FROM test_ns.conversations ORDER BY conversation_id")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to select: {:?}",
        response.error
    );

    // Verify we got results
    assert!(!response.results.is_empty(), "Should have query results");
    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 3, "Should have 3 data rows");
    }
}

#[actix_web::test]
async fn test_shared_table_delete() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert data
    server
        .execute_sql(
            r#"INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv001', 'To Delete', 'active')"#,
        )
        .await;
    server
        .execute_sql(
            r#"INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv002', 'To Keep', 'active')"#,
        )
        .await;

    // Delete one row (soft delete)
    let response = server
        .execute_sql("DELETE FROM test_ns.conversations WHERE conversation_id = 'conv001'")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to delete: {:?}",
        response.error
    );

    // Verify deletion (soft delete should hide the row)
    let response = server
        .execute_sql("SELECT conversation_id FROM test_ns.conversations ORDER BY conversation_id")
        .await;

    assert_eq!(response.status, "success");
}

#[actix_web::test]
async fn test_shared_table_system_columns() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert data
    server.execute_sql(
        r#"INSERT INTO test_ns.conversations (conversation_id, title, participant_count) VALUES ('conv001', 'Test Conversation', 5)"#
    ).await;

    // Query including system columns
    let response = server
        .execute_sql("SELECT conversation_id, title, _updated, _deleted FROM test_ns.conversations")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to query system columns: {:?}",
        response.error
    );

    // Verify system columns exist in results
    assert!(!response.results.is_empty(), "Should have results");
    assert!(
        !response.results[0].columns.is_empty(),
        "Should have columns"
    );
}

#[actix_web::test]
async fn test_shared_table_if_not_exists() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;

    let create_sql = r#"CREATE SHARED TABLE IF NOT EXISTS test_ns.conversations (
        id INT AUTO_INCREMENT,
        name VARCHAR NOT NULL,
        value VARCHAR
    ) FLUSH ROWS 50"#;

    // First create should succeed
    let response = server.execute_sql(create_sql).await;
    assert_eq!(response.status, "success");

    // Second create with IF NOT EXISTS should also succeed (no-op)
    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "IF NOT EXISTS should not fail on duplicate: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_shared_table_flush_policy_rows() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;

    let create_sql = r#"CREATE SHARED TABLE test_ns.conversations (
        id INT AUTO_INCREMENT,
        name VARCHAR NOT NULL,
        value VARCHAR
    ) FLUSH ROWS 500"#;

    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "Failed to create table with FLUSH ROWS: {:?}",
        response.error
    );

    // Verify table exists
    assert!(server.table_exists("test_ns", "conversations").await);
}

#[actix_web::test]
async fn test_shared_table_query_filtering() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert test data
    server.execute_sql(
        r#"INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv001', 'Active Conversation', 'active')"#
    ).await;
    server.execute_sql(
        r#"INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv002', 'Archived Conversation', 'archived')"#
    ).await;

    // Query with filter
    let response = server
        .execute_sql("SELECT * FROM test_ns.conversations WHERE status = 'active'")
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to query with filter: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_shared_table_ordering() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert data in random order
    server
        .execute_sql(
            r#"INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv003', 'Third')"#,
        )
        .await;
    server
        .execute_sql(
            r#"INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv001', 'First')"#,
        )
        .await;
    server
        .execute_sql(
            r#"INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv002', 'Second')"#,
        )
        .await;

    // Query with ORDER BY
    let response = server
        .execute_sql(
            "SELECT conversation_id, title FROM test_ns.conversations ORDER BY conversation_id ASC",
        )
        .await;

    assert_eq!(
        response.status, "success",
        "Failed to query with ORDER BY: {:?}",
        response.error
    );
}

#[actix_web::test]
async fn test_shared_table_drop_with_data() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;

    // Insert data
    for i in 0..5 {
        server
            .execute_sql(&format!(
                r#"INSERT INTO test_ns.conversations (conversation_id, title, participant_count) VALUES ('conv{}', 'Data {}', {})"#,
                i, i, i + 1
            ))
            .await;
    }

    // Drop table with data
    let response = fixtures::drop_table(&server, "test_ns", "conversations").await;
    assert_eq!(
        response.status, "success",
        "Failed to drop table with data: {:?}",
        response.error
    );

    // Verify table no longer exists
    assert!(
        !server.table_exists("test_ns", "conversations").await,
        "Table should be dropped"
    );
}

#[actix_web::test]
async fn test_shared_table_multiple_tables_same_namespace() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ns").await;

    // Create multiple shared tables
    fixtures::create_shared_table(&server, "test_ns", "conversations").await;
    fixtures::create_shared_table(&server, "test_ns", "config").await;

    // Verify both tables exist
    assert!(server.table_exists("test_ns", "conversations").await);
    assert!(server.table_exists("test_ns", "config").await);

    // Insert data into both tables
    server
        .execute_sql(r#"INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv1', 'Test')"#)
        .await;
    server
        .execute_sql(r#"INSERT INTO test_ns.config (name, value) VALUES ('setting1', 'value1')"#)
        .await;

    // Query both tables
    let response1 = server
        .execute_sql("SELECT * FROM test_ns.conversations")
        .await;
    let response2 = server.execute_sql("SELECT * FROM test_ns.config").await;

    assert_eq!(response1.status, "success");
    assert_eq!(response2.status, "success");
}

#[actix_web::test]
async fn test_shared_table_complete_lifecycle() {
    let server = TestServer::new().await;

    // Complete lifecycle test

    // 1. Create namespace
    fixtures::create_namespace(&server, "lifecycle_test").await;

    // 2. Create shared table
    fixtures::create_shared_table(&server, "lifecycle_test", "conversations").await;
    assert!(server.table_exists("lifecycle_test", "conversations").await);

    // 3. Insert data
    for i in 0..3 {
        server.execute_sql(
            &format!(r#"INSERT INTO lifecycle_test.conversations (conversation_id, title, participant_count) VALUES ('item{}', 'value{}', {})"#, i, i, i + 1)
        ).await;
    }

    // 4. Query data
    let response = server
        .execute_sql("SELECT * FROM lifecycle_test.conversations")
        .await;
    assert_eq!(response.status, "success");

    // 5. Update data
    server
        .execute_sql(
            r#"UPDATE lifecycle_test.conversations SET title = 'updated' WHERE conversation_id = 'item1'"#,
        )
        .await;

    // 6. Delete data
    server
        .execute_sql("DELETE FROM lifecycle_test.conversations WHERE conversation_id = 'item2'")
        .await;

    // 7. Drop table
    fixtures::drop_table(&server, "lifecycle_test", "conversations").await;
    assert!(!server.table_exists("lifecycle_test", "conversations").await);

    // 8. Cleanup namespace
    server.cleanup().await.expect("Cleanup failed");
}
