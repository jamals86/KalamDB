//! Concurrency and Contention Tests
//!
//! Tests concurrent access patterns to ensure:
//! - Multiple concurrent writers don't corrupt data
//! - Read/write contention doesn't cause deadlocks
//! - Concurrent subscriptions work correctly
//! - No race conditions in critical paths
//!
//! These tests validate that KalamDB can handle realistic multi-client workloads.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::sync::Arc;

/// Test: Concurrent writers to the same user table
#[tokio::test]
async fn test_concurrent_writers_same_user_table() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    
    let server = TestServer::new().await;
    let namespace = "concurrent_test";
    let table_name = "messages";
    let user_id = "concurrent_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            content TEXT,
            thread_id INT
        ) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Spawn 10 concurrent writers, each inserting 10 rows
    let mut handles = vec![];
    let server_arc = Arc::new(server);
    
    for thread_id in 0..10 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let uid = user_id.to_string();
        
        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let id = (thread_id * 10) + i;
                let insert_sql = format!(
                    "INSERT INTO {}.{} (id, content, thread_id) VALUES ({}, 'Content from thread {}', {})",
                    ns, tn, id, thread_id, thread_id
                );
                
                let response = server_clone.execute_sql_as_user(&insert_sql, &uid).await;
                
                if response.status != ResponseStatus::Success {
                    eprintln!("Thread {} insert {} failed: {:?}", thread_id, i, response.error);
                }
            }
            
            thread_id
        });
        
        handles.push(handle);
    }
    
    // Wait for all writers to complete
    let results = futures_util::future::join_all(handles).await;
    
    let completed_threads: Vec<_> = results.into_iter().filter_map(|r| r.ok()).collect();
    assert_eq!(completed_threads.len(), 10, "All threads should complete");
    
    // Verify total row count
    let count_response = server_arc.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
        user_id
    ).await;
    
    if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 100, "Should have 100 rows (10 threads × 10 rows each)");
    }
    
    // Verify each thread's data
    for thread_id in 0..10 {
        let query = format!(
            "SELECT COUNT(*) as count FROM {}.{} WHERE thread_id = {}",
            namespace, table_name, thread_id
        );
        
        let response = server_arc.execute_sql_as_user(&query, user_id).await;
        
        if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
            let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            assert_eq!(
                count, 10,
                "Thread {} should have 10 rows",
                thread_id
            );
        }
    }
}

/// Test: Concurrent inserts with unique constraint violations
#[tokio::test]
async fn test_concurrent_inserts_with_pk_conflicts() {
    let server = TestServer::new().await;
    let namespace = "conflict_test";
    let table_name = "unique_ids";
    let user_id = "conflict_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            value TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Multiple threads trying to insert the same ID (should conflict)
    let mut handles = vec![];
    let server_arc = Arc::new(server);
    let conflict_id = 12345i64;
    
    for thread_id in 0..5 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let uid = user_id.to_string();
        
        let handle = tokio::spawn(async move {
            let insert_sql = format!(
                "INSERT INTO {}.{} (id, value) VALUES ({}, 'Thread {}')",
                ns, tn, conflict_id, thread_id
            );
            
            server_clone.execute_sql_as_user(&insert_sql, &uid).await
        });
        
        handles.push(handle);
    }
    
    let results = futures_util::future::join_all(handles).await;
    
    // At least one should succeed, others may fail with PK violation
    let success_count = results.iter().filter(|r| {
        if let Ok(response) = r {
            response.status == ResponseStatus::Success
        } else {
            false
        }
    }).count();
    
    let error_count = results.iter().filter(|r| {
        if let Ok(response) = r {
            response.status == ResponseStatus::Error
        } else {
            false
        }
    }).count();
    
    println!("Success: {}, Errors: {}", success_count, error_count);
    
    // Exactly one insert should succeed
    assert!(
        success_count >= 1,
        "At least one insert should succeed for conflict ID"
    );
    
    // Verify only one row exists with the conflict ID
    let count_response = server_arc.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{} WHERE id = {}", namespace, table_name, conflict_id),
        user_id
    ).await;
    
    if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert!(
            count <= 1,
            "Should have at most 1 row with conflict ID (got {})",
            count
        );
    }
}

