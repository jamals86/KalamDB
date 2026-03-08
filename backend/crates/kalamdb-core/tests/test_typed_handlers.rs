//! Integration test demonstrating typed handler pattern
//!
//! Shows how the executor classifies SQL, parses once, and dispatches to typed handlers.

use kalamdb_commons::models::UserId;
use kalamdb_commons::NodeId;
use kalamdb_commons::Role;
use kalamdb_configs::ServerConfig;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::context::ExecutionContext;
use kalamdb_core::sql::context::ExecutionResult;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_store::test_utils::TestDb;
use std::sync::Arc;

/// Helper to create AppContext with temporary RocksDB for testing
async fn create_test_app_context() -> (Arc<AppContext>, TestDb) {
    let test_db = TestDb::with_system_tables().expect("Failed to create test database");
    let storage_base_path = test_db.storage_dir().expect("Failed to create storage directory");
    let backend = test_db.backend();

    let app_context = AppContext::create_isolated(
        backend,
        NodeId::new(1),
        storage_base_path.to_string_lossy().into_owned(),
        ServerConfig::default(),
    );

    (app_context, test_db)
}

#[tokio::test]
#[ignore = "Requires Raft for CREATE NAMESPACE"]
async fn test_typed_handler_create_namespace() {
    let (app_ctx, _test_db) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let exec_ctx =
        ExecutionContext::new(UserId::from("admin"), Role::Dba, app_ctx.base_session_context());

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
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let user_ctx = ExecutionContext::new(
        UserId::from("regular_user"),
        Role::User,
        app_ctx.base_session_context(),
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
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let exec_ctx =
        ExecutionContext::new(UserId::from("user"), Role::User, app_ctx.base_session_context());

    // SELECT should hit the DataFusion path immediately
    let sql = "SELECT 1 as test";
    let result = executor.execute(sql, &exec_ctx, vec![]).await;

    // Should succeed (DataFusion can execute this)
    assert!(result.is_ok(), "SELECT should execute via DataFusion");
}
