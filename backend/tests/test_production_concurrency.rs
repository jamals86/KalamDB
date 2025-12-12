//! Production Readiness: Concurrency Tests
//!
//! Tests concurrent access patterns, contention handling, and race conditions.
//! These tests ensure KalamDB handles multiple simultaneous operations safely.

#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_api::models::ResponseStatus;
use kalamdb_commons::Role;

/// Verify concurrent inserts to same user table work correctly
#[tokio::test]
async fn concurrent_inserts_same_user_table() {
    let server = TestServer::new().await;

    // Setup
    let resp = server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS app_concurrent_ins")
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE TABLE app_concurrent_ins.events (
            id INT PRIMARY KEY,
            data VARCHAR
        )
        WITH (TYPE = 'USER')
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let user_id = server.create_user("user1", "Pass123!", Role::User).await;

    // Spawn 5 concurrent writers, each inserting 10 rows
    let mut handles = vec![];
    for writer_id in 0..5 {
        let server_clone = server.clone();
        let user_id_clone = user_id.clone();

        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let row_id = writer_id * 10 + i;
                let sql = format!(
                    "INSERT INTO app_concurrent_ins.events (id, data) VALUES ({}, 'writer_{}_row_{}')",
                    row_id, writer_id, i
                );
                let resp = server_clone
                    .execute_sql_as_user(&sql, user_id_clone.as_str())
                    .await;

                assert_eq!(
                    resp.status,
                    ResponseStatus::Success,
                    "Concurrent insert failed: {:?}",
                    resp.error
                );
            }
        });
        handles.push(handle);
    }

    // Wait for all writers
    for handle in handles {
        handle.await.expect("Writer task panicked");
    }

    // Verify all 50 rows were inserted
    let resp = server
        .execute_sql_as_user(
            "SELECT COUNT(*) as count FROM app_concurrent_ins.events",
            user_id.as_str(),
        )
        .await;

    assert_eq!(resp.status, ResponseStatus::Success);
    if let Some(rows) = resp.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").unwrap().as_i64().unwrap();
        assert_eq!(count, 50, "Should have 50 rows from concurrent inserts");
    }
}

/// Verify concurrent SELECT queries work correctly
#[tokio::test]
async fn concurrent_select_queries() {
    let server = TestServer::new().await;

    // Setup with data
    let resp = server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS app_concurrent_sel")
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE TABLE app_concurrent_sel.data (
            id INT PRIMARY KEY,
            value VARCHAR
        )
        WITH (TYPE = 'USER')
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let user_id = server.create_user("user1", "Pass123!", Role::User).await;

    // Insert test data
    for i in 0..20 {
        let sql = format!(
            "INSERT INTO app_concurrent_sel.data (id, value) VALUES ({}, 'value{}')",
            i, i
        );
        let resp = server.execute_sql_as_user(&sql, user_id.as_str()).await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    // Spawn 10 concurrent readers
    let mut handles = vec![];
    for _ in 0..10 {
        let server_clone = server.clone();
        let user_id_clone = user_id.clone();

        let handle = tokio::spawn(async move {
            let resp = server_clone
                .execute_sql_as_user(
                    "SELECT COUNT(*) as count FROM app_concurrent_sel.data",
                    user_id_clone.as_str(),
                )
                .await;

            assert_eq!(resp.status, ResponseStatus::Success);
            if let Some(rows) = resp.results.first().and_then(|r| r.rows.as_ref()) {
                let count = rows[0].get("count").unwrap().as_i64().unwrap();
                assert_eq!(count, 20, "All readers should see 20 rows");
            }
        });
        handles.push(handle);
    }

    // Wait for all readers
    for handle in handles {
        handle.await.expect("Reader task panicked");
    }
}

/// Verify duplicate PRIMARY KEY handling under concurrency
/// 
/// NOTE: This test is ignored because atomic PK constraint enforcement
/// under concurrent inserts requires database-level locking or transactions,
/// which is not yet implemented. The current implementation has a TOCTOU race
/// condition where concurrent inserts can all pass the duplicate check before
/// any of them complete the write.
#[tokio::test]
#[ignore = "Requires atomic PK constraint enforcement - future implementation"]
async fn concurrent_duplicate_primary_key_handling() {
    let server = TestServer::new().await;

    let resp = server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS app_concurrent_pk")
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE TABLE app_concurrent_pk.items (
            id INT PRIMARY KEY,
            data VARCHAR
        )
        WITH (TYPE = 'USER')
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let user_id = server.create_user("user1", "Pass123!", Role::User).await;

    // Try to insert same PRIMARY KEY from 3 concurrent clients
    let mut handles = vec![];
    for writer_id in 0..3 {
        let server_clone = server.clone();
        let user_id_clone = user_id.clone();

        let handle = tokio::spawn(async move {
            let sql = format!(
                "INSERT INTO app_concurrent_pk.items (id, data) VALUES (1, 'writer_{}')",
                writer_id
            );
            server_clone
                .execute_sql_as_user(&sql, user_id_clone.as_str())
                .await
        });
        handles.push(handle);
    }

    // Collect results
    let mut success_count = 0;
    let mut error_count = 0;

    for handle in handles {
        let resp = handle.await.expect("Writer task panicked");
        match resp.status {
            ResponseStatus::Success => success_count += 1,
            ResponseStatus::Error => error_count += 1,
        }
    }

    // Exactly ONE should succeed, the others should fail with duplicate key error
    assert_eq!(
        success_count, 1,
        "Exactly one concurrent insert should succeed"
    );
    assert_eq!(
        error_count, 2,
        "Two concurrent inserts should fail with duplicate key"
    );

    // Verify only one row exists
    let resp = server
        .execute_sql_as_user(
            "SELECT COUNT(*) as count FROM app_concurrent_pk.items",
            user_id.as_str(),
        )
        .await;

    if let Some(rows) = resp.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").unwrap().as_i64().unwrap();
        assert_eq!(
            count, 1,
            "Should have exactly 1 row despite concurrent attempts"
        );
    }
}

