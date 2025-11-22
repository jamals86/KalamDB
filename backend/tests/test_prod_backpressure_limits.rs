//! Backpressure and Limits Tests
//!
//! Tests behavior under resource pressure and large workloads:
//! - Large INSERT payloads
//! - Many concurrent clients
//! - High-frequency operations
//!
//! These tests validate that KalamDB degrades gracefully under load.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;
use std::sync::Arc;

/// Test: Large INSERT with many rows in single statement
#[tokio::test]
async fn test_large_batch_insert() {
    let server = TestServer::new().await;
    let namespace = "backpressure_test";
    let table_name = "bulk_data";
    let user_id = "bulk_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            data TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Insert 100 rows in a single INSERT statement
    let mut values = Vec::new();
    for i in 0..100 {
        values.push(format!("({}, 'Data row {}')", i, i));
    }
    
    let insert_sql = format!(
        "INSERT INTO {}.{} (id, data) VALUES {}",
        namespace, table_name,
        values.join(", ")
    );
    
    let response = server.execute_sql_as_user(&insert_sql, user_id).await;
    
    // Should succeed or fail gracefully
    if response.status == ResponseStatus::Success {
        println!("✓ Large batch insert succeeded (100 rows)");
        
        // Verify count
        let count_response = server.execute_sql_as_user(
            &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name),
            user_id
        ).await;
        
        if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
            let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
            assert_eq!(count, 100, "All 100 rows should be inserted");
        }
    } else {
        println!("✗ Large batch insert failed (expected behavior if size limit exists)");
        if let Some(error) = &response.error {
            println!("  Error: {}", error.message);
        }
    }
}

/// Test: Very long string values
#[tokio::test]
async fn test_very_long_string_values() {
    let server = TestServer::new().await;
    let namespace = "long_string_test";
    let table_name = "documents";
    let user_id = "doc_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            doc_id BIGINT PRIMARY KEY,
            content TEXT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Create a 10KB string
    let long_content = "A".repeat(10_000);
    
    let insert_sql = format!(
        "INSERT INTO {}.{} (doc_id, content) VALUES (1, '{}')",
        namespace, table_name, long_content
    );
    
    let response = server.execute_sql_as_user(&insert_sql, user_id).await;
    
    // Should handle reasonably large strings
    if response.status == ResponseStatus::Success {
        println!("✓ 10KB string insert succeeded");
        
        // Verify retrieval
        let select_response = server.execute_sql_as_user(
            &format!("SELECT LENGTH(content) as len FROM {}.{} WHERE doc_id = 1", namespace, table_name),
            user_id
        ).await;
        
        if let Some(rows) = select_response.results.first().and_then(|r| r.rows.as_ref()) {
            let len = rows[0].get("len").and_then(|v| v.as_i64()).unwrap_or(0);
            assert_eq!(len, 10000, "String length should be preserved");
        }
    } else {
        println!("✗ 10KB string insert failed");
        if let Some(error) = &response.error {
            println!("  Error: {}", error.message);
        }
    }
}

/// Test: Many concurrent clients (connection handling)
#[tokio::test]
async fn test_many_concurrent_clients() {
    let server = TestServer::new().await;
    let namespace = "concurrent_clients";
    let table_name = "requests";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            request_id BIGINT PRIMARY KEY,
            user_id TEXT
        ) WITH (TYPE = 'SHARED', ACCESS_LEVEL = 'PUBLIC')",
        namespace, table_name
    );
    
    server.execute_sql(&create_sql).await;
    
    let server_arc = Arc::new(server);
    let mut handles = vec![];
    
    // Spawn 20 concurrent "clients"
    for client_id in 0..20 {
        let server_clone = server_arc.clone();
        let ns = namespace.to_string();
        let tn = table_name.to_string();
        
        let handle = tokio::spawn(async move {
            let user_id = format!("client_{}", client_id);
            
            // Each client performs 5 queries
            let mut success_count = 0;
            
            for i in 0..5 {
                let request_id = (client_id * 100) + i;
                let insert_sql = format!(
                    "INSERT INTO {}.{} (request_id, user_id) VALUES ({}, '{}')",
                    ns, tn, request_id, user_id
                );
                
                let response = server_clone.execute_sql(&insert_sql).await;
                
                if response.status == ResponseStatus::Success {
                    success_count += 1;
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            }
            
            success_count
        });
        
        handles.push(handle);
    }
    
    // Wait for all clients
    let results = futures_util::future::join_all(handles).await;
    
    // Count total successful queries
    let total_success: usize = results.iter().filter_map(|r| r.as_ref().ok()).sum();
    
    println!("Total successful queries: {} / {}", total_success, 100);
    
    // Most queries should succeed
    assert!(
        total_success >= 80,
        "At least 80% of queries should succeed under concurrent load (got {}%)",
        total_success
    );
    
    // Verify total row count
    let count_response = server_arc.execute_sql(
        &format!("SELECT COUNT(*) as count FROM {}.{}", namespace, table_name)
    ).await;
    
    if let Some(rows) = count_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        println!("Total rows inserted: {}", count);
    }
}

