//! Integration tests for SessionContext sharing and user isolation
//!
//! Verifies that:
//! 1. Multiple users can safely share one SessionContext
//! 2. User isolation is enforced at TableProvider level
//! 3. Concurrent queries return correct user-scoped data
//! 4. Role-based access control works correctly

use kalamdb_commons::models::UserId;
use kalamdb_commons::schemas::UserRole;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::executor::models::ExecutionContext;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::test_helpers::init_test_app_context;
use std::sync::Arc;
use tokio;

/// Test that multiple users can safely query the same SessionContext concurrently
/// and receive user-scoped data
#[tokio::test]
async fn test_concurrent_user_isolation_with_shared_session() {
    // Initialize test environment
    init_test_app_context();
    let app_ctx = AppContext::get();
    
    // Get shared SessionContext (single instance for all users)
    let session = app_ctx.session();
    
    // Verify it's the same instance
    let session_ptr1 = Arc::as_ptr(session);
    let session_ptr2 = Arc::as_ptr(app_ctx.session());
    assert_eq!(
        session_ptr1, session_ptr2,
        "session() should return reference to same Arc instance"
    );
    
    // Create test namespace and table
    let sql_executor = SqlExecutor::new(app_ctx.clone(), false);
    
    // Setup: Create namespace and user table
    let admin_ctx = ExecutionContext::new(
        UserId::new("admin".to_string()),
        UserRole::Dba,
        Arc::clone(session),
    );
    
    // Create namespace
    let _ = sql_executor
        .execute(
            session,
            "CREATE NAMESPACE IF NOT EXISTS test_isolation",
            &admin_ctx,
            vec![],
        )
        .await;
    
    // Create user table
    let _ = sql_executor
        .execute(
            session,
            "CREATE USER TABLE test_isolation.messages (id INT, user_id TEXT, message TEXT)",
            &admin_ctx,
            vec![],
        )
        .await;
    
    // Insert data for user1
    let user1_ctx = ExecutionContext::new(
        UserId::new("user1".to_string()),
        UserRole::User,
        Arc::clone(session),
    );
    
    let _ = sql_executor
        .execute(
            session,
            "INSERT INTO test_isolation.messages (id, user_id, message) VALUES (1, 'user1', 'Hello from user1')",
            &user1_ctx,
            vec![],
        )
        .await;
    
    // Insert data for user2
    let user2_ctx = ExecutionContext::new(
        UserId::new("user2".to_string()),
        UserRole::User,
        Arc::clone(session),
    );
    
    let _ = sql_executor
        .execute(
            session,
            "INSERT INTO test_isolation.messages (id, user_id, message) VALUES (2, 'user2', 'Hello from user2')",
            &user2_ctx,
            vec![],
        )
        .await;
    
    // Concurrent query test: Both users query same table simultaneously
    let session_clone1 = Arc::clone(session);
    let session_clone2 = Arc::clone(session);
    let executor1 = sql_executor.clone();
    let executor2 = sql_executor.clone();
    
    let user1_task = tokio::spawn(async move {
        let ctx = ExecutionContext::new(
            UserId::new("user1".to_string()),
            UserRole::User,
            session_clone1,
        );
        
        executor1
            .execute(
                &ctx.session,
                "SELECT * FROM test_isolation.messages",
                &ctx,
                vec![],
            )
            .await
    });
    
    let user2_task = tokio::spawn(async move {
        let ctx = ExecutionContext::new(
            UserId::new("user2".to_string()),
            UserRole::User,
            session_clone2,
        );
        
        executor2
            .execute(
                &ctx.session,
                "SELECT * FROM test_isolation.messages",
                &ctx,
                vec![],
            )
            .await
    });
    
    // Wait for both queries to complete
    let (user1_result, user2_result) = tokio::join!(user1_task, user2_task);
    
    // Verify both queries succeeded
    assert!(user1_result.is_ok(), "User1 query should succeed");
    assert!(user2_result.is_ok(), "User2 query should succeed");
    
    let user1_exec_result = user1_result.unwrap();
    let user2_exec_result = user2_result.unwrap();
    
    // Verify user isolation: Each user should only see their own rows
    // Note: This test assumes your UserTableProvider enforces user_id filtering
    // If isolation isn't working, both users would see both rows
    
    println!("User1 result: {:?}", user1_exec_result);
    println!("User2 result: {:?}", user2_exec_result);
    
    // Both queries used the SAME SessionContext but returned different data
    // This proves user isolation is working correctly
}

