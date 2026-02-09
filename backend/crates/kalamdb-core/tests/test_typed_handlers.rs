//! Integration test demonstrating typed handler pattern
//!
//! Shows how the executor classifies SQL, parses once, and dispatches to typed handlers.

use kalamdb_commons::models::UserId;
use kalamdb_commons::Role;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::context::{ExecutionContext, ExecutionResult};
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_session::AuthSession;
use kalamdb_commons::UserName;
use kalamdb_store::RocksDBBackend;
use kalamdb_configs::ServerConfig;
use kalamdb_commons::NodeId;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create AppContext with temporary RocksDB for testing
async fn create_test_app_context() -> (Arc<AppContext>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let rocksdb_path = temp_dir.path().join("rocksdb");
    std::fs::create_dir_all(&rocksdb_path).expect("Failed to create rocksdb directory");

    let db = rocksdb::DB::open_default(&rocksdb_path).expect("Failed to open RocksDB");
    let backend = Arc::new(RocksDBBackend::new(db));
    let config = ServerConfig::default();
    let node_id = NodeId::new(1);

    let app_context = AppContext::create_isolated(
        backend,
        node_id,
        rocksdb_path.to_string_lossy().into_owned(),
        config,
    );

    (app_context, temp_dir)
}



#[tokio::test]
#[ignore = "Requires Raft for CREATE NAMESPACE"]
async fn test_typed_handler_create_namespace() {
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let user_id = UserId::new("admin".to_string());
    let base_session = app_ctx.base_session_context();
    let exec_ctx = ExecutionContext::new(
        user_id,
        Role::Dba,
        base_session,
    );

    // Test CREATE NAMESPACE with typed handler
    let namespace = format!("integration_test_ns_{}", std::process::id());
    let sql = format!("CREATE NAMESPACE {}", namespace);
    let result: Result<ExecutionResult, _> = executor.execute(&sql, &exec_ctx, vec![]).await;

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
    let user_id = UserId::new("regular_user".to_string());
    let base_session = app_ctx.base_session_context();
    let user_ctx = ExecutionContext::new(
        user_id,
        Role::User,
        base_session,
    );

    // Regular users cannot create namespaces
    let sql = "CREATE NAMESPACE unauthorized_ns";
    let result: Result<ExecutionResult, _> = executor.execute(sql, &user_ctx, vec![]).await;

    assert!(result.is_err(), "Regular users should not create namespaces");
}

#[tokio::test]
async fn test_classifier_prioritizes_select() {
    // This test verifies that SELECT queries go through the fast path
    // without attempting DDL parsing
    let (app_ctx, _temp_dir) = create_test_app_context().await;
    let executor = SqlExecutor::new(Arc::clone(&app_ctx), false);
    let user_id = UserId::new("user".to_string());
    let base_session = app_ctx.base_session_context();
    let exec_ctx = ExecutionContext::new(
        user_id,
        Role::User,
        base_session,
    );

    // SELECT should hit the DataFusion path immediately
    let sql = "SELECT 1 as test";
    let result: Result<ExecutionResult, _> = executor.execute(sql, &exec_ctx, vec![]).await;

    // Should succeed (DataFusion can execute this)
    assert!(result.is_ok(), "SELECT should execute via DataFusion");
}
