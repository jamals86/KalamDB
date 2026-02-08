//! Integration tests for Common Table Expressions (CTEs)
//!
//! Tests cover:
//! - Simple CTEs with single WITH clause
//! - Multiple CTEs (chained WITH clauses)
//! - Recursive CTEs
//! - CTEs with aggregations
//! - CTEs with JOINs
//! - CTEs with filtering

use kalamdb_commons::{Role, UserId, UserName};
use kalamdb_core::app_context::{AppContext, AppContextBuilder};
use kalamdb_core::execution_result::ExecutionResult;
use kalamdb_core::sql::context::ExecutionContext;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_session::AuthSession;
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

/// Helper to create ExecutionContext with username
fn create_exec_context_with_app_context(
    app_context: Arc<AppContext>,
    username: &str,
    user_id: &str,
    role: Role,
) -> ExecutionContext {
    let auth_session = AuthSession::with_username_and_auth_details(
        UserId::new(user_id),
        UserName::new(username),
        role,
        kalamdb_commons::models::ConnectionInfo::new(None),
        kalamdb_session::AuthMethod::Bearer,
    );
    ExecutionContext::new(
        auth_session,
        app_context.datafusion_session_factory().clone(),
    )
}

/// Setup: Create a test table and insert sample data
async fn setup_test_table(
    executor: &SqlExecutor,
    exec_ctx: &ExecutionContext,
) -> Result<(), String> {
    // Create a namespace
    executor
        .execute(
            "CREATE NAMESPACE test_ns",
            exec_ctx,
            vec![],
        )
        .await
        .map_err(|e| format!("Failed to create namespace: {}", e))?;

    // Create a test table
    executor
        .execute(
            "CREATE USER TABLE test_ns.employees (id INT, name TEXT, department TEXT, salary INT)",
            exec_ctx,
            vec![],
        )
        .await
        .map_err(|e| format!("Failed to create table: {}", e))?;

    // Insert test data
    let insert_queries = vec![
        "INSERT INTO test_ns.employees (id, name, department, salary) VALUES (1, 'Alice', 'Engineering', 100000)",
        "INSERT INTO test_ns.employees (id, name, department, salary) VALUES (2, 'Bob', 'Engineering', 90000)",
        "INSERT INTO test_ns.employees (id, name, department, salary) VALUES (3, 'Charlie', 'Sales', 80000)",
        "INSERT INTO test_ns.employees (id, name, department, salary) VALUES (4, 'Diana', 'Sales', 85000)",
        "INSERT INTO test_ns.employees (id, name, department, salary) VALUES (5, 'Eve', 'Marketing', 75000)",
    ];

    for query in insert_queries {
        executor
            .execute(query, exec_ctx, vec![])
            .await
            .map_err(|e| format!("Failed to insert data: {}", e))?;
    }

    Ok(())
}

// ============================================================================
// SIMPLE CTE TESTS
// ============================================================================

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_simple_cte() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test simple CTE
    let result = executor
        .execute(
            r#"
            WITH high_earners AS (
                SELECT name, salary 
                FROM test_ns.employees 
                WHERE salary > 80000
            )
            SELECT * FROM high_earners ORDER BY salary DESC
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_ok(), "CTE query should succeed: {:?}", result.err());
    let exec_result = result.unwrap();

    match exec_result {
        ExecutionResult::Query(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            assert_eq!(total_rows, 3, "Should return 3 high earners (Alice, Bob, Diana)");
        },
        _ => panic!("Expected Query result, got: {:?}", exec_result),
    }
}

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cte_with_aggregation() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test CTE with aggregation
    let result = executor
        .execute(
            r#"
            WITH dept_stats AS (
                SELECT 
                    department,
                    COUNT(*) as employee_count,
                    AVG(salary) as avg_salary
                FROM test_ns.employees 
                GROUP BY department
            )
            SELECT * FROM dept_stats ORDER BY avg_salary DESC
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_ok(), "CTE with aggregation should succeed: {:?}", result.err());
    let exec_result = result.unwrap();

    match exec_result {
        ExecutionResult::Query(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            assert_eq!(total_rows, 3, "Should return 3 departments");
        },
        _ => panic!("Expected Query result, got: {:?}", exec_result),
    }
}