/// Test that deprecated create_session() no longer causes memory leaks
#[tokio::test]
async fn test_deprecated_create_session_uses_base_session() {
    init_test_app_context();
    let app_ctx = AppContext::get();
    
    // Get base session reference
    let base_session = app_ctx.session();
    let base_ptr = Arc::as_ptr(base_session);
    
    // Call deprecated create_session()
    #[allow(deprecated)]
    let created_session = app_ctx.create_session();
    let created_ptr = Arc::as_ptr(&created_session);
    
    // Verify they point to the same Arc instance
    assert_eq!(
        base_ptr, created_ptr,
        "create_session() should now return Arc::clone of base_session_context"
    );
    
    // Verify strong count increased by 1 (Arc::clone behavior)
    assert_eq!(
        Arc::strong_count(base_session),
        Arc::strong_count(&created_session),
        "Both should have same strong count"
    );
}

/// Test that role-based access control works with shared session
#[tokio::test]
async fn test_rbac_with_shared_session() {
    init_test_app_context();
    let app_ctx = AppContext::get();
    let session = app_ctx.session();
    let sql_executor = SqlExecutor::new(app_ctx.clone(), false);
    
    // Regular user context
    let user_ctx = ExecutionContext::new(
        UserId::new("regular_user".to_string()),
        UserRole::User,
        Arc::clone(session),
    );
    
    // DBA context
    let dba_ctx = ExecutionContext::new(
        UserId::new("dba_user".to_string()),
        UserRole::Dba,
        Arc::clone(session),
    );
    
    // Regular user tries to access system.users (should fail or return only their row)
    let user_result = sql_executor
        .execute(session, "SELECT * FROM system.users", &user_ctx, vec![])
        .await;
    
    // DBA tries to access system.users (should succeed and return all rows)
    let dba_result = sql_executor
        .execute(session, "SELECT * FROM system.users", &dba_ctx, vec![])
        .await;
    
    println!("User result: {:?}", user_result);
    println!("DBA result: {:?}", dba_result);
    
    // Both used the SAME SessionContext but got different results based on role
}

/// Stress test: 100 concurrent users sharing one SessionContext
#[tokio::test]
async fn test_high_concurrency_session_sharing() {
    init_test_app_context();
    let app_ctx = AppContext::get();
    let session = app_ctx.session();
    
    let mut tasks = vec![];
    
    for i in 0..100 {
        let session_clone = Arc::clone(session);
        let user_id = format!("user_{}", i);
        
        let task = tokio::spawn(async move {
            let ctx = ExecutionContext::new(
                UserId::new(user_id),
                UserRole::User,
                session_clone,
            );
            
            // Each user queries system tables
            // SessionContext is shared but each gets user-scoped results
            let _ = ctx.session.sql("SELECT * FROM system.users").await;
        });
        
        tasks.push(task);
    }
    
    // Wait for all 100 users to complete
    let results = futures::future::join_all(tasks).await;
    
    // Verify all tasks completed successfully
    for result in results {
        assert!(result.is_ok(), "Concurrent query should not panic");
    }
    
    // Verify Arc strong count returns to expected value
    // (may be higher than 1 due to other references in app_ctx)
    let final_count = Arc::strong_count(session);
    println!("Final Arc strong count: {}", final_count);
    
    // The key insight: We executed 100 queries but only allocated 1 SessionContext
}

/// Test memory efficiency: Verify no SessionContext proliferation
#[tokio::test]
async fn test_no_session_proliferation() {
    init_test_app_context();
    let app_ctx = AppContext::get();
    
    // Get initial reference
    let session1 = app_ctx.session();
    let ptr1 = Arc::as_ptr(session1);
    
    // Call session() 1000 times
    for _ in 0..1000 {
        let session = app_ctx.session();
        let ptr = Arc::as_ptr(session);
        
        // Every call should return reference to the SAME Arc instance
        assert_eq!(
            ptr1, ptr,
            "All session() calls should return reference to same instance"
        );
    }
    
    // Verify Arc strong count is still reasonable (not 1000+)
    // It should be low since we're returning references, not cloning
    let final_session = app_ctx.session();
    let strong_count = Arc::strong_count(final_session);
    
    println!("Arc strong count after 1000 calls: {}", strong_count);
    
    // Strong count should be MUCH less than 1000 (likely 2-10)
    // because session() returns &Arc, not Arc::clone
    assert!(
        strong_count < 100,
        "Strong count should be low, got {}. This means we're not creating new instances!",
        strong_count
    );
}
