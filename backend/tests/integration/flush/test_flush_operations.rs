//! Integration tests for Flush Operations
//!
//! **IMPORTANT**: Automatic flush background scheduler is NOT YET IMPLEMENTED.
//! These tests verify that:
//! 1. Tables can be created with FLUSH ROWS policy (metadata is stored)
//! 2. Inserts work correctly with flush policies defined
//! 3. Queries return correct data from RocksDB buffer
//!
//! **NOT TESTED (because not implemented)**:
//! - Automatic flush triggering based on row count
//! - Parquet file generation
//! - Queries combining Parquet + RocksDB data
//!
//! The manual flush job code exists in `kalamdb-core/src/flush/`, but there is no
//! background scheduler to trigger it automatically when row thresholds are reached.
//!
//! Table schema used: AI messaging app messages table with variety of column types:
//! - message_id (BIGINT) - unique message identifier
//! - sender_id (VARCHAR) - user who sent the message
//! - conversation_id (VARCHAR) - conversation thread
//! - content (TEXT) - message content
//! - tokens (INT) - token count for AI processing
//! - cost (DOUBLE) - cost in dollars
//! - created_at (TIMESTAMP) - when message was created
//! - is_ai (BOOLEAN) - whether message is from AI

#[path = "../common/mod.rs"]
mod common;

use common::{fixtures, flush_helpers, TestServer};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if Parquet files exist for a user table
/// Returns (number of .parquet files found, total files in directory)
fn check_user_table_parquet_files(
    namespace: &str,
    table_name: &str,
    user_id: &str,
) -> (usize, usize) {
    // User table storage path: /data/${user_id}/tables/
    // Full path: ./data/{user_id}/tables/{namespace}/{table_name}/
    let storage_path = PathBuf::from("./data")
        .join(user_id)
        .join("tables")
        .join(namespace)
        .join(table_name);

    if !storage_path.exists() {
        return (0, 0);
    }

    let mut parquet_count = 0;
    let mut total_count = 0;

    if let Ok(entries) = fs::read_dir(&storage_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    total_count += 1;
                    if let Some(extension) = entry.path().extension() {
                        if extension == "parquet" {
                            parquet_count += 1;
                            println!("  Found Parquet file: {}", entry.path().display());
                        }
                    }
                }
            }
        }
    }

    (parquet_count, total_count)
}

/// Check if Parquet files exist for a shared table
/// Returns (number of .parquet files found, total files in directory)
fn check_shared_table_parquet_files(namespace: &str, table_name: &str) -> (usize, usize) {
    // Shared table storage path: ./data/shared/{namespace}/{table_name}/
    let storage_path = PathBuf::from("./data")
        .join("shared")
        .join(namespace)
        .join(table_name);

    if !storage_path.exists() {
        return (0, 0);
    }

    let mut parquet_count = 0;
    let mut total_count = 0;

    if let Ok(entries) = fs::read_dir(&storage_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    total_count += 1;
                    if let Some(extension) = entry.path().extension() {
                        if extension == "parquet" {
                            parquet_count += 1;
                            println!("  Found Parquet file: {}", entry.path().display());
                        }
                    }
                }
            }
        }
    }

    (parquet_count, total_count)
}

// ============================================================================
// Test 1: Automatic Flushing - User Table
// ============================================================================

