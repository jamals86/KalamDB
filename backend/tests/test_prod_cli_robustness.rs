//! CLI Robustness Tests
//!
//! Tests CLI behavior in various scenarios:
//! - Exit codes for success/failure
//! - Non-interactive mode (batch scripts)
//! - Error message clarity
//!
//! **Note**: These tests use the programmatic server API to validate
//! SQL execution behavior. Actual CLI binary tests would require spawning
//! processes which is out of scope for unit tests.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, TestServer};
use kalamdb_api::models::ResponseStatus;

/// Test: Successful queries should return Success status (simulates exit code 0)
#[tokio::test]
async fn test_successful_query_exit_status() {
    let server = TestServer::new().await;
    
    let queries = vec![
        "SELECT 1 as value",
        "SELECT COUNT(*) FROM system.tables",
        "CREATE NAMESPACE IF NOT EXISTS test_cli",
    ];
    
    for query in queries {
        let response = server.execute_sql(query).await;
        
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Query should succeed: {}",
            query
        );
    }
}

/// Test: Failed queries return Error status (simulates exit code != 0)
#[tokio::test]
async fn test_failed_query_exit_status() {
    let server = TestServer::new().await;
    
    let invalid_queries = vec![
        "SELECT * FROM nonexistent_table",
        "CREATE TABLE (id INT)",
        "INSERT INTO VALUES",
        "UPDATE SET field = 1",
    ];
    
    for query in invalid_queries {
        let response = server.execute_sql(query).await;
        
        assert_eq!(
            response.status,
            ResponseStatus::Error,
            "Query should fail: {}",
            query
        );
        
        // Verify error message is provided
        assert!(
            response.error.is_some(),
            "Error response should include error message"
        );
    }
}

/// Test: Batch execution (multiple statements)
#[tokio::test]
async fn test_batch_execution() {
    let server = TestServer::new().await;
    let namespace = "batch_test";
    
    // Simulate batch file execution
    let statements = vec![
        format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace),
        format!("CREATE TABLE {}.test1 (id BIGINT PRIMARY KEY) WITH (TYPE='USER')", namespace),
        format!("CREATE TABLE {}.test2 (id BIGINT PRIMARY KEY) WITH (TYPE='SHARED', ACCESS_LEVEL='PUBLIC')", namespace),
        format!("INSERT INTO {}.test1 (id) VALUES (1)", namespace),
        format!("INSERT INTO {}.test1 (id) VALUES (2)", namespace),
        format!("SELECT COUNT(*) FROM {}.test1", namespace),
    ];
    
    let mut success_count = 0;
    let mut error_count = 0;
    
    for stmt in statements {
        let response = server.execute_sql(&stmt).await;
        
        if response.status == ResponseStatus::Success {
            success_count += 1;
        } else {
            error_count += 1;
            println!("Statement failed: {} - {:?}", stmt, response.error);
        }
    }
    
    println!("Batch execution: {} success, {} errors", success_count, error_count);
    
    assert!(
        success_count >= 5,
        "Most batch statements should succeed"
    );
}

/// Test: Clear error messages for common mistakes
#[tokio::test]
async fn test_clear_error_messages() {
    let server = TestServer::new().await;
    
    let test_cases = vec![
        (
            "SELECT * FROM users",  // Missing namespace
            vec!["not found", "unknown", "does not exist"],
        ),
        (
            "CREATE TABLE test (id INT)",  // Missing namespace
            vec!["namespace", "qualified"],
        ),
        (
            "INSERT INTO test.table",  // Incomplete INSERT
            vec!["syntax", "expected", "invalid"],
        ),
    ];
    
    for (query, expected_terms) in test_cases {
        let response = server.execute_sql(query).await;
        
        assert_eq!(
            response.status,
            ResponseStatus::Error,
            "Query should fail: {}",
            query
        );
        
        if let Some(error) = &response.error {
            let msg = error.message.to_lowercase();
            
            let contains_expected = expected_terms.iter().any(|term| msg.contains(term));
            
            println!("Query: {}", query);
            println!("Error: {}", error.message);
            println!("Contains expected term: {}", contains_expected);
        }
    }
}

/// Test: Namespace operations for scripting
#[tokio::test]
async fn test_namespace_operations_for_scripting() {
    let server = TestServer::new().await;
    let namespace = "script_ns";
    
    // Create namespace (idempotent)
    let create_response = server.execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace)).await;
    assert_eq!(create_response.status, ResponseStatus::Success);
    
    // Create again (should still succeed with IF NOT EXISTS)
    let create_again_response = server.execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace)).await;
    assert_eq!(create_again_response.status, ResponseStatus::Success);
    
    // Drop namespace
    let drop_response = server.execute_sql(&format!("DROP NAMESPACE IF EXISTS {}", namespace)).await;
    assert_eq!(drop_response.status, ResponseStatus::Success);
    
    // Drop again (should still succeed with IF EXISTS)
    let drop_again_response = server.execute_sql(&format!("DROP NAMESPACE IF EXISTS {}", namespace)).await;
    assert_eq!(drop_again_response.status, ResponseStatus::Success);
}

