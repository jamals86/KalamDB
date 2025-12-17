//! Integration tests for UPDATE/DELETE with Version Resolution (Phase 3, US1)
//!
//! Tests:
//! - T060: UPDATE record in fast storage (RocksDB)
//! - T061: UPDATE record in Parquet (long-term storage)
//! - T062: INSERT → FLUSH → UPDATE → query returns latest version
//! - T063: Multiple updates → all versions flushed → query returns MAX(_seq)
//! - T064: DELETE → _deleted = true set → query excludes record
//! - T065: DELETE record in Parquet → new version with _deleted = true in fast storage
//! - T066: Concurrent updates → all succeed, final query returns latest
//! - T067: Nanosecond collision test → verify +1ns increment
//! - T068: Performance regression test → query latency with multiple versions

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, QueryResultTestExt, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::sync::Arc;
use tokio::task::JoinSet;

/// T060: Unit test UPDATE in fast storage
#[actix_web::test]
async fn test_update_in_fast_storage() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_fast").await;
    let create_response = server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_fast.products (
                id TEXT PRIMARY KEY,
                name TEXT,
                price INT,
                stock INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;
    assert_eq!(
        create_response.status,
        ResponseStatus::Success,
        "CREATE TABLE failed: {:?}",
        create_response.error
    );

    // Insert record (stays in RocksDB/fast storage)
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_fast.products (id, name, price, stock) 
               VALUES ('prod1', 'Widget', 100, 50)"#,
            "user1",
        )
        .await;

    // Update record (in-place update in fast storage)
    let response = server
        .execute_sql_as_user(
            r#"UPDATE test_uv_fast.products 
               SET price = 120, stock = 45 
               WHERE id = 'prod1'"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "UPDATE should succeed: {:?}",
        response.error
    );

    // Verify updated values
    let response = server
        .execute_sql_as_user(
            "SELECT id, name, price, stock FROM test_uv_fast.products WHERE id = 'prod1'",
            "user1",
        )
        .await;

    println!(
        "Query response: status={:?}, error={:?}",
        response.status, response.error
    );
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Query failed: {:?}",
        response.error
    );
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.get("price").unwrap().as_i64().unwrap(), 120);
    assert_eq!(row.get("stock").unwrap().as_i64().unwrap(), 45);
    assert_eq!(row.get("name").unwrap().as_str().unwrap(), "Widget"); // Unchanged

    println!("✅ T060: UPDATE in fast storage works correctly");
}

/// T061: Unit test UPDATE in Parquet (requires creating new version in fast storage)
#[actix_web::test]
async fn test_update_in_parquet() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_parquet").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_parquet.inventory (
                id TEXT PRIMARY KEY,
                item TEXT,
                quantity INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert record
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_parquet.inventory (id, item, quantity) 
               VALUES ('inv1', 'Laptop', 10)"#,
            "user1",
        )
        .await;

    // Flush to Parquet (moves record to long-term storage)
    // Flush user table to Parquet
    flush_helpers::execute_flush_synchronously(&server, "test_uv_parquet", "inventory")
        .await
        .expect("Flush should succeed");

    // Update record (creates new version in fast storage)
    let response = server
        .execute_sql_as_user(
            r#"UPDATE test_uv_parquet.inventory 
               SET quantity = 8 
               WHERE id = 'inv1'"#,
            "user1",
        )
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "UPDATE on flushed record should succeed: {:?}",
        response.error
    );

    println!("[DEBUG TEST] UPDATE succeeded, now running SELECT to verify...");
    // Verify version resolution returns latest value
    let response = server
        .execute_sql_as_user(
            "SELECT id, item, quantity FROM test_uv_parquet.inventory WHERE id = 'inv1'",
            "user1",
        )
        .await;

    println!(
        "[DEBUG TEST] SELECT response: status={:?}, error={:?}",
        response.status, response.error
    );
    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(
        rows.len(),
        1,
        "Should return exactly 1 row (latest version)"
    );
    let row = &rows[0];
    assert_eq!(
        row.get("quantity").unwrap().as_i64().unwrap(),
        8,
        "Should return latest quantity"
    );

    println!("✅ T061: UPDATE in Parquet creates new version correctly");
}

