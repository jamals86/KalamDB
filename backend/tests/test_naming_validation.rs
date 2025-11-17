//! Integration tests for naming validation
//!
//! Tests that namespace, table, and column names are validated correctly
//! and that reserved words are properly rejected.

#[path = "integration/common/mod.rs"]
mod common;

use common::{fixtures, TestServer};

#[actix_web::test]
async fn test_reserved_namespace_names() {
    let server = TestServer::new().await;

    // Test reserved namespace names
    let reserved_names = vec!["system", "sys", "root", "kalamdb"];

    for name in reserved_names {
        let sql = format!("CREATE NAMESPACE {}", name);
        let response = server.execute_sql(&sql).await;
        
        assert_eq!(
            response.status, "error",
            "Should reject reserved namespace name '{}', but got status: {}",
            name, response.status
        );
        
        if let Some(error) = &response.error {
            let error_msg = error.message.to_lowercase();
            assert!(
                error_msg.contains("reserved") || error_msg.contains("cannot be used"),
                "Error message should mention 'reserved' for namespace '{}': {}",
                name,
                error.message
            );
        } else {
            panic!("Expected error for reserved namespace '{}'", name);
        }
    }
    // No namespaces were created; no cleanup needed
}

#[actix_web::test]
async fn test_valid_namespace_names() {
    let server = TestServer::new().await;

    let valid_names = vec!["validns1", "validns2", "validns3"];

    for name in valid_names {
        let sql = format!("CREATE NAMESPACE {}", name);
        let response = server.execute_sql(&sql).await;
        
        assert_eq!(
            response.status, "success",
            "Should accept valid namespace name '{}', but got error: {:?}",
            name,
            response.error
        );
        // Targeted cleanup to avoid cross-test interference
        let _ = server.cleanup_namespace(name).await;
    }
}

#[actix_web::test]
async fn test_reserved_column_names() {
    let server = TestServer::new().await;

    // Create a test namespace first
    fixtures::create_namespace(&server, "test_cols").await;

    // Test reserved column names
    let reserved_columns = vec!["_seq", "_deleted", "_row_id", "_id", "_updated"];

    for col_name in reserved_columns {
        let table_name = format!("test_{}", col_name.replace("_", ""));
        let sql = format!(
            "CREATE USER TABLE test_cols.{} ({} TEXT PRIMARY KEY)",
            table_name,
            col_name
        );
        let response = server.execute_sql(&sql).await;
        
        assert_eq!(
            response.status, "error",
            "Should reject reserved column name '{}', but got success",
            col_name
        );
    }
    // Targeted cleanup for this namespace
    let _ = server.cleanup_namespace("test_cols").await;
}

#[actix_web::test]
async fn test_valid_column_names() {
    let server = TestServer::new().await;

    // Create a test namespace first
    fixtures::create_namespace(&server, "test_valid_cols").await;

    let valid_columns = vec![
        ("user_id", "users"), 
        ("firstName", "profiles"), 
    ];

    for (col_name, table_name) in valid_columns {
        let sql = format!(
            "CREATE USER TABLE test_valid_cols.{} ({} TEXT PRIMARY KEY)",
            table_name,
            col_name
        );
        let response = server.execute_sql(&sql).await;
        
        assert_eq!(
            response.status, "success",
            "Should accept valid column name '{}', but got error: {:?}",
            col_name,
            response.error
        );
    }
    // Targeted cleanup for this namespace
    let _ = server.cleanup_namespace("test_valid_cols").await;
}

#[actix_web::test]
async fn test_no_auto_id_column_injection() {
    let server = TestServer::new().await;

    // Create a test namespace first
    fixtures::create_namespace(&server, "test_no_id").await;

    // Create a table with only user-defined columns
    let sql = "CREATE USER TABLE test_no_id.messages (message TEXT PRIMARY KEY, content TEXT)";
    let response = server.execute_sql(sql).await;
    assert_eq!(response.status, "success", "Should create table");

    // Insert a row to verify table works
    let insert_sql = "INSERT INTO test_no_id.messages (message, content) VALUES ('msg1', 'Hello')";
    let response = server.execute_sql_as_user(insert_sql, "user_001").await;
    assert_eq!(response.status, "success", "Should insert row");
    // Targeted cleanup for this namespace
    let _ = server.cleanup_namespace("test_no_id").await;
}

#[actix_web::test]
async fn test_user_can_use_id_as_column_name() {
    let server = TestServer::new().await;

    // Create a test namespace first
    fixtures::create_namespace(&server, "test_user_id").await;

    // Users should be able to define their own "id" column if they want
    let sql = "CREATE USER TABLE test_user_id.products (id TEXT PRIMARY KEY, name TEXT)";
    let response = server.execute_sql(sql).await;
    
    assert_eq!(
        response.status, "success",
        "Should allow user to define 'id' column themselves, but got error: {:?}",
        response.error
    );
    // Targeted cleanup for this namespace
    let _ = server.cleanup_namespace("test_user_id").await;
}