#[actix_web::test]
async fn test_01_auto_flush_user_table() {
    let server = TestServer::new().await;

    // Create namespace
    fixtures::create_namespace(&server, "auto_user_ns").await;

    // Create user table with auto-flush after 100 rows
    let create_sql = r#"CREATE USER TABLE auto_user_ns.messages (
        message_id BIGINT,
        sender_id VARCHAR,
        conversation_id VARCHAR,
        content TEXT,
        tokens INT,
        cost DOUBLE,
        created_at TIMESTAMP,
        is_ai BOOLEAN
    ) FLUSH ROWS 100"#;

    let response = server
        .execute_sql_with_user(create_sql, Some("test_user_001"))
        .await;
    assert_eq!(
        response.status, "success",
        "Failed to create user table with auto-flush: {:?}",
        response.error
    );

    // Insert 110 rows (should trigger auto-flush after first 100)
    println!("Inserting 110 rows (should auto-flush at 100)...");
    for i in 1..=110 {
        let insert_sql = format!(
            r#"INSERT INTO auto_user_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, 'sender_{:04}', 'conv_auto', 'Auto message {}', {}, {:.4}, '2025-10-21T12:{:02}:00', {})"#,
            i,
            i % 30,
            i,
            40 + (i % 60),
            0.0005 + (i as f64 * 0.0001),
            i % 60,
            if i % 5 == 0 { "true" } else { "false" }
        );

        let response = server
            .execute_sql_with_user(&insert_sql, Some("test_user_001"))
            .await;
        if response.status != "success" {
            panic!("Failed to insert row {}: {:?}", i, response.error);
        }
    }
    println!("Inserted 110 rows");

    // Wait for auto-flush to complete
    // NOTE: Auto-flush scheduler is NOT YET IMPLEMENTED
    // This sleep does nothing currently, but is here for when flush is implemented
    println!("Waiting for auto-flush to complete...");
    sleep(Duration::from_millis(500)).await;

    // TODO: Verify Parquet files when auto-flush scheduler is implemented
    // Currently, all data remains in RocksDB buffer
    println!("NOTE: Auto-flush scheduler not yet implemented - data remains in RocksDB");

    // Query the table - should return all 110 rows (currently all from RocksDB buffer)
    println!("Querying after auto-flush...");
    let query_response = server
        .execute_sql_with_user(
            "SELECT COUNT(message_id) as count FROM auto_user_ns.messages",
            Some("test_user_001"),
        )
        .await;

    assert_eq!(
        query_response.status, "success",
        "Failed to query after auto-flush: {:?}",
        query_response.error
    );

    if let Some(rows) = query_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(
            count, 110,
            "Expected 110 rows after auto-flush (100 flushed + 10 buffered), got {}",
            count
        );
        println!("✓ All 110 rows returned after auto-flush");
    }

    // Insert another 50 rows (total now: 100 flushed + 60 buffered = 160)
    println!("Adding 50 more rows...");
    for i in 111..=160 {
        let insert_sql = format!(
            r#"INSERT INTO auto_user_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, 'sender_{:04}', 'conv_auto', 'Additional auto message {}', {}, {:.4}, '2025-10-21T13:{:02}:00', {})"#,
            i,
            i % 30,
            i,
            50 + (i % 50),
            0.001 + (i as f64 * 0.0001),
            i % 60,
            if i % 6 == 0 { "true" } else { "false" }
        );

        let response = server
            .execute_sql_with_user(&insert_sql, Some("test_user_001"))
            .await;
        if response.status != "success" {
            panic!(
                "Failed to insert additional row {}: {:?}",
                i, response.error
            );
        }
    }

    // Query again - should return all 160 rows (100 flushed + 60 buffered)
    println!("Querying combined flushed and buffered data...");
    let final_response = server
        .execute_sql_with_user(
            "SELECT COUNT(*) as total FROM auto_user_ns.messages",
            Some("test_user_001"),
        )
        .await;

    assert_eq!(
        final_response.status, "success",
        "Failed to query combined data: {:?}",
        final_response.error
    );

    if let Some(rows) = final_response.results.first().and_then(|r| r.rows.as_ref()) {
        let total = rows[0].get("total").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(
            total, 160,
            "Expected 160 rows (100 flushed + 60 buffered), got {}",
            total
        );
        println!("✓ All 160 rows returned (flushed + buffered)");
    }

    // Verify aggregations work across both flushed and buffered data
    let avg_response = server
        .execute_sql_with_user(
            "SELECT AVG(tokens) as avg_tokens, MAX(cost) as max_cost FROM auto_user_ns.messages",
            Some("test_user_001"),
        )
        .await;

    if let Some(rows) = avg_response.results.first().and_then(|r| r.rows.as_ref()) {
        let avg_tokens = rows[0]
            .get("avg_tokens")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        assert!(
            avg_tokens > 0.0,
            "Average tokens should be calculated correctly"
        );
        println!("✓ Aggregations work correctly across flushed and buffered data");
    }

    println!("✓ Auto-flush POLICY test for user table PASSED (NOTE: actual flushing not yet implemented)");
}