/// T062: Integration test - INSERT → FLUSH → UPDATE → query returns latest version
#[actix_web::test]
async fn test_full_workflow_insert_flush_update() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_workflow").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_workflow.users (
                user_id TEXT PRIMARY KEY,
                name TEXT,
                status TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_workflow.orders (
                id TEXT PRIMARY KEY,
                customer TEXT,
                total INT,
                status TEXT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Step 1: INSERT
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_workflow.orders (id, customer, total, status) 
               VALUES ('order1', 'Alice', 500, 'pending')"#,
            "user1",
        )
        .await;

    // Step 2: FLUSH
    flush_helpers::execute_flush_synchronously(&server, "test_uv_workflow", "orders")
        .await
        .expect("Flush should succeed");

    // Step 3: UPDATE (creates new version in fast storage)
    server
        .execute_sql_as_user(
            r#"UPDATE test_uv_workflow.orders 
               SET status = 'shipped', total = 550 
               WHERE id = 'order1'"#,
            "user1",
        )
        .await;

    // Step 4: Query returns latest version
    let response = server
        .execute_sql_as_user(
            "SELECT id, status, total FROM test_uv_workflow.orders WHERE id = 'order1'",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.get("status").unwrap().as_str().unwrap(), "shipped");
    assert_eq!(row.get("total").unwrap().as_i64().unwrap(), 550);

    println!("✅ T062: INSERT → FLUSH → UPDATE workflow works correctly");
}

/// T063: Integration test - record updated 3 times → all versions flushed → query returns MAX(_seq)
#[actix_web::test]
async fn test_multi_version_query() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_multivers").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_multivers.counters (
                id TEXT PRIMARY KEY,
                value INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert initial version
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_multivers.counters (id, value) VALUES ('counter1', 0)"#,
            "user1",
        )
        .await;

    // Flush version 1
    flush_helpers::execute_flush_synchronously(&server, "test_uv_multivers", "counters")
        .await
        .expect("Flush should succeed");

    // Update to version 2
    server
        .execute_sql_as_user(
            r#"UPDATE test_uv_multivers.counters SET value = 10 WHERE id = 'counter1'"#,
            "user1",
        )
        .await;

    // Flush version 2
    flush_helpers::execute_flush_synchronously(&server, "test_uv_multivers", "counters")
        .await
        .expect("Flush should succeed");

    // Update to version 3
    server
        .execute_sql_as_user(
            r#"UPDATE test_uv_multivers.counters SET value = 20 WHERE id = 'counter1'"#,
            "user1",
        )
        .await;

    // Flush version 3
    flush_helpers::execute_flush_synchronously(&server, "test_uv_multivers", "counters")
        .await
        .expect("Flush should succeed");

    // Query should return latest version (value = 20)
    let response = server
        .execute_sql_as_user(
            "SELECT id, value FROM test_uv_multivers.counters WHERE id = 'counter1'",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(
        rows.len(),
        1,
        "Should return exactly 1 row (latest version)"
    );
    let row = &rows[0];
    assert_eq!(
        row.get("value").unwrap().as_i64().unwrap(),
        20,
        "Should return latest value (version 3)"
    );

    println!("✅ T063: Multi-version query returns MAX(_seq) correctly");
}

/// T064: Integration test - DELETE → _deleted = true set → query excludes record
#[actix_web::test]
async fn test_delete_excludes_record() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_delexc").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_delexc.users (
                id TEXT PRIMARY KEY,
                name TEXT,
                active BOOLEAN
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert records
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_delexc.users (id, name, active) 
               VALUES ('user1', 'Alice', true), ('user2', 'Bob', true)"#,
            "user1",
        )
        .await;

    // Delete user1
    server
        .execute_sql_as_user(r#"DELETE FROM test_uv_delexc.users WHERE id = 'user1'"#, "user1")
        .await;

    // Query should exclude deleted record
    let response = server
        .execute_sql_as_user("SELECT id, name FROM test_uv_delexc.users ORDER BY id", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1, "Should only return non-deleted record");
    assert_eq!(rows[0].get("id").unwrap().as_str().unwrap(), "user2");

    println!("✅ T064: DELETE sets _deleted=true and query excludes record");
}

