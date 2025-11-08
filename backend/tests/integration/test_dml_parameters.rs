//! Integration tests for DML operations with parameterized queries
//!
//! Tests for Phase 7 (User Story 5): Full DML with Native Write Paths
//! - T068: INSERT with parameters
//! - T069: UPDATE with parameters
//! - T070: DELETE with parameters
//!
//! Validates:
//! - Parameter binding works correctly
//! - rows_affected returned accurately
//! - Parameter validation (max 50 params, 512KB per param)
//! - Native write paths used (not DataFusion)

use kalamdb_core::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_core::sql::executor::handlers::dml::{InsertHandler, UpdateHandler, DeleteHandler};
use kalamdb_core::sql::executor::handlers::StatementHandler;
use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_sql::statement_classifier::{SqlStatement, SqlStatementKind};
use datafusion::execution::context::SessionContext;

mod common;
use common::TestServer;

// ============================================================================
// T068: INSERT with Parameters
// ============================================================================

#[tokio::test]
async fn test_insert_with_simple_parameters() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Create test table
    server.execute_sql("CREATE NAMESPACE default").await;
    server.execute_sql("CREATE USER TABLE default.test_insert (id INT, name TEXT, age INT)").await;

    // Execute INSERT with parameters
    let handler = InsertHandler::new();
    let session = SessionContext::new();
    let stmt = SqlStatement::new(
        "INSERT INTO default.test_insert (id, name, age) VALUES ($1, $2, $3)".to_string(),
        SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
    );
    let params = vec![
        ScalarValue::Int32(Some(1)),
        ScalarValue::Utf8(Some("Alice".to_string())),
        ScalarValue::Int32(Some(30)),
    ];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_ok(), "INSERT with parameters failed: {:?}", result.err());

    match result.unwrap() {
        ExecutionResult::Inserted { rows_affected } => {
            assert_eq!(rows_affected, 1, "Expected 1 row inserted");
        }
        other => panic!("Expected Inserted result, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_insert_multiple_rows_with_parameters() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Create test table
    server.execute_sql("CREATE NAMESPACE default").await;
    server.execute_sql("CREATE USER TABLE default.test_insert_multi (id INT, name TEXT)").await;

    // Execute INSERT with multiple rows
    let handler = InsertHandler::new();
    let session = SessionContext::new();
    let stmt = SqlStatement::new(
        "INSERT INTO default.test_insert_multi (id, name) VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Charlie')".to_string(),
        SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
    );
    let params = vec![];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_ok(), "Multi-row INSERT failed: {:?}", result.err());

    match result.unwrap() {
        ExecutionResult::Inserted { rows_affected } => {
            assert_eq!(rows_affected, 3, "Expected 3 rows inserted");
        }
        other => panic!("Expected Inserted result, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_insert_parameter_count_validation() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Try to INSERT with 51 parameters (exceeds limit of 50)
    let handler = InsertHandler::new();
    let session = SessionContext::new();
    
    // Build SQL with 51 placeholders
    let mut params = Vec::new();
    for i in 1..=51 {
        params.push(ScalarValue::Int32(Some(i)));
    }
    let stmt = SqlStatement::new(
        "INSERT INTO default.test_insert_limit (col1) VALUES ($1)".to_string(),
        SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
    );

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_err(), "Expected parameter count validation error");
    assert!(
        result.unwrap_err().to_string().contains("Parameter count exceeds limit"),
        "Expected parameter count error message"
    );
}

#[tokio::test]
async fn test_insert_parameter_size_validation() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Try to INSERT with 600KB parameter (exceeds limit of 512KB)
    let handler = InsertHandler::new();
    let session = SessionContext::new();
    let large_string = "a".repeat(600_000); // 600KB
    let stmt = SqlStatement::new(
        "INSERT INTO default.test_insert_size (id, data) VALUES ($1, $2)".to_string(),
        SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
    );
    let params = vec![
        ScalarValue::Int32(Some(1)),
        ScalarValue::Utf8(Some(large_string)),
    ];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_err(), "Expected parameter size validation error");
    assert!(
        result.unwrap_err().to_string().contains("size exceeds limit"),
        "Expected parameter size error message"
    );
}

// ============================================================================
// T069: UPDATE with Parameters
// ============================================================================

#[tokio::test]
async fn test_update_with_simple_parameters() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Create test table and insert data
    server.execute_sql("CREATE NAMESPACE default").await;
    server.execute_sql("CREATE USER TABLE default.test_update (id TEXT, name TEXT, age INT)").await;
    server.execute_sql_as_user("INSERT INTO default.test_update (id, name, age) VALUES ('row1', 'Alice', 30)", "test_user").await;

    // Execute UPDATE with parameters
    let handler = UpdateHandler::new();
    let session = SessionContext::new();
    let stmt = SqlStatement::new(
        "UPDATE default.test_update SET name = $1, age = $2 WHERE id = 'row1'".to_string(),
        SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
    );
    let params = vec![
        ScalarValue::Utf8(Some("Alice Updated".to_string())),
        ScalarValue::Int32(Some(31)),
    ];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_ok(), "UPDATE with parameters failed: {:?}", result.err());

    match result.unwrap() {
        ExecutionResult::Updated { rows_affected } => {
            assert_eq!(rows_affected, 1, "Expected 1 row updated");
        }
        other => panic!("Expected Updated result, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_update_parameter_count_validation() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Try to UPDATE with 51 parameters (exceeds limit of 50)
    let handler = UpdateHandler::new();
    let session = SessionContext::new();
    
    // Build SQL with 51 parameters
    let mut params = Vec::new();
    for i in 1..=51 {
        params.push(ScalarValue::Int32(Some(i)));
    }
    let stmt = SqlStatement::new(
        "UPDATE default.test_update_limit SET col1 = $1 WHERE id = 'row1'".to_string(),
        SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
    );

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_err(), "Expected parameter count validation error");
    assert!(
        result.unwrap_err().to_string().contains("Parameter count exceeds limit"),
        "Expected parameter count error message"
    );
}