// ============================================================================
// MULTIPLE CTE TESTS
// ============================================================================

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_multiple_ctes() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test multiple CTEs
    let result = executor
        .execute(
            r#"
            WITH 
                engineering AS (
                    SELECT name, salary 
                    FROM test_ns.employees 
                    WHERE department = 'Engineering'
                ),
                sales AS (
                    SELECT name, salary 
                    FROM test_ns.employees 
                    WHERE department = 'Sales'
                )
            SELECT 'Engineering' as dept, name, salary FROM engineering
            UNION ALL
            SELECT 'Sales' as dept, name, salary FROM sales
            ORDER BY salary DESC
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_ok(), "Multiple CTEs should succeed: {:?}", result.err());
    let exec_result = result.unwrap();

    match exec_result {
        ExecutionResult::Query(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            assert_eq!(total_rows, 4, "Should return 4 employees (2 Engineering + 2 Sales)");
        },
        _ => panic!("Expected Query result, got: {:?}", exec_result),
    }
}

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_chained_ctes() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test chained CTEs (one CTE references another)
    let result = executor
        .execute(
            r#"
            WITH 
                all_employees AS (
                    SELECT * FROM test_ns.employees
                ),
                high_earners AS (
                    SELECT * FROM all_employees WHERE salary > 80000
                )
            SELECT name, department, salary 
            FROM high_earners 
            ORDER BY salary DESC
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_ok(), "Chained CTEs should succeed: {:?}", result.err());
    let exec_result = result.unwrap();

    match exec_result {
        ExecutionResult::Query(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            assert_eq!(total_rows, 3, "Should return 3 high earners");
        },
        _ => panic!("Expected Query result, got: {:?}", exec_result),
    }
}

// ============================================================================
// CTE WITH FILTERING AND ORDERING TESTS
// ============================================================================

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cte_with_where_clause() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test CTE with WHERE clause in both CTE and main query
    let result = executor
        .execute(
            r#"
            WITH dept_employees AS (
                SELECT name, salary, department
                FROM test_ns.employees
                WHERE department IN ('Engineering', 'Sales')
            )
            SELECT * FROM dept_employees 
            WHERE salary >= 85000
            ORDER BY salary DESC
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_ok(), "CTE with WHERE should succeed: {:?}", result.err());
    let exec_result = result.unwrap();

    match exec_result {
        ExecutionResult::Query(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            assert_eq!(total_rows, 3, "Should return 3 employees (Alice, Bob, Diana)");
        },
        _ => panic!("Expected Query result, got: {:?}", exec_result),
    }
}

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cte_with_limit() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test CTE with LIMIT
    let result = executor
        .execute(
            r#"
            WITH all_salaries AS (
                SELECT name, salary 
                FROM test_ns.employees
                ORDER BY salary DESC
            )
            SELECT * FROM all_salaries LIMIT 2
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_ok(), "CTE with LIMIT should succeed: {:?}", result.err());
    let exec_result = result.unwrap();

    match exec_result {
        ExecutionResult::Query(batches) => {
            let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
            assert_eq!(total_rows, 2, "Should return top 2 earners");
        },
        _ => panic!("Expected Query result, got: {:?}", exec_result),
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cte_syntax_error() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // Setup test data
    setup_test_table(&executor, &exec_ctx).await.unwrap();

    // Test CTE with syntax error (missing AS keyword)
    let result = executor
        .execute(
            r#"
            WITH high_earners (
                SELECT name, salary 
                FROM test_ns.employees 
                WHERE salary > 80000
            )
            SELECT * FROM high_earners
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_err(), "Invalid CTE syntax should fail");
}

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cte_undefined_table() {
    let (app_context, _temp_dir) = create_test_app_context().await;
    let exec_ctx = create_exec_context_with_app_context(
        app_context.clone(),
        "admin",
        "u_admin",
        Role::Dba,
    );
    let executor = SqlExecutor::new(app_context.clone(), false);

    // No setup - testing undefined table

    // Test CTE referencing non-existent table
    let result = executor
        .execute(
            r#"
            WITH data AS (
                SELECT * FROM non_existent_table
            )
            SELECT * FROM data
            "#,
            &exec_ctx,
            vec![],
        )
        .await;

    assert!(result.is_err(), "CTE with undefined table should fail");
}