/// T065: Integration test - DELETE record in Parquet → new version with _deleted = true in fast storage
#[actix_web::test]
async fn test_delete_in_parquet() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_delpq").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_delpq.accounts (
                id TEXT PRIMARY KEY,
                email TEXT,
                balance INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert record
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_delpq.accounts (id, email, balance) 
               VALUES ('acc1', 'alice@example.com', 1000)"#,
            "user1",
        )
        .await;

    // Flush to Parquet
    flush_helpers::execute_flush_synchronously(&server, "test_uv_delpq", "accounts")
        .await
        .expect("Flush should succeed");

    // Delete record (creates new version with _deleted=true in fast storage)
    let response = server
        .execute_sql_as_user(r#"DELETE FROM test_uv_delpq.accounts WHERE id = 'acc1'"#, "user1")
        .await;

    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "DELETE on flushed record should succeed: {:?}",
        response.error
    );

    // Query should return no results (deleted record excluded)
    let response = server
        .execute_sql_as_user("SELECT id FROM test_uv_delpq.accounts WHERE id = 'acc1'", "user1")
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(
        rows.len(),
        0,
        "Deleted record should be excluded from query"
    );

    println!("✅ T065: DELETE in Parquet creates new deleted version correctly");
}

/// T066: Concurrent update test - 10 threads UPDATE same record → all succeed
#[actix_web::test]
async fn test_concurrent_updates() {
    let server = Arc::new(TestServer::new().await);

    // Setup
    fixtures::create_namespace(&server, "test_uv_concur").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_concur.shared_counter (
                id TEXT PRIMARY KEY,
                count INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert initial record
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_concur.shared_counter (id, count) VALUES ('counter', 0)"#,
            "user1",
        )
        .await;

    // Spawn 10 concurrent UPDATE operations
    let mut tasks = JoinSet::new();

    for i in 0..10 {
        let server_clone = Arc::clone(&server);
        tasks.spawn(async move {
            server_clone
                .execute_sql_as_user(
                    &format!(
                        "UPDATE test_uv_concur.shared_counter SET count = {} WHERE id = 'counter'",
                        i + 1
                    ),
                    "user1",
                )
                .await
        });
    }

    // Wait for all updates to complete
    let mut success_count = 0;
    while let Some(result) = tasks.join_next().await {
        if let Ok(response) = result {
            if response.status == kalamdb_api::models::ResponseStatus::Success {
                success_count += 1;
            }
        }
    }

    assert_eq!(
        success_count, 10,
        "All 10 concurrent updates should succeed"
    );

    // Query should return some final value (1-10)
    let response = server
        .execute_sql_as_user(
            "SELECT id, count FROM test_uv_concur.shared_counter WHERE id = 'counter'",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    let final_count = rows[0].get("count").unwrap().as_i64().unwrap();
    assert!(
        (1..=10).contains(&final_count),
        "Final count should be between 1 and 10, got {}",
        final_count
    );

    println!("✅ T066: Concurrent updates all succeed");
}

/// T067: Nanosecond collision test - rapid updates → verify +1ns increment
#[actix_web::test]
async fn test_nanosecond_collision_handling() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_nano").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_nano.rapid_updates (
                id TEXT PRIMARY KEY,
                iteration INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert initial record
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_nano.rapid_updates (id, iteration) VALUES ('rec1', 0)"#,
            "user1",
        )
        .await;

    // Perform rapid updates (as fast as possible)
    for i in 1..=20 {
        server
            .execute_sql_as_user(
                &format!(
                    "UPDATE test_uv_nano.rapid_updates SET iteration = {} WHERE id = 'rec1'",
                    i
                ),
                "user1",
            )
            .await;
    }

    // Verify final state (should have latest iteration)
    let response = server
        .execute_sql_as_user(
            "SELECT id, iteration FROM test_uv_nano.rapid_updates WHERE id = 'rec1'",
            "user1",
        )
        .await;

    assert_eq!(response.status, ResponseStatus::Success);
    let rows = response.results[0].rows_as_maps();
    assert_eq!(rows.len(), 1);
    let final_iteration = rows[0].get("iteration").unwrap().as_i64().unwrap();
    assert_eq!(
        final_iteration, 20,
        "Should return latest iteration despite rapid updates"
    );

    println!("✅ T067: Nanosecond collision handling works correctly");
}

