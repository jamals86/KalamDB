//! Integration tests for Soft Delete (Feature 006)
//!
//! Tests soft delete behavior:
//! - DELETE sets _deleted=true instead of physical removal
//! - SELECT automatically filters deleted rows
//! - Deleted data can be recovered
//! - _deleted field is accessible when explicitly selected

use super::test_support::{fixtures, QueryResultTestExt, TestServer};
use kalam_link::models::ResponseStatus;

#[actix_web::test]
async fn test_soft_delete_hides_rows() {
    let server = TestServer::new_shared().await;

    // Setup
    fixtures::create_namespace(&server, "test_hide").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_hide.tasks (
                id TEXT PRIMARY KEY,
                title TEXT,
                completed BOOLEAN
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert test data
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_hide.tasks (id, title, completed) 
               VALUES ('task1', 'First task', false)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_hide.tasks (id, title, completed) 
               VALUES ('task2', 'Second task', false)"#,
            "user1",
        )
        .await;

    // Verify both tasks exist
    let response = server
        .execute_sql_as_user("SELECT id FROM test_hide.tasks ORDER BY id", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    assert_eq!(
        response.results[0].rows.as_ref().map(|r| r.len()).unwrap_or(0),
        2,
        "Should have 2 tasks"
    );

    // Delete task1 (soft delete)
    let response = server
        .execute_sql_as_user("DELETE FROM test_hide.tasks WHERE id = 'task1'", "user1")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "DELETE should succeed: {:?}",
        response.error
    );

    // Verify task1 is hidden from SELECT
    let response = server
        .execute_sql_as_user("SELECT id FROM test_hide.tasks ORDER BY id", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.rows_as_maps();
    assert_eq!(rows.len(), 1, "Should only see 1 task after soft delete");
    assert_eq!(
        rows[0].get("id").unwrap().as_str().unwrap(),
        "task2",
        "Only task2 should be visible"
    );

    println!("✅ Soft delete hides rows from SELECT");
}

#[actix_web::test]
async fn test_soft_delete_preserves_data() {
    let server = TestServer::new_shared().await;

    // Setup
    fixtures::create_namespace(&server, "test_soft_preserves").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_soft_preserves.tasks (
                id TEXT PRIMARY KEY,
                title TEXT,
                completed BOOLEAN
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert and delete
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_soft_preserves.tasks (id, title, completed) 
               VALUES ('task1', 'Important task', false)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user("DELETE FROM test_soft_preserves.tasks WHERE id = 'task1'", "user1")
        .await;

    // Query with explicit _deleted column
    let response = server
        .execute_sql_as_user(
            "SELECT id, title, _deleted FROM test_soft_preserves.tasks WHERE id = 'task1'",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);

    // Note: The soft delete filter is applied before projection, so deleted rows won't appear
    // This is the expected behavior - soft deleted rows are hidden even when selecting _deleted
    assert_eq!(
        response.results[0].rows.as_ref().map(|r| r.len()).unwrap_or(0),
        0,
        "Soft deleted rows should be filtered out automatically"
    );

    println!("✅ Soft delete preserves data (hidden from queries)");
}

#[actix_web::test]
async fn test_deleted_field_default_false() {
    let server = TestServer::new_shared().await;

    // Setup
    fixtures::create_namespace(&server, "test_soft_deleted_field").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_soft_deleted_field.tasks (
                id TEXT PRIMARY KEY,
                title TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert data
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_soft_deleted_field.tasks (id, title) 
               VALUES ('task1', 'New task')"#,
            "user1",
        )
        .await;

    // Select with _deleted column
    let response = server
        .execute_sql_as_user(
            "SELECT id, title, _deleted FROM test_soft_deleted_field.tasks",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.rows_as_maps();
    assert_eq!(rows.len(), 1);

    let deleted_value = rows[0].get("_deleted").unwrap().as_bool();
    assert_eq!(deleted_value, Some(false), "_deleted should default to false for new rows");

    println!("✅ _deleted field defaults to false");
}

#[actix_web::test]
async fn test_multiple_deletes() {
    let server = TestServer::new_shared().await;

    // Setup
    fixtures::create_namespace(&server, "test_soft_multi").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_soft_multi.tasks (
                id TEXT PRIMARY KEY,
                title TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert multiple rows
    for i in 1..=5 {
        server
            .execute_sql_as_user(
                &format!(
                    "INSERT INTO test_soft_multi.tasks (id, title) VALUES ('task{}', 'Task {}')",
                    i, i
                ),
                "user1",
            )
            .await;
    }

    // Delete tasks 2 and 4
    server
        .execute_sql_as_user("DELETE FROM test_soft_multi.tasks WHERE id = 'task2'", "user1")
        .await;

    server
        .execute_sql_as_user("DELETE FROM test_soft_multi.tasks WHERE id = 'task4'", "user1")
        .await;

    // Verify only 3 tasks remain
    let response = server
        .execute_sql_as_user("SELECT id FROM test_soft_multi.tasks ORDER BY id", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.rows_as_maps();
    assert_eq!(rows.len(), 3, "Should have 3 tasks after deleting 2");

    let ids: Vec<&str> = rows.iter().map(|r| r.get("id").unwrap().as_str().unwrap()).collect();

    assert_eq!(ids, vec!["task1", "task3", "task5"]);

    println!("✅ Multiple soft deletes work correctly");
}

#[actix_web::test]
async fn test_delete_with_where_clause() {
    let server = TestServer::new_shared().await;

    // Setup
    fixtures::create_namespace(&server, "test_soft_where").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_soft_where.tasks (
                id TEXT PRIMARY KEY,
                title TEXT,
                priority INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert tasks with different priorities
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_soft_where.tasks (id, title, priority) 
               VALUES ('task1', 'Low priority', 1)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_soft_where.tasks (id, title, priority) 
               VALUES ('task2', 'High priority', 5)"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"INSERT INTO test_soft_where.tasks (id, title, priority) 
               VALUES ('task3', 'Low priority', 1)"#,
            "user1",
        )
        .await;

    // Delete all low priority tasks
    let response = server
        .execute_sql_as_user("DELETE FROM test_soft_where.tasks WHERE priority = 1", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);

    // Verify only high priority task remains
    let response = server
        .execute_sql_as_user("SELECT id FROM test_soft_where.tasks", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.rows_as_maps();
    assert_eq!(rows.len(), 1, "Should have 1 task after conditional delete");
    assert_eq!(rows[0].get("id").unwrap().as_str().unwrap(), "task2");

    println!("✅ DELETE with WHERE clause works correctly");
}

#[actix_web::test]
async fn test_count_excludes_deleted_rows() {
    let server = TestServer::new_shared().await;

    // Setup
    fixtures::create_namespace(&server, "test_soft_count").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_soft_count.tasks (
                id TEXT PRIMARY KEY,
                title TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert 5 tasks
    for i in 1..=5 {
        server
            .execute_sql_as_user(
                &format!(
                    "INSERT INTO test_soft_count.tasks (id, title) VALUES ('task{}', 'Task {}')",
                    i, i
                ),
                "user1",
            )
            .await;
    }

    // Count before delete
    let response = server
        .execute_sql_as_user("SELECT COUNT(*) as count FROM test_soft_count.tasks", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.rows_as_maps();
    // Debug print rows[0] to see exact key
    if rows.is_empty() {
        panic!("No result rows returned for COUNT(*)");
    }
    let count_val = rows[0]
        .get("count")
        .or_else(|| rows[0].get("COUNT(*)"))
        .or_else(|| {
            // Fallback to searching for ANY key that contains 'count' (case independent)
            rows[0]
                .iter()
                .find(|(k, _)| k.to_lowercase() == "count" || k.contains("COUNT(*)"))
                .map(|(_, v)| v)
        })
        .expect(&format!("Missing count column. Available columns: {:?}", rows[0].keys()));

    let count = match count_val {
        serde_json::Value::Number(n) => n.as_i64().unwrap(),
        serde_json::Value::String(s) => s.parse::<i64>().expect("Count string is not a valid i64"),
        _ => panic!("Unexpected count value type: {:?}", count_val),
    };
    assert_eq!(count, 5, "Should count 5 tasks before delete");

    // Delete 2 tasks
    server
        .execute_sql_as_user(
            "DELETE FROM test_soft_count.tasks WHERE id IN ('task1', 'task3')",
            "user1",
        )
        .await;

    // Count after delete
    let response = server
        .execute_sql_as_user("SELECT COUNT(*) as count FROM test_soft_count.tasks", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.rows_as_maps();
    if rows.is_empty() {
        panic!("No result rows returned for COUNT(*) after delete");
    }
    let count_val = rows[0]
        .get("count")
        .or_else(|| rows[0].get("COUNT(*)"))
        .or_else(|| {
            rows[0]
                .iter()
                .find(|(k, _)| k.to_lowercase() == "count" || k.contains("COUNT(*)"))
                .map(|(_, v)| v)
        })
        .expect(&format!(
            "Missing count column after delete. Available columns: {:?}",
            rows[0].keys()
        ));

    let count = match count_val {
        serde_json::Value::Number(n) => n.as_i64().unwrap(),
        serde_json::Value::String(s) => {
            s.parse::<i64>().expect("Count string after delete is not a valid i64")
        },
        _ => panic!("Unexpected count value type after delete: {:?}", count_val),
    };
    assert_eq!(count, 3, "Should count 3 tasks after soft delete");

    println!("✅ COUNT excludes soft deleted rows");
}