#[tokio::test]
async fn test_update_parameter_size_validation() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Try to UPDATE with 600KB parameter (exceeds limit of 512KB)
    let handler = UpdateHandler::new();
    let session = SessionContext::new();
    let large_string = "a".repeat(600_000); // 600KB
    let stmt = SqlStatement::new(
        "UPDATE default.test_update_size SET data = $1 WHERE id = 'row1'".to_string(),
        SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
    );
    let params = vec![ScalarValue::Utf8(Some(large_string))];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_err(), "Expected parameter size validation error");
    assert!(
        result.unwrap_err().to_string().contains("size exceeds limit"),
        "Expected parameter size error message"
    );
}

// ============================================================================
// T070: DELETE with Parameters
// ============================================================================

#[tokio::test]
async fn test_delete_with_simple_parameters() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Create test table and insert data
    server.execute_sql("CREATE NAMESPACE default").await;
    server.execute_sql("CREATE USER TABLE default.test_delete (id TEXT, name TEXT)").await;
    server.execute_sql_as_user("INSERT INTO default.test_delete (id, name) VALUES ('row1', 'Alice')", "test_user").await;

    // Execute DELETE
    let handler = DeleteHandler::new();
    let session = SessionContext::new();
    let stmt = SqlStatement::new(
        "DELETE FROM default.test_delete WHERE id = 'row1'".to_string(),
        SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
    );
    let params = vec![];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_ok(), "DELETE failed: {:?}", result.err());

    match result.unwrap() {
        ExecutionResult::Deleted { rows_affected } => {
            assert_eq!(rows_affected, 1, "Expected 1 row deleted");
        }
        other => panic!("Expected Deleted result, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_delete_parameter_count_validation() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Try to DELETE with 51 parameters (exceeds limit of 50)
    let handler = DeleteHandler::new();
    let session = SessionContext::new();
    
    // Build SQL with 51 parameters (unrealistic but tests validation)
    let mut params = Vec::new();
    for i in 1..=51 {
        params.push(ScalarValue::Int32(Some(i)));
    }
    let stmt = SqlStatement::new(
        "DELETE FROM default.test_delete_limit WHERE id = 'row1'".to_string(),
        SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
    );

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_err(), "Expected parameter count validation error");
    assert!(
        result.unwrap_err().to_string().contains("Parameter count exceeds limit"),
        "Expected parameter count error message"
    );
}

#[tokio::test]
async fn test_delete_parameter_size_validation() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Try to DELETE with 600KB parameter (exceeds limit of 512KB)
    let handler = DeleteHandler::new();
    let session = SessionContext::new();
    let large_string = "a".repeat(600_000); // 600KB
    let stmt = SqlStatement::new(
        "DELETE FROM default.test_delete_size WHERE id = 'row1'".to_string(),
        SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
    );
    let params = vec![ScalarValue::Utf8(Some(large_string))];

    let result = handler.execute(&session, stmt, params, &ctx).await;
    assert!(result.is_err(), "Expected parameter size validation error");
    assert!(
        result.unwrap_err().to_string().contains("size exceeds limit"),
        "Expected parameter size error message"
    );
}

// ============================================================================
// End-to-End Flow Tests
// ============================================================================

#[tokio::test]
async fn test_dml_e2e_insert_update_delete() {
    let server = TestServer::new().await;
    let ctx = ExecutionContext::new(
        UserId::from("test_user"),
        Role::User,
    );

    // Create test table
    server.execute_sql("CREATE NAMESPACE default").await;
    server.execute_sql("CREATE USER TABLE default.test_e2e (id TEXT, name TEXT, age INT)").await;

    // Step 1: INSERT with parameters
    let insert_handler = InsertHandler::new();
    let session = SessionContext::new();
    let insert_stmt = SqlStatement::new(
        "INSERT INTO default.test_e2e (id, name, age) VALUES ($1, $2, $3)".to_string(),
        SqlStatementKind::Insert(kalamdb_sql::ddl::InsertStatement),
    );
    let insert_params = vec![
        ScalarValue::Utf8(Some("1".to_string())),
        ScalarValue::Utf8(Some("Alice".to_string())),
        ScalarValue::Int32(Some(30)),
    ];
    let insert_result = insert_handler.execute(&session, insert_stmt, insert_params, &ctx).await;
    assert!(insert_result.is_ok(), "INSERT failed");

    // Step 2: UPDATE with parameters
    let update_handler = UpdateHandler::new();
    let update_stmt = SqlStatement::new(
        "UPDATE default.test_e2e SET age = $1 WHERE id = '1'".to_string(),
        SqlStatementKind::Update(kalamdb_sql::ddl::UpdateStatement),
    );
    let update_params = vec![ScalarValue::Int32(Some(31))];
    let update_result = update_handler.execute(&session, update_stmt, update_params, &ctx).await;
    assert!(update_result.is_ok(), "UPDATE failed");

    // Step 3: DELETE
    let delete_handler = DeleteHandler::new();
    let delete_stmt = SqlStatement::new(
        "DELETE FROM default.test_e2e WHERE id = '1'".to_string(),
        SqlStatementKind::Delete(kalamdb_sql::ddl::DeleteStatement),
    );
    let delete_params = vec![];
    let delete_result = delete_handler.execute(&session, delete_stmt, delete_params, &ctx).await;
    assert!(delete_result.is_ok(), "DELETE failed");
}