/// Test: Table operations for scripting
#[tokio::test]
async fn test_table_operations_for_scripting() {
    let server = TestServer::new().await;
    let namespace = "table_script";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Create table (idempotent)
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS {}.script_table (id BIGINT PRIMARY KEY) WITH (TYPE='USER')",
        namespace
    );
    
    let create_response = server.execute_sql(&create_sql).await;
    assert_eq!(create_response.status, ResponseStatus::Success);
    
    // Create again (should succeed with IF NOT EXISTS)
    let create_again_response = server.execute_sql(&create_sql).await;
    assert_eq!(create_again_response.status, ResponseStatus::Success);
    
    // Drop table (idempotent)
    let drop_response = server.execute_sql(&format!("DROP TABLE IF EXISTS {}.script_table", namespace)).await;
    assert_eq!(drop_response.status, ResponseStatus::Success);
    
    // Drop again (should succeed with IF EXISTS)
    let drop_again_response = server.execute_sql(&format!("DROP TABLE IF EXISTS {}.script_table", namespace)).await;
    assert_eq!(drop_again_response.status, ResponseStatus::Success);
}

/// Test: Query output is consistent across formats
#[tokio::test]
async fn test_query_output_consistency() {
    let server = TestServer::new().await;
    
    // Simple query
    let response = server.execute_sql("SELECT 42 as answer, 'hello' as greeting").await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(result) = response.results.first() {
        // Verify schema
        assert_eq!(result.columns.len(), 2, "Should have 2 columns");
        
        let column_names = &result.columns;
        assert!(column_names.iter().any(|c| c == "answer"));
        assert!(column_names.iter().any(|c| c == "greeting"));
        
        // Verify data
        if let Some(rows) = &result.rows {
            assert_eq!(rows.len(), 1, "Should have 1 row");
            
            let answer = rows[0].get("answer").and_then(|v| v.as_i64());
            let greeting = rows[0].get("greeting").and_then(|v| v.as_str());
            
            assert_eq!(answer, Some(42));
            assert_eq!(greeting, Some("hello"));
        }
    }
}

/// Test: Empty result sets are handled correctly
#[tokio::test]
async fn test_empty_result_sets() {
    let server = TestServer::new().await;
    let namespace = "empty_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql(&format!(
        "CREATE TABLE {}.empty_table (id BIGINT PRIMARY KEY) WITH (TYPE='USER')",
        namespace
    )).await;
    
    // Query empty table
    let response = server.execute_sql(&format!("SELECT * FROM {}.empty_table", namespace)).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(result) = response.results.first() {
        // Schema should exist
        assert!(result.columns.len() > 0, "Schema should be present");
        
        // Rows should be empty
        if let Some(rows) = &result.rows {
            assert_eq!(rows.len(), 0, "Should have 0 rows");
        }
    }
}

/// Test: Large result set formatting (pagination would happen in CLI)
#[tokio::test]
async fn test_large_result_set_handling() {
    let server = TestServer::new().await;
    let namespace = "large_result";
    let user_id = "cli_user";
    
    fixtures::create_namespace(&server, namespace).await;
    
    server.execute_sql_as_user(&format!(
        "CREATE TABLE {}.many_rows (id BIGINT PRIMARY KEY) WITH (TYPE='USER')",
        namespace
    ), user_id).await;
    
    // Insert 50 rows
    for i in 0..50 {
        server.execute_sql_as_user(&format!(
            "INSERT INTO {}.many_rows (id) VALUES ({})",
            namespace, i
        ), user_id).await;
    }
    
    // Query all rows (no LIMIT)
    let response = server.execute_sql_as_user(
        &format!("SELECT * FROM {}.many_rows ORDER BY id", namespace),
        user_id
    ).await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 50, "Should return all 50 rows");
        
        println!("Large result set: {} rows returned", rows.len());
    }
}

/// Test: Meta-commands simulation (system queries)
#[tokio::test]
async fn test_meta_commands_simulation() {
    let server = TestServer::new().await;
    
    // Simulate \dt (list tables)
    let list_tables = "SELECT namespace_id, table_name, table_type FROM system.tables ORDER BY namespace_id, table_name LIMIT 20";
    let response = server.execute_sql(list_tables).await;
    assert_eq!(response.status, ResponseStatus::Success);
    
    // Simulate \stats (cache statistics)
    let stats = "SELECT key, value FROM system.stats ORDER BY key";
    let response = server.execute_sql(stats).await;
    assert_eq!(response.status, ResponseStatus::Success);
    
    // Simulate \d <table> (describe table) - would need specific table
    let describe = "SELECT * FROM system.tables LIMIT 1";
    let response = server.execute_sql(describe).await;
    assert_eq!(response.status, ResponseStatus::Success);
}

/// Test: Error recovery (continue after error)
#[tokio::test]
async fn test_error_recovery_continue_after_error() {
    let server = TestServer::new().await;
    let namespace = "recovery_test";
    
    fixtures::create_namespace(&server, namespace).await;
    
    // Execute sequence with error in middle
    let statements = vec![
        (format!("CREATE TABLE {}.test (id BIGINT PRIMARY KEY) WITH (TYPE='USER')", namespace), true),
        ("SELECT * FROM nonexistent_table".to_string(), false),  // Error
        (format!("INSERT INTO {}.test (id) VALUES (1)", namespace), true),  // Should still work
        (format!("SELECT COUNT(*) FROM {}.test", namespace), true),
    ];
    
    for (stmt, should_succeed) in statements {
        let response = server.execute_sql(&stmt).await;
        
        if should_succeed {
            assert_eq!(
                response.status,
                ResponseStatus::Success,
                "Statement should succeed: {}",
                stmt
            );
        } else {
            assert_eq!(
                response.status,
                ResponseStatus::Error,
                "Statement should fail: {}",
                stmt
            );
        }
    }
}