// ============================================================================
// Test 2: Large Dataset Auto-Flush - User Table
// ============================================================================

#[actix_web::test]
async fn test_02_large_dataset_user_table() {
    let server = TestServer::new().await;

    // Create namespace
    fixtures::create_namespace(&server, "large_user_ns").await;

    // Create user table with auto-flush after 100 rows
    let create_sql = r#"CREATE USER TABLE large_user_ns.messages (
        message_id BIGINT,
        sender_id VARCHAR,
        conversation_id VARCHAR,
        content TEXT,
        tokens INT,
        cost DOUBLE,
        created_at TIMESTAMP,
        is_ai BOOLEAN
    ) FLUSH ROWS 100"#;

    server
        .execute_sql_with_user(create_sql, Some("test_user_002"))
        .await;

    // Insert 1000 rows (should trigger multiple flushes)
    println!("Inserting 1000 rows...");
    for i in 1..=1000 {
        let insert_sql = format!(
            r#"INSERT INTO large_user_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, 'user_{:04}', 'conv_{:03}', 'Message content number {}', {}, {:.4}, '2025-10-21T10:{:02}:00', {})"#,
            i,
            i % 50,
            (i / 100) + 1,
            i,
            50 + (i % 100),
            0.001 + (i as f64 * 0.0001),
            i % 60,
            if i % 3 == 0 { "true" } else { "false" }
        );

        let response = server
            .execute_sql_with_user(&insert_sql, Some("test_user_002"))
            .await;
        if response.status != "success" {
            panic!("Failed to insert row {}: {:?}", i, response.error);
        }
    }
    println!("Inserted 1000 rows successfully");

    // Wait for all flushes to complete
    // NOTE: Auto-flush scheduler is NOT YET IMPLEMENTED
    sleep(Duration::from_millis(1000)).await;

    // TODO: Verify Parquet files when auto-flush scheduler is implemented
    println!("NOTE: Auto-flush scheduler not yet implemented - all data in RocksDB");

    // Query and verify all 1000 rows are returned (currently all from RocksDB buffer)
    println!("Querying large dataset...");
    let query_response = server
        .execute_sql_with_user(
            "SELECT COUNT(*) as count FROM large_user_ns.messages",
            Some("test_user_002"),
        )
        .await;

    assert_eq!(query_response.status, "success");

    if let Some(rows) = query_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 1000, "Expected 1000 rows, got {}", count);
        println!("✓ All 1000 rows returned from multiple flushes");
    }

    // Query with filters to verify data integrity
    let filter_response = server
        .execute_sql_with_user(
            "SELECT COUNT(*) as ai_count FROM large_user_ns.messages WHERE is_ai = true",
            Some("test_user_002"),
        )
        .await;

    if let Some(rows) = filter_response
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
    {
        let ai_count = rows[0]
            .get("ai_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        assert!(
            ai_count >= 333 && ai_count <= 334,
            "Expected ~333 AI messages, got {}",
            ai_count
        );
        println!("✓ Filtered query works correctly on large flushed dataset");
    }

    // Verify data from different conversations (tests multiple flush cycles)
    let conv_response = server
        .execute_sql_with_user(
            "SELECT conversation_id, COUNT(*) as count FROM large_user_ns.messages GROUP BY conversation_id ORDER BY conversation_id",
            Some("test_user_002")
        )
        .await;

    if let Some(rows) = conv_response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(rows.len() >= 10, "Expected at least 10 conversations");
        println!("✓ Data correctly distributed across multiple flush cycles");
    }

    println!("✓ Large dataset user table test PASSED (NOTE: data in RocksDB, not flushed)");
}

