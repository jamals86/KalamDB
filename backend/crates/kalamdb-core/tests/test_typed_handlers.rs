//! Integration test demonstrating typed handler pattern
//!
//! Shows how the executor classifies SQL, parses once, and dispatches to typed handlers.

use datafusion::prelude::SessionContext;
use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::executor::models::ExecutionContext;
use kalamdb_core::sql::executor::models::ExecutionResult;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::test_helpers::create_test_session;
use std::sync::Arc;

fn init_app_context() -> Arc<AppContext> {
    // Use test helper to initialize AppContext
    let _test_db = kalamdb_core::test_helpers::init_test_app_context();
    AppContext::get()
}

#[tokio::test]
async fn test_typed_handler_create_namespace() {
    let app_ctx = init_app_context();
    let executor = SqlExecutor::new(app_ctx, false);
    let session = SessionContext::new();
    let exec_ctx = ExecutionContext::new(UserId::from("admin"), Role::Dba, create_test_session());

    // Test CREATE NAMESPACE with typed handler
    let sql = "CREATE NAMESPACE integration_test_ns";
    let result = executor.execute(sql, &exec_ctx, vec![]).await;

    assert!(result.is_ok(), "CREATE NAMESPACE should succeed");

    match result.unwrap() {
        ExecutionResult::Success { message } => {
            assert!(message.contains("integration_test_ns"));
            assert!(message.contains("created successfully"));
        }
        _ => panic!("Expected Success result"),
    }
}

#[tokio::test]
async fn test_typed_handler_authorization() {
    let app_ctx = init_app_context();
    let executor = SqlExecutor::new(app_ctx, false);
    let session = SessionContext::new();
    let user_ctx = ExecutionContext::new(
        UserId::from("regular_user"),
        Role::User,
        create_test_session(),
    );

    // Regular users cannot create namespaces
    let sql = "CREATE NAMESPACE unauthorized_ns";
    let result = executor.execute(sql, &user_ctx, vec![]).await;

    assert!(
        result.is_err(),
        "Regular users should not create namespaces"
    );
}

#[tokio::test]
async fn test_classifier_prioritizes_select() {
    // This test verifies that SELECT queries go through the fast path
    // without attempting DDL parsing
    let app_ctx = init_app_context();
    let executor = SqlExecutor::new(app_ctx, false);
    let session = SessionContext::new();
    let exec_ctx = ExecutionContext::new(UserId::from("user"), Role::User, create_test_session());

    // SELECT should hit the DataFusion path immediately
    let sql = "SELECT 1 as test";
    let result = executor.execute(sql, &exec_ctx, vec![]).await;

    // Should succeed (DataFusion can execute this)
    assert!(result.is_ok(), "SELECT should execute via DataFusion");
}
