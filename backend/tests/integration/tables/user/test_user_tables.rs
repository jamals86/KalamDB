//! Integration tests for User Table functionality (Phase 18 - T234-T236, T241)
//!
//! Tests the complete lifecycle of user tables:
//! - CREATE TABLE (TYPE = 'USER') with user_id scoping
//! - INSERT operations with user isolation
//! - UPDATE operations with user isolation
//! - DELETE operations (soft delete) with user isolation
//! - SELECT queries with user data filtering
//! - User data isolation (user1 can't see user2's data)
//!
//! Uses the REST API `/v1/api/sql` endpoint to test end-to-end functionality.

#[path = "../../common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::{ResponseStatus, SqlResponse};

/// Helper to create a user table for testing
async fn create_user_table(
    server: &TestServer,
    namespace: &str,
    table_name: &str,
    user_id: &str,
) -> SqlResponse {
    // Ensure a clean slate per test: drop the table if it already exists
    let _ = server
        .execute_sql_as_user(
            &format!("DROP TABLE {}.{}", namespace, table_name),
            "system",
        )
        .await;

    server
        .execute_sql_as_user(
            &format!(
                r#"CREATE TABLE {}.{} (
                id TEXT PRIMARY KEY,
                content TEXT,
                priority INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
                namespace, table_name
            ),
            user_id,
        )
        .await
}

#[actix_web::test]
async fn test_user_table_create_and_basic_insert() {
    let server = TestServer::new().await;

    // Create namespace first
    let response = fixtures::create_namespace(&server, "test_ut_create").await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create namespace: {:?}",
        response.error
    );

    // Create user table as user1
    let response = create_user_table(&server, "test_ut_create", "notes", "user1").await;
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to create user table: {:?}",
        response.error
    );

    // Insert data as user1
    let response = server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_create.notes (id, content, priority) 
           VALUES ('note1', 'First note', 1)"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to insert: {:?}",
        response.error
    );

    // Verify table exists
    assert!(
        server.table_exists("test_ut_create", "notes").await,
        "Table should exist"
    );
}

#[actix_web::test]
async fn test_user_table_data_isolation() {
    let server = TestServer::new().await;

    // Setup namespace and table
    fixtures::create_namespace(&server, "test_ut_isolate").await;
    create_user_table(&server, "test_ut_isolate", "notes", "user1").await;

    // Insert data as user1
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_isolate.notes (id, content, priority) 
           VALUES ('note1', 'User1 note', 1)"#,
            "user1",
        )
        .await;

    // Insert data as user2
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_isolate.notes (id, content, priority) 
           VALUES ('note2', 'User2 note', 2)"#,
            "user2",
        )
        .await;

    // User1 selects - should only see their own data
    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ut_isolate.notes", "user1")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to select as user1: {:?}",
        response.error
    );

    if let Some(rows) = &response.results[0].rows {
        // User1 should only see their own note
        assert_eq!(rows.len(), 1, "User1 should only see 1 row");

        let row = &rows[0];
        assert_eq!(row.get("id").unwrap().as_str().unwrap(), "note1");
        assert_eq!(row.get("content").unwrap().as_str().unwrap(), "User1 note");
    } else {
        panic!("Expected rows in response");
    }

    // User2 selects - should only see their own data
    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ut_isolate.notes", "user2")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to select as user2: {:?}",
        response.error
    );

    if let Some(rows) = &response.results[0].rows {
        // User2 should only see their own note
        assert_eq!(rows.len(), 1, "User2 should only see 1 row");

        let row = &rows[0];
        assert_eq!(row.get("id").unwrap().as_str().unwrap(), "note2");
        assert_eq!(row.get("content").unwrap().as_str().unwrap(), "User2 note");
    } else {
        panic!("Expected rows in response");
    }
}

#[actix_web::test]
async fn test_user_table_update_with_isolation() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ut_update").await;
    create_user_table(&server, "test_ut_update", "notes", "user1").await;

    // Insert as user1
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_update.notes (id, content, priority) 
           VALUES ('note1', 'Original', 1)"#,
            "user1",
        )
        .await;

    // Insert as user2
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_update.notes (id, content, priority) 
           VALUES ('note2', 'User2 note', 2)"#,
            "user2",
        )
        .await;

    // User1 updates their note
    let response = server
        .execute_sql_as_user(
            "UPDATE test_ut_update.notes SET content = 'Updated by user1' WHERE id = 'note1'",
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to update: {:?}",
        response.error
    );

    // Verify user1's update
    let response = server
        .execute_sql_as_user(
            "SELECT id, content FROM test_ut_update.notes WHERE id = 'note1'",
            "user1",
        )
        .await;

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("content").unwrap().as_str().unwrap(),
            "Updated by user1"
        );
    }

    // Verify user2's data unchanged
    let response = server
        .execute_sql_as_user(
            "SELECT id, content FROM test_ut_update.notes WHERE id = 'note2'",
            "user2",
        )
        .await;

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("content").unwrap().as_str().unwrap(),
            "User2 note"
        );
    }
}