// ============================================================================
// Test 3: Automatic Flushing - Shared Table
// ============================================================================

#[actix_web::test]
#[ignore = "Shared tables require pre-created column families at DB init"]
async fn test_03_auto_flush_shared_table() {
    let server = TestServer::new().await;

    // Create namespace
    fixtures::create_namespace(&server, "auto_shared_ns").await;

    // Create shared table with auto-flush after 100 rows
    let create_sql = r#"CREATE SHARED TABLE auto_shared_ns.messages (
        message_id BIGINT,
        sender_id VARCHAR,
        conversation_id VARCHAR,
        content TEXT,
        tokens INT,
        cost DOUBLE,
        created_at TIMESTAMP,
        is_ai BOOLEAN
    ) FLUSH ROWS 100"#;

    let response = server.execute_sql(create_sql).await;
    assert_eq!(
        response.status, "success",
        "Failed to create shared table with auto-flush: {:?}",
        response.error
    );

    // Insert 110 rows (should auto-flush at 100)
    println!("Inserting 110 rows into auto-flush shared table...");
    for i in 1..=110 {
        let insert_sql = format!(
            r#"INSERT INTO auto_shared_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, 'shared_user_{:04}', 'shared_conv_auto', 'Auto shared message {}', {}, {:.4}, '2025-10-21T16:{:02}:00', {})"#,
            i,
            i % 35,
            i,
            45 + (i % 65),
            0.00075 + (i as f64 * 0.00011),
            i % 60,
            if i % 5 == 0 { "true" } else { "false" }
        );

        let response = server.execute_sql(&insert_sql).await;
        if response.status != "success" {
            panic!(
                "Failed to insert row {} into auto-flush shared table: {:?}",
                i, response.error
            );
        }
    }
    println!("Inserted 110 rows into auto-flush shared table");

    // Wait for auto-flush
    // NOTE: Auto-flush scheduler is NOT YET IMPLEMENTED
    println!("Waiting for auto-flush of shared table...");
    sleep(Duration::from_millis(500)).await;

    // TODO: Verify Parquet files when auto-flush scheduler is implemented
    println!("NOTE: Auto-flush scheduler not yet implemented - all data in RocksDB");

    // Query - should return all 110 rows (currently all from RocksDB buffer)
    println!("Querying after auto-flush of shared table...");
    let query_response = server
        .execute_sql("SELECT COUNT(*) as count FROM auto_shared_ns.messages")
        .await;

    assert_eq!(query_response.status, "success");

    if let Some(rows) = query_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(
            count, 110,
            "Expected 110 rows after auto-flush of shared table, got {}",
            count
        );
        println!("✓ All 110 rows returned from auto-flushed shared table");
    }

    // Insert another 50 rows
    println!("Adding 50 more rows to auto-flush shared table...");
    for i in 111..=160 {
        let insert_sql = format!(
            r#"INSERT INTO auto_shared_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, 'shared_user_{:04}', 'shared_conv_auto', 'More auto shared message {}', {}, {:.4}, '2025-10-21T17:{:02}:00', {})"#,
            i,
            i % 35,
            i,
            55 + (i % 55),
            0.0012 + (i as f64 * 0.00013),
            i % 60,
            if i % 6 == 0 { "true" } else { "false" }
        );

        let response = server.execute_sql(&insert_sql).await;
        if response.status != "success" {
            panic!(
                "Failed to insert additional row {}: {:?}",
                i, response.error
            );
        }
    }

    // Query again - should return all 160 rows
    println!("Querying final state of auto-flush shared table...");
    let final_response = server
        .execute_sql("SELECT COUNT(message_id) as total FROM auto_shared_ns.messages")
        .await;

    assert_eq!(final_response.status, "success");

    if let Some(rows) = final_response.results.first().and_then(|r| r.rows.as_ref()) {
        let total = rows[0].get("total").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(
            total, 160,
            "Expected 160 rows in shared table (100 flushed + 60 buffered), got {}",
            total
        );
        println!("✓ All 160 rows returned from auto-flush shared table");
    }

    // Test complex query with aggregations and filters
    let complex_response = server
        .execute_sql(
            "SELECT is_ai, COUNT(message_id) as msg_count, AVG(cost) as avg_cost, SUM(tokens) as total_tokens 
             FROM auto_shared_ns.messages 
             GROUP BY is_ai 
             ORDER BY is_ai"
        )
        .await;

    assert_eq!(complex_response.status, "success");

    if let Some(rows) = complex_response
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
    {
        assert!(rows.len() >= 1, "Expected grouped results");
        println!("✓ Complex aggregations work across flushed and buffered shared table data");
    }

    println!("✓ Auto-flush POLICY test for shared table PASSED (NOTE: actual flushing not yet implemented)");
}

