//! Integration test demonstrating typed handler pattern
//!
//! Shows how the executor classifies SQL, parses once, and dispatches to typed handlers.

use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::context::ExecutionContext;
use kalamdb_core::sql::context::ExecutionResult;
use kalamdb_core::sql::executor::SqlExecutor;
use std::sync::Arc;

fn init_app_context() -> Arc<AppContext> {
    test_app_context_simple()
}

#[tokio::test]
#[ignore = "Requires Raft for CREATE NAMESPACE"]
async fn test_typed_handler_create_namespace() {
    let app_ctx = test_app_context();
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let exec_ctx = ExecutionContext::new(
        UserId::from("admin"),
        Role::Dba,
        Arc::new(app_ctx.session_factory().create_session()),
    );

    // Test CREATE NAMESPACE with typed handler
    let namespace = format!("integration_test_ns_{}", std::process::id());
    let sql = format!("CREATE NAMESPACE {}", namespace);
    let result = executor.execute(&sql, &exec_ctx, vec![]).await;

    assert!(result.is_ok(), "CREATE NAMESPACE should succeed");

    match result.unwrap() {
        ExecutionResult::Success { message } => {
            assert!(message.contains(&namespace));
            assert!(message.contains("created successfully"));
        },
        _ => panic!("Expected Success result"),
    }
}

#[tokio::test]
async fn test_typed_handler_authorization() {
    let app_ctx = init_app_context();
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let user_ctx = ExecutionContext::new(
        UserId::from("regular_user"),
        Role::User,
        Arc::new(app_ctx.session_factory().create_session()),
    );

    // Regular users cannot create namespaces
    let sql = "CREATE NAMESPACE unauthorized_ns";
    let result = executor.execute(sql, &user_ctx, vec![]).await;

    assert!(result.is_err(), "Regular users should not create namespaces");
}

#[tokio::test]
async fn test_classifier_prioritizes_select() {
    // This test verifies that SELECT queries go through the fast path
    // without attempting DDL parsing
    let app_ctx = init_app_context();
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let exec_ctx = ExecutionContext::new(
        UserId::from("user"),
        Role::User,
        Arc::new(app_ctx.session_factory().create_session()),
    );

    // SELECT should hit the DataFusion path immediately
    let sql = "SELECT 1 as test";
    let result = executor.execute(sql, &exec_ctx, vec![]).await;

    // Should succeed (DataFusion can execute this)
    assert!(result.is_ok(), "SELECT should execute via DataFusion");
}