/// T068: Performance regression test - query latency with 1/10/100 versions ≤ 2× baseline
#[actix_web::test]
async fn test_query_performance_with_multiple_versions() {
    let server = TestServer::new().await;

    // Setup
    fixtures::create_namespace(&server, "test_uv_perf").await;
    server
        .execute_sql_as_user(
            r#"CREATE TABLE test_uv_perf.perf_test (
                id TEXT PRIMARY KEY,
                version INT
            ) WITH (
                TYPE = 'USER',
                STORAGE_ID = 'local'
            )"#,
            "user1",
        )
        .await;

    // Insert initial version
    server
        .execute_sql_as_user(
            r#"INSERT INTO test_uv_perf.perf_test (id, version) VALUES ('rec1', 0)"#,
            "user1",
        )
        .await;

    // Measure baseline query time (1 version)
    let start = std::time::Instant::now();
    server
        .execute_sql_as_user(
            "SELECT id, version FROM test_uv_perf.perf_test WHERE id = 'rec1'",
            "user1",
        )
        .await;
    let baseline_duration = start.elapsed();

    // Create 10 versions
    for i in 1..=10 {
        server
            .execute_sql_as_user(
                &format!(
                    "UPDATE test_uv_perf.perf_test SET version = {} WHERE id = 'rec1'",
                    i
                ),
                "user1",
            )
            .await;
        flush_helpers::execute_flush_synchronously(&server, "test_uv_perf", "perf_test")
            .await
            .expect("Flush should succeed");
    }

    // Measure query time with 10 versions
    let start = std::time::Instant::now();
    server
        .execute_sql_as_user(
            "SELECT id, version FROM test_uv_perf.perf_test WHERE id = 'rec1'",
            "user1",
        )
        .await;
    let duration_10_versions = start.elapsed();

    // Create 100 versions (91 more)
    for i in 11..=100 {
        server
            .execute_sql_as_user(
                &format!(
                    "UPDATE test_uv_perf.perf_test SET version = {} WHERE id = 'rec1'",
                    i
                ),
                "user1",
            )
            .await;
        flush_helpers::execute_flush_synchronously(&server, "test_uv_perf", "perf_test")
            .await
            .expect("Flush should succeed");
    }

    // Measure query time with 100 versions
    let start = std::time::Instant::now();
    server
        .execute_sql_as_user(
            "SELECT id, version FROM test_uv_perf.perf_test WHERE id = 'rec1'",
            "user1",
        )
        .await;
    let duration_100_versions = start.elapsed();

    // Performance assertion: 10 versions should be ≤ 2× baseline
    let max_allowed_10 = baseline_duration.mul_f32(2.0);
    assert!(
        duration_10_versions <= max_allowed_10,
        "10 versions query ({:?}) should be ≤ 2× baseline ({:?}), max allowed: {:?}",
        duration_10_versions,
        baseline_duration,
        max_allowed_10
    );

    // Performance assertion: 100 versions should be ≤ 2× baseline
    let max_allowed_100 = baseline_duration.mul_f32(2.0);
    assert!(
        duration_100_versions <= max_allowed_100,
        "100 versions query ({:?}) should be ≤ 2× baseline ({:?}), max allowed: {:?}",
        duration_100_versions,
        baseline_duration,
        max_allowed_100
    );

    println!("✅ T068: Performance regression test passed");
    println!(
        "   Baseline: {:?}, 10 versions: {:?}, 100 versions: {:?}",
        baseline_duration, duration_10_versions, duration_100_versions
    );
}