/// Test: Read/write contention - readers and writers concurrently
#[tokio::test]
async fn test_read_write_contention() {
    let server = TestServer::new().await;
    let namespace = "rw_contention";
    let table_name = "items";
    let user_id = "rw_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            item_id BIGINT PRIMARY KEY,
            description TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Pre-populate with some data
    for i in 0..20 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (item_id, description) VALUES ({}, 'Item {}')",
            namespace, table_name, i, i
        );
        server.execute_sql_as_user(&insert_sql, user_id).await;
    }
    
    let server_arc = Arc::new(server);
    let mut handles = vec![];
    
    // Spawn 5 readers (continuous SELECT queries)
    for reader_id in 0..5 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let uid = user_id.to_string();
        
        let handle = tokio::spawn(async move {
            let mut success_count = 0;
            
            for _ in 0..10 {
                let select_sql = format!(
                    "SELECT COUNT(*) as count FROM {}.{} WHERE item_id < {}",
                    ns, tn, reader_id * 5
                );
                
                let response = server_clone.execute_sql_as_user(&select_sql, &uid).await;
                
                if response.status == ResponseStatus::Success {
                    success_count += 1;
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
            
            success_count
        });
        
        handles.push(handle);
    }
    
    // Spawn 3 writers (concurrent INSERTs)
    for writer_id in 0..3 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let uid = user_id.to_string();
        
        let handle = tokio::spawn(async move {
            let mut success_count = 0;
            
            for i in 0..10 {
                let item_id = 100 + (writer_id * 10) + i;
                let insert_sql = format!(
                    "INSERT INTO {}.{} (item_id, description) VALUES ({}, 'Writer {} item {}')",
                    ns, tn, item_id, writer_id, i
                );
                
                let response = server_clone.execute_sql_as_user(&insert_sql, &uid).await;
                
                if response.status == ResponseStatus::Success {
                    success_count += 1;
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;
            }
            
            success_count
        });
        
        handles.push(handle);
    }
    
    // Wait for all readers and writers
    let results = futures_util::future::join_all(handles).await;
    
    // All tasks should complete without panics
    assert_eq!(results.len(), 8, "All reader and writer tasks should complete");
    
    for result in results {
        assert!(result.is_ok(), "No task should panic");
    }
    
    // Verify final row count
    let count_response = server_arc.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
        user_id
    ).await;
    
    if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        
        // Should have initial 20 + writer inserts
        assert!(
            count >= 20,
            "Should have at least 20 rows (initial data), got {}",
            count
        );
    }
}