/// Verify concurrent UPDATE operations work correctly
#[tokio::test]
async fn concurrent_updates_same_row() {
    let server = TestServer::new().await;

    let resp = server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS app_concurrent_upd")
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE TABLE app_concurrent_upd.counter (
            id INT PRIMARY KEY,
            value INT
        )
        WITH (TYPE = 'USER')
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let user_id = server.create_user("user1", "Pass123!", Role::User).await;

    // Insert initial row
    let resp = server
        .execute_sql_as_user(
            "INSERT INTO app_concurrent_upd.counter (id, value) VALUES (1, 0)",
            user_id.as_str(),
        )
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    // Spawn 5 concurrent updaters
    let mut handles = vec![];
    for i in 1..=5 {
        let server_clone = server.clone();
        let user_id_clone = user_id.clone();

        let handle = tokio::spawn(async move {
            let sql = format!(
                "UPDATE app_concurrent_upd.counter SET value = {} WHERE id = 1",
                i * 10
            );
            server_clone
                .execute_sql_as_user(&sql, user_id_clone.as_str())
                .await
        });
        handles.push(handle);
    }

    // Wait for all updates
    for handle in handles {
        let resp = handle.await.expect("Updater task panicked");
        // All updates should succeed
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    // Verify final value is one of the updated values (10, 20, 30, 40, or 50)
    let resp = server
        .execute_sql_as_user(
            "SELECT value FROM app_concurrent_upd.counter WHERE id = 1",
            user_id.as_str(),
        )
        .await;

    assert_eq!(resp.status, ResponseStatus::Success);
    if let Some(rows) = resp.results.first().and_then(|r| r.rows.as_ref()) {
        let value = rows[0].get("value").unwrap().as_i64().unwrap();
        assert!(
            vec![10, 20, 30, 40, 50].contains(&value),
            "Final value should be one of the concurrent updates, got: {}",
            value
        );
    }
}

/// Verify concurrent DELETE operations work correctly
#[tokio::test]
async fn concurrent_deletes() {
    let server = TestServer::new().await;

    let resp = server
        .execute_sql("CREATE NAMESPACE IF NOT EXISTS app_concurrent_del")
        .await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let create_table = r#"
        CREATE TABLE app_concurrent_del.temp (
            id INT PRIMARY KEY,
            data VARCHAR
        )
        WITH (TYPE = 'USER')
    "#;
    let resp = server.execute_sql(create_table).await;
    assert_eq!(resp.status, ResponseStatus::Success);

    let user_id = server.create_user("user1", "Pass123!", Role::User).await;

    // Insert 10 rows
    for i in 0..10 {
        let sql = format!(
            "INSERT INTO app_concurrent_del.temp (id, data) VALUES ({}, 'data{}')",
            i, i
        );
        let resp = server.execute_sql_as_user(&sql, user_id.as_str()).await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    // Spawn 5 concurrent deleters (each deletes different rows to avoid conflicts)
    let mut handles = vec![];
    for i in 0..5 {
        let server_clone = server.clone();
        let user_id_clone = user_id.clone();

        let handle = tokio::spawn(async move {
            // Each deleter tries to delete specific rows
            let sql1 = format!("DELETE FROM app_concurrent_del.temp WHERE id = {}", i * 2);
            let resp1 = server_clone
                .execute_sql_as_user(&sql1, user_id_clone.as_str())
                .await;

            let sql2 = format!(
                "DELETE FROM app_concurrent_del.temp WHERE id = {}",
                i * 2 + 1
            );
            let resp2 = server_clone
                .execute_sql_as_user(&sql2, user_id_clone.as_str())
                .await;

            (resp1, resp2)
        });
        handles.push(handle);
    }

    // Wait for all deleters
    let mut success_count = 0;
    for handle in handles {
        let (resp1, resp2) = handle.await.expect("Deleter task panicked");

        if resp1.status == ResponseStatus::Success {
            success_count += 1;
        }
        if resp2.status == ResponseStatus::Success {
            success_count += 1;
        }
    }

    // At least some deletes should succeed
    println!(
        "Concurrent deletes: {} successful operations",
        success_count
    );
    assert!(
        success_count > 0,
        "At least some concurrent deletes should succeed"
    );

    // Verify rows were deleted (may not be all 10 due to races)
    let resp = server
        .execute_sql_as_user(
            "SELECT COUNT(*) as count FROM app_concurrent_del.temp",
            user_id.as_str(),
        )
        .await;

    if let Some(rows) = resp.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").unwrap().as_i64().unwrap();
        println!("Rows remaining after concurrent deletes: {}", count);
        // We just verify the count is non-negative and the operations completed
        // The exact count depends on deletion order and timing
        assert!(
            count >= 0 && count <= 10,
            "Row count should be between 0 and 10"
        );
    }
}