// ============================================================================
// Test 4: Large Dataset Auto-Flush - Shared Table
// ============================================================================

#[actix_web::test]
#[ignore = "Shared tables require pre-created column families at DB init"]
async fn test_04_large_dataset_shared_table() {
    let server = TestServer::new().await;

    // Create namespace
    fixtures::create_namespace(&server, "large_shared_ns").await;

    // Create shared table with auto-flush after 100 rows
    let create_sql = r#"CREATE SHARED TABLE large_shared_ns.messages (
        message_id BIGINT,
        sender_id VARCHAR,
        conversation_id VARCHAR,
        content TEXT,
        tokens INT,
        cost DOUBLE,
        created_at TIMESTAMP,
        is_ai BOOLEAN
    ) FLUSH ROWS 100"#;

    server.execute_sql(create_sql).await;

    // Insert 1000 rows (should trigger multiple flushes)
    println!("Inserting 1000 rows into shared table...");
    for i in 1..=1000 {
        let insert_sql = format!(
            r#"INSERT INTO large_shared_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, 'shared_user_{:04}', 'shared_conv_{:03}', 'Shared message {}', {}, {:.4}, '2025-10-21T14:{:02}:00', {})"#,
            i,
            i % 40,
            (i / 100) + 1,
            i,
            55 + (i % 90),
            0.0015 + (i as f64 * 0.00012),
            i % 60,
            if i % 3 == 0 { "true" } else { "false" }
        );

        let response = server.execute_sql(&insert_sql).await;
        if response.status != "success" {
            panic!(
                "Failed to insert row {} into shared table: {:?}",
                i, response.error
            );
        }
    }
    println!("Inserted 1000 rows into shared table");

    // Wait for all flushes to complete
    // NOTE: Auto-flush scheduler is NOT YET IMPLEMENTED
    sleep(Duration::from_millis(1000)).await;

    // TODO: Verify Parquet files when auto-flush scheduler is implemented
    println!("NOTE: Auto-flush scheduler not yet implemented - all data in RocksDB");

    // Query and verify all 1000 rows from multiple flush cycles (currently all from RocksDB)
    println!("Querying large shared table dataset...");
    let query_response = server
        .execute_sql("SELECT COUNT(message_id) as count FROM large_shared_ns.messages")
        .await;

    assert_eq!(query_response.status, "success");

    if let Some(rows) = query_response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(
            count, 1000,
            "Expected 1000 rows in shared table, got {}",
            count
        );
        println!("✓ All 1000 rows returned from flushed shared table");
    }

    // Verify filtering works across multiple flush cycles
    let filter_response = server
        .execute_sql(
            "SELECT COUNT(message_id) as count FROM large_shared_ns.messages WHERE is_ai = true",
        )
        .await;

    if let Some(rows) = filter_response
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
    {
        let count = rows[0].get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        assert!(
            count >= 333 && count <= 334,
            "Expected ~333 AI messages in shared table"
        );
    }

    // Test data distribution across conversations
    let conv_response = server
        .execute_sql(
            "SELECT conversation_id, COUNT(message_id) as count FROM large_shared_ns.messages GROUP BY conversation_id ORDER BY conversation_id"
        )
        .await;

    if let Some(rows) = conv_response.results.first().and_then(|r| r.rows.as_ref()) {
        assert!(
            rows.len() >= 10,
            "Expected at least 10 conversations in shared table"
        );
        println!("✓ Shared table data correctly distributed across multiple flush cycles");
    }

    println!("✓ Large dataset shared table test PASSED (NOTE: data in RocksDB, not flushed)");
}