#[actix_web::test]
async fn test_user_table_delete_with_isolation() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ut_delete").await;
    create_user_table(&server, "test_ut_delete", "notes", "user1").await;

    // Insert as both users
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_delete.notes (id, content, priority) 
           VALUES ('note1', 'User1 note', 1)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_delete.notes (id, content, priority) 
           VALUES ('note2', 'User2 note', 2)"#,
            "user2",
        )
        .await;

    // User1 deletes their note
    let response = server
        .execute_sql_as_user("DELETE FROM test_ut_delete.notes WHERE id = 'note1'", "user1")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to delete: {:?}",
        response.error
    );

    // Verify user1's data is deleted (soft delete - _deleted=true)
    let response = server
        .execute_sql_as_user(
            "SELECT id, content FROM test_ut_delete.notes WHERE id = 'note1'",
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "SELECT after DELETE failed: {:?}",
        response.error
    );

    if !response.results.is_empty() {
        if let Some(rows) = &response.results[0].rows {
            // Soft delete should filter out deleted rows
            assert_eq!(rows.len(), 0, "Deleted row should not be visible");
        }
    }

    // Verify user2's data still exists
    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ut_delete.notes", "user2")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "SELECT user2 data failed: {:?}",
        response.error
    );

    if !response.results.is_empty() {
        if let Some(rows) = &response.results[0].rows {
            assert_eq!(rows.len(), 1, "User2's data should still exist");
            assert_eq!(rows[0].get("id").unwrap().as_str().unwrap(), "note2");
        }
    }
}

#[actix_web::test]
async fn test_user_table_system_columns() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ut_syscol").await;
    create_user_table(&server, "test_ut_syscol", "notes", "user1").await;

    // Insert
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_syscol.notes (id, content, priority) 
           VALUES ('note1', 'Test note', 1)"#,
            "user1",
        )
        .await;

    // Select with system columns
    let response = server
        .execute_sql_as_user(
            "SELECT id, content, _seq, _deleted FROM test_ut_syscol.notes WHERE id = 'note1'",
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Failed to select system columns: {:?}",
        response.error
    );

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 1);

        let row = &rows[0];
        // Verify system columns exist
        assert!(row.contains_key("_seq"), "_seq column should exist");
        assert!(row.contains_key("_deleted"), "_deleted column should exist");

        // _deleted should be false for new rows
        assert_eq!(row.get("_deleted").unwrap().as_bool().unwrap(), false);
    } else {
        panic!("Expected rows in response");
    }
}

#[actix_web::test]
async fn test_user_table_multiple_inserts() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ut_multi").await;
    create_user_table(&server, "test_ut_multi", "notes", "user1").await;

    // Insert multiple rows
    for i in 1..=5 {
        server
            .execute_sql_as_user(
                &format!(
                    r#"INSERT INTO test_ut_multi.notes (id, content, priority) 
                   VALUES ('note{}', 'Note {}', {})"#,
                    i, i, i
                ),
                "user1",
            )
            .await;
    }

    // Verify count
    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ut_multi.notes", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 5, "Should have 5 rows");
    }
}

#[actix_web::test]
async fn test_user_table_user_cannot_access_other_users_data() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_ut_access").await;
    create_user_table(&server, "test_ut_access", "notes", "user1").await;

    // Insert as user1
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_ut_access.notes (id, content, priority) 
           VALUES ('note1', 'Secret user1 data', 10)"#,
            "user1",
        )
        .await;

    // Try to read as user2 - should not see user1's data
    let response = server
        .execute_sql_as_user("SELECT id, content FROM test_ut_access.notes", "user2")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(
            rows.len(),
            0,
            "User2 should not see any data (user1's data is isolated)"
        );
    }

    // Try to update user1's data as user2 - should have no effect
    let response = server
        .execute_sql_as_user(
            "UPDATE test_ut_access.notes SET content = 'Hacked!' WHERE id = 'note1'",
            "user2",
        )
        .await;

    // Update might succeed but should affect 0 rows
    assert_eq!(response.status, ResponseStatus::Success);

    // Verify user1's data unchanged
    let response = server
        .execute_sql_as_user(
            "SELECT id, content FROM test_ut_access.notes WHERE id = 'note1'",
            "user1",
        )
        .await;

    if let Some(rows) = &response.results[0].rows {
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("content").unwrap().as_str().unwrap(),
            "Secret user1 data",
            "User1's data should not be modified by user2"
        );
    }
}