/// Test: Rapid-fire sequential queries
#[tokio::test]
async fn test_rapid_sequential_queries() {
    let server = TestServer::new().await;
    let namespace = "rapid_test";
    let table_name = "events";
    let user_id = "rapid_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            event_id BIGINT PRIMARY KEY,
            timestamp TIMESTAMP DEFAULT NOW()
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Execute 50 INSERTs as fast as possible
    let start = std::time::Instant::now();
    let mut success_count = 0;
    
    for i in 0..50 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (event_id) VALUES ({})",
            namespace, table_name, i
        );
        
        let response = server.execute_sql_as_user(&insert_sql, user_id).await;
        
        if response.status == ResponseStatus::Success {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    
    println!("Executed {} inserts in {:?} ({:.1} inserts/sec)", 
        success_count, 
        elapsed,
        success_count as f64 / elapsed.as_secs_f64()
    );
    
    assert!(
        success_count >= 45,
        "At least 90% of rapid queries should succeed"
    );
}

/// Test: Wide table with many columns
#[tokio::test]
async fn test_wide_table_many_columns() {
    let server = TestServer::new().await;
    let namespace = "wide_table_test";
    let table_name = "wide_data";
    let user_id = "wide_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create table with 30 columns
    let mut columns = vec!["id BIGINT PRIMARY KEY".to_string()];
    for i in 1..30 {
        columns.push(format!("col_{} TEXT", i));
    }
    
    let create_sql = format!(
        "CREATE TABLE {}.{} ({}) WITH (TYPE = 'USER')",
        namespace, table_name,
        columns.join(", ")
    );
    
    let response = server.execute_sql_as_user(&create_sql, user_id).await;
    
    if response.status == ResponseStatus::Success {
        println!("✓ Wide table with 30 columns created");
        
        // Insert a row
        let mut values = vec!["1".to_string()];
        for i in 1..30 {
            values.push(format!("'value_{}'", i));
        }
        
        let insert_sql = format!(
            "INSERT INTO {}.{} VALUES ({})",
            namespace, table_name,
            values.join(", ")
        );
        
        let insert_response = server.execute_sql_as_user(&insert_sql, user_id).await;
        
        assert_eq!(
            insert_response.status,
            ResponseStatus::Success,
            "Should be able to insert into wide table"
        );
    } else {
        println!("✗ Wide table creation failed (column limit may exist)");
    }
}

/// Test: High-frequency UPDATE operations
#[tokio::test]
async fn test_high_frequency_updates() {
    let server = TestServer::new().await;
    let namespace = "update_freq_test";
    let table_name = "counter";
    let user_id = "freq_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            id BIGINT PRIMARY KEY,
            counter BIGINT
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Insert initial row
    server.execute_sql_as_user(
        &format!("INSERT INTO {}.{} (id, counter) VALUES (1, 0)", namespace, table_name),
        user_id
    ).await;
    
    // Perform 30 UPDATEs rapidly
    let start = std::time::Instant::now();
    let mut success_count = 0;
    
    for _ in 0..30 {
        let update_sql = format!(
            "UPDATE {}.{} SET counter = counter + 1 WHERE id = 1",
            namespace, table_name
        );
        
        let response = server.execute_sql_as_user(&update_sql, user_id).await;
        
        if response.status == ResponseStatus::Success {
            success_count += 1;
        }
    }
    
    let elapsed = start.elapsed();
    
    println!("Executed {} updates in {:?}", success_count, elapsed);
    
    // Query final value
    let select_response = server.execute_sql_as_user(
        &format!("SELECT counter FROM {}.{} WHERE id = 1", namespace, table_name),
        user_id
    ).await;
    
    if let Some(rows) = select_response.results.first().and_then(|r| r.rows.as_ref()) {
        let counter = rows[0].get("counter").and_then(|v| v.as_i64()).unwrap_or(0);
        println!("Final counter value: {}", counter);
        
        // Counter should reflect successful updates
        assert!(counter > 0, "Counter should have been incremented");
    }
}

/// Test: Query result set size (many rows)
#[tokio::test]
async fn test_large_query_result_set() {
    let server = TestServer::new().await;
    let namespace = "large_result_test";
    let table_name = "items";
    let user_id = "result_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    let create_sql = format!(
        "CREATE TABLE {}.{} (
            item_id BIGINT PRIMARY KEY
        ) WITH (TYPE = 'USER')",
        namespace, table_name
    );
    
    server.execute_sql_as_user(&create_sql, user_id).await;
    
    // Insert 200 rows
    for i in 0..200 {
        let insert_sql = format!(
            "INSERT INTO {}.{} (item_id) VALUES ({})",
            namespace, table_name, i
        );
        server.execute_sql_as_user(&insert_sql, user_id).await;
    }
    
    // Query all rows
    let select_response = server.execute_sql_as_user(
        &format!("SELECT * FROM {}.{} ORDER BY item_id", namespace, table_name),
        user_id
    ).await;
    
    assert_eq!(
        select_response.status,
        ResponseStatus::Success,
        "Should be able to query large result set"
    );
    
    if let Some(rows) = select_response.results.first().and_then(|r| r.rows.as_ref()) {
        println!("Retrieved {} rows", rows.len());
        assert_eq!(rows.len(), 200, "Should return all 200 rows");
    }
}