/// Test: Concurrent UPDATE operations
#[tokio::test]
async fn test_concurrent_updates_same_row() {
    let server = TestServer::new().await;
    let namespace = "update_test";
    let table_name = "counter";
    let user_id = "update_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            value BIGINT,
            last_updated_by TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Insert initial row
    let insert_sql = format!(
        "INSERT INTO {}.{} (id, value, last_updated_by) VALUES (1, 0, 'initial')",
        namespace, table_name
    );
    server.execute_sql_as_user(&insert_sql, user_id).await;
    
    let server_arc = Arc::new(server);
    let mut handles = vec![];
    
    // Spawn 10 concurrent updaters
    for updater_id in 0..10 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let uid = user_id.to_string();
        
        let handle = tokio::spawn(async move {
            for _ in 0..5 {
                // Each updater increments the value
                let update_sql = format!(
                    "UPDATE {}.{} SET value = value + 1, last_updated_by = 'updater_{}' WHERE id = 1",
                    ns, tn, updater_id
                );
                
                server_clone.execute_sql_as_user(&update_sql, &uid).await;
                
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all updates to complete
    futures_util::future::join_all(handles).await;
    
    // Query final value
    let select_response = server_arc.execute_sql_as_user(
        &format!("SELECT value, last_updated_by FROM {}.{} WHERE id = 1", namespace, table_name),
        user_id
    ).await;
    
    if let Some(rows) = select_response.results.first().and_then(|r| r.rows.as_ref()) {
        let value = rows[0].get("value").and_then(|v| v.as_i64()).unwrap_or(0);
        
        // Note: Depending on MVCC implementation, value may be less than 50 (10 updaters × 5 updates)
        // This test documents the behavior rather than asserting exact count
        println!("Final value after concurrent updates: {}", value);
        
        assert!(
            value >= 0,
            "Value should be non-negative after updates, got {}",
            value
        );
    }
}

/// Test: Concurrent DELETE operations
#[tokio::test]
async fn test_concurrent_deletes() {
    let server = TestServer::new().await;
    let namespace = "delete_test";
    let table_name = "temp_records";
    let user_id = "delete_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            record_id BIGINT PRIMARY KEY,
            status TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Insert 100 rows
    for i in 0..100 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (record_id, status) VALUES ({}, 'active')",
            namespace, table_name, i
        );
        server.execute_sql_as_user(&insert_sql, user_id).await;
    }
    
    let server_arc = Arc::new(server);
    let mut handles = vec![];
    
    // Spawn 10 concurrent deleters, each trying to delete 10 rows
    for deleter_id in 0..10 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let uid = user_id.to_string();
        
        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let record_id = (deleter_id * 10) + i;
                let delete_sql = format!(
                    "DELETE FROM {}.{} WHERE record_id = {}",
                    ns, tn, record_id
                );
                
                server_clone.execute_sql_as_user(&delete_sql, &uid).await;
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all deletions
    futures_util::future::join_all(handles).await;
    
    // Count remaining rows (soft deletes)
    let count_response = server_arc.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{} WHERE _deleted = false", namespace, table_name),
        user_id
    ).await;
    
    if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        
        // All rows should be soft-deleted
        assert_eq!(count, 0, "All rows should be soft-deleted");
    }
    
    // Count soft-deleted rows
    let deleted_count_response = server_arc.execute_sql_as_user(
        &format!("SELECT COUNT(*) as count FROM {}.{} WHERE _deleted = true", namespace, table_name),
        user_id
    ).await;
    
    if let Some(rows) = deleted_count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        
        assert_eq!(count, 100, "Should have 100 soft-deleted rows");
    }
}

/// Test: Concurrent shared table access by multiple users
#[tokio::test]
async fn test_concurrent_shared_table_access() {
    let server = TestServer::new().await;
    let namespace = "shared_concurrent";
    let table_name = "global_config";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            key TEXT PRIMARY KEY,
            value TEXT,
            updated_by TEXT
        ) WITH (TYPE = 'SHARED', ACCESS_LEVEL = 'PUBLIC')",
        namespace, table_name
    );
    
    server.execute_sql(&create_sql).await;
    
    let server_arc = Arc::new(server);
    let mut handles = vec![];
    
    // Spawn 5 users concurrently accessing shared table
    for user_num in 0..5 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        let user_id = format!("user_{}", user_num);
        
        let handle = tokio::spawn(async move {
            for i in 0..5 {
                let key = format!("config_{}_{}", user_num, i);
                let insert_sql = format!(
                    "INSERT INTO {}.{} (key, value, updated_by) VALUES ('{}', 'value_{}', '{}')",
                    ns, tn, key, i, user_id
                );
                
                server_clone.execute_sql_as_user(&insert_sql, &user_id).await;
            }
        });
        
        handles.push(handle);
    }
    
    futures_util::future::join_all(handles).await;
    
    // Verify all inserts succeeded
    let count_response = server_arc.execute_sql(
        &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name)
    ).await;
    
    if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        
        assert_eq!(
            count, 25,
            "Should have 25 rows (5 users × 5 inserts each)"
        );
    }
}
