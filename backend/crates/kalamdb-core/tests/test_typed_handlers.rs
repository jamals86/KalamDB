//! Integration test demonstrating typed handler pattern
//!
//! Shows how the executor classifies SQL, parses once, and dispatches to typed handlers.

use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_core::app_context::{AppContext, AppContextBuilder};
use kalamdb_core::sql::context::ExecutionContext;
use kalamdb_core::sql::context::ExecutionResult;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_session::AuthSession;
use kalamdb_commons::UserName;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create AppContext with temporary RocksDB for testing
async fn create_test_app_context() -> (Arc<AppContext>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rocksdb_path = temp_dir.path().join("rocksdb");

    let app_context = AppContextBuilder::new(&rocksdb_path)
        .build()
        .await
        .expect("Failed to build AppContext");

    (Arc::new(app_context), temp_dir)
}

fn init_app_context() -> Arc<AppContext> {
    // This needs to be async but for now return a placeholder
    // The tests that need it are marked #[ignore]
    panic!("init_app_context requires async context - use #[tokio::test]")
}

#[tokio::test]
#[ignore = "Requires Raft for CREATE NAMESPACE"]
async fn test_typed_handler_create_namespace() {
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let auth_session = AuthSession::with_username_and_auth_details(
        UserId::from("admin"),
        UserName::from("admin"),
        Role::Dba,
        kalamdb_commons::models::ConnectionInfo::new(None),
        kalamdb_session::AuthMethod::Bearer,
    );
    let exec_ctx = ExecutionContext::new(
        auth_session,
        app_ctx.datafusion_session_factory().clone(),
    );

    // Test CREATE NAMESPACE with typed handler
    let namespace = format!("integration_test_ns_{}", std::process::id());
    let sql = format!("CREATE NAMESPACE {}", namespace);
    let result = executor.execute_sql(&sql, &exec_ctx).await;

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
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let auth_session = AuthSession::with_username_and_auth_details(
        UserId::from("regular_user"),
        UserName::from("regular_user"),
        Role::User,
        kalamdb_commons::models::ConnectionInfo::new(None),
        kalamdb_session::AuthMethod::Bearer,
    );
    let user_ctx = ExecutionContext::new(
        auth_session,
        app_ctx.datafusion_session_factory().clone(),
    );

    // Regular users cannot create namespaces
    let sql = "CREATE NAMESPACE unauthorized_ns";
    let result = executor.execute_sql(sql, &user_ctx).await;

    assert!(result.is_err(), "Regular users should not create namespaces");
}

#[tokio::test]
async fn test_classifier_prioritizes_select() {
    // This test verifies that SELECT queries go through the fast path
    // without attempting DDL parsing
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let auth_session = AuthSession::with_username_and_auth_details(
        UserId::from("user"),
        UserName::from("user"),
        Role::User,
        kalamdb_commons::models::ConnectionInfo::new(None),
        kalamdb_session::AuthMethod::Bearer,
    );
    let exec_ctx = ExecutionContext::new(
        auth_session,
        app_ctx.datafusion_session_factory().clone(),
    );

    // SELECT should hit the DataFusion path immediately
    let sql = "SELECT 1 as test";
    let result = executor.execute_sql(sql, &exec_ctx).await;

    // Should succeed (DataFusion can execute this)
    assert!(result.is_ok(), "SELECT should execute via DataFusion");
}