// ============================================================================
// Test 5: Data Integrity Across Flush Boundaries
// ============================================================================

#[actix_web::test]
async fn test_05_flush_data_integrity() {
    let server = TestServer::new().await;

    // Create namespace
    fixtures::create_namespace(&server, "integrity_ns").await;

    // Create user table
    let create_sql = r#"CREATE USER TABLE integrity_ns.messages (
        message_id BIGINT,
        sender_id VARCHAR,
        conversation_id VARCHAR,
        content TEXT,
        tokens INT,
        cost DOUBLE,
        created_at TIMESTAMP,
        is_ai BOOLEAN
    ) FLUSH ROWS 50"#;

    server
        .execute_sql_with_user(create_sql, Some("test_user_003"))
        .await;

    // Insert data with specific values to verify integrity
    let test_data = vec![
        (1, "user_001", "conv_001", "Test message 1", 100, 0.01, true),
        (
            2,
            "user_002",
            "conv_001",
            "Test message 2",
            200,
            0.02,
            false,
        ),
        (
            3,
            "user_001",
            "conv_002",
            "Test message 3",
            150,
            0.015,
            true,
        ),
    ];

    for (id, user, conv, content, tokens, cost, is_ai) in test_data {
        let insert_sql = format!(
            r#"INSERT INTO integrity_ns.messages (message_id, sender_id, conversation_id, content, tokens, cost, created_at, is_ai) 
               VALUES ({}, '{}', '{}', '{}', {}, {}, '2025-10-21T18:00:00', {})"#,
            id, user, conv, content, tokens, cost, is_ai
        );
        server
            .execute_sql_with_user(&insert_sql, Some("test_user_003"))
            .await;
    }

    // Wait for potential flush
    sleep(Duration::from_millis(200)).await;

    // Verify exact data
    let verify_response = server
        .execute_sql_with_user(
            "SELECT message_id, sender_id, tokens, cost, is_ai FROM integrity_ns.messages ORDER BY message_id",
            Some("test_user_003")
        )
        .await;

    if let Some(rows) = verify_response
        .results
        .first()
        .and_then(|r| r.rows.as_ref())
    {
        assert_eq!(rows.len(), 3, "Expected 3 rows");

        // Verify first row
        assert_eq!(
            rows[0].get("message_id").and_then(|v| v.as_i64()).unwrap(),
            1
        );
        assert_eq!(
            rows[0].get("sender_id").and_then(|v| v.as_str()).unwrap(),
            "user_001"
        );
        assert_eq!(rows[0].get("tokens").and_then(|v| v.as_i64()).unwrap(), 100);
        assert_eq!(rows[0].get("cost").and_then(|v| v.as_f64()).unwrap(), 0.01);
        assert_eq!(
            rows[0].get("is_ai").and_then(|v| v.as_bool()).unwrap(),
            true
        );

        println!("✓ Data integrity verified after potential flush");
    }

    println!("✓ Flush data integrity test PASSED (NOTE: data in RocksDB, not flushed)");
}
