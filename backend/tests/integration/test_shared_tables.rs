//! Integration tests for Shared Table functionality (Phase 13)
//!
//! Tests the complete lifecycle of shared tables:
//! - CREATE TABLE (shared tables)
//! - INSERT operations
//! - UPDATE operations
//! - DELETE operations (soft/hard)
//! - SELECT queries
//! - DROP TABLE cleanup
//!
//! Uses the REST API `/api/sql` endpoint to test end-to-end functionality.

use actix_web::{test, web, App};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::services::NamespaceService;
use kalamdb_api::handlers::sql_handler::execute_sql;
use kalamdb_api::models::SqlRequest;
use std::sync::Arc;
use tempfile::TempDir;

/// Test setup helper - creates app with full KalamDB stack
async fn setup_test_app() -> (
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    TempDir,
) {
    // Create temporary database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_db");
    
    // Initialize RocksDB
    let db_init = kalamdb_core::storage::RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open().expect("Failed to open RocksDB");
    
    // Initialize KalamSQL
    let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(db).expect("Failed to create KalamSQL"));
    
    // Initialize NamespaceService
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    
    // Initialize DataFusion session factory
    let session_factory = Arc::new(
        DataFusionSessionFactory::new().expect("Failed to create DataFusion session factory")
    );
    
    // Create session context
    let session_context = Arc::new(session_factory.create_session());
    
    // Initialize SqlExecutor
    let sql_executor = Arc::new(SqlExecutor::new(
        namespace_service.clone(),
        session_context.clone(),
    ));
    
    // Create test app
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .service(execute_sql)
    )
    .await;
    
    (app, temp_dir)
}

/// Helper to execute SQL and get response
async fn execute_sql_request(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    sql: &str,
) -> kalamdb_api::models::SqlResponse {
    let req_body = SqlRequest {
        sql: sql.to_string(),
    };
    
    let req = test::TestRequest::post()
        .uri("/api/sql")
        .set_json(&req_body)
        .to_request();
    
    let resp = test::call_service(app, req).await;
    let body = test::read_body(resp).await;
    serde_json::from_slice(&body).expect("Failed to parse response")
}

#[actix_web::test]
async fn test_shared_table_create_and_drop() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Create namespace first
    let response = execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    assert_eq!(response.status, "success", "Failed to create namespace: {:?}", response.error);
    
    // Create shared table
    let create_sql = r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT,
            created_at BIGINT
        ) LOCATION '/data/shared/conversations'
        FLUSH POLICY ROWS 1000
        DELETED_RETENTION 7d
    "#;
    
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "Failed to create shared table: {:?}", response.error);
    
    // Verify table exists in system.tables
    let response = execute_sql_request(&app, "SELECT table_name, table_type FROM system.tables WHERE namespace_id = 'test_ns'").await;
    assert_eq!(response.status, "success");
    assert_eq!(response.results.len(), 1);
    // Note: May need adjustment based on actual system.tables schema
    
    // Drop table
    let response = execute_sql_request(&app, "DROP TABLE test_ns.conversations").await;
    assert_eq!(response.status, "success", "Failed to drop table: {:?}", response.error);
}

#[actix_web::test]
async fn test_shared_table_insert_and_select() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup: Create namespace and table
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT,
            participant_count BIGINT
        ) LOCATION '/data/shared/conversations'
    "#;
    execute_sql_request(&app, create_sql).await;
    
    // Insert data
    let insert_sql = r#"
        INSERT INTO test_ns.conversations (conversation_id, title, participant_count)
        VALUES ('conv001', 'Team Standup', 5)
    "#;
    
    let response = execute_sql_request(&app, insert_sql).await;
    assert_eq!(response.status, "success", "Failed to insert: {:?}", response.error);
    
    // Select data back
    let select_sql = "SELECT conversation_id, title, participant_count FROM test_ns.conversations WHERE conversation_id = 'conv001'";
    
    let response = execute_sql_request(&app, select_sql).await;
    assert_eq!(response.status, "success", "Failed to select: {:?}", response.error);
    assert_eq!(response.results.len(), 1);
    
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 1, "Expected 1 row, got {}", rows.len());
    
    let row = &rows[0];
    assert_eq!(row.get("conversation_id").unwrap().as_str().unwrap(), "conv001");
    assert_eq!(row.get("title").unwrap().as_str().unwrap(), "Team Standup");
    assert_eq!(row.get("participant_count").unwrap().as_i64().unwrap(), 5);
}

#[actix_web::test]
async fn test_shared_table_multiple_inserts() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert multiple rows
    let insert_sql = r#"
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv001', 'Standup');
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv002', 'Planning');
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv003', 'Review');
    "#;
    
    let response = execute_sql_request(&app, insert_sql).await;
    assert_eq!(response.status, "success", "Failed to insert multiple rows: {:?}", response.error);
    assert_eq!(response.results.len(), 3, "Expected 3 results for 3 INSERT statements");
    
    // Verify all rows exist
    let select_sql = "SELECT conversation_id, title FROM test_ns.conversations ORDER BY conversation_id";
    let response = execute_sql_request(&app, select_sql).await;
    
    assert_eq!(response.status, "success");
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 3, "Expected 3 rows");
    
    assert_eq!(rows[0].get("conversation_id").unwrap().as_str().unwrap(), "conv001");
    assert_eq!(rows[1].get("conversation_id").unwrap().as_str().unwrap(), "conv002");
    assert_eq!(rows[2].get("conversation_id").unwrap().as_str().unwrap(), "conv003");
}

#[actix_web::test]
async fn test_shared_table_update() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT,
            status TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert initial data
    execute_sql_request(&app, r#"
        INSERT INTO test_ns.conversations (conversation_id, title, status)
        VALUES ('conv001', 'Planning Meeting', 'active')
    "#).await;
    
    // Update the row
    let update_sql = r#"
        UPDATE test_ns.conversations 
        SET title = 'Updated Planning Meeting', status = 'archived'
        WHERE conversation_id = 'conv001'
    "#;
    
    let response = execute_sql_request(&app, update_sql).await;
    assert_eq!(response.status, "success", "Failed to update: {:?}", response.error);
    
    // Verify update
    let response = execute_sql_request(&app, 
        "SELECT title, status FROM test_ns.conversations WHERE conversation_id = 'conv001'"
    ).await;
    
    assert_eq!(response.status, "success");
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 1);
    
    assert_eq!(rows[0].get("title").unwrap().as_str().unwrap(), "Updated Planning Meeting");
    assert_eq!(rows[0].get("status").unwrap().as_str().unwrap(), "archived");
}

#[actix_web::test]
async fn test_shared_table_delete() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert data
    execute_sql_request(&app, r#"
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv001', 'To Delete');
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv002', 'To Keep');
    "#).await;
    
    // Delete one row
    let delete_sql = "DELETE FROM test_ns.conversations WHERE conversation_id = 'conv001'";
    let response = execute_sql_request(&app, delete_sql).await;
    assert_eq!(response.status, "success", "Failed to delete: {:?}", response.error);
    
    // Verify deletion (soft delete should hide the row)
    let response = execute_sql_request(&app, 
        "SELECT conversation_id FROM test_ns.conversations ORDER BY conversation_id"
    ).await;
    
    assert_eq!(response.status, "success");
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 1, "Expected 1 row after delete");
    assert_eq!(rows[0].get("conversation_id").unwrap().as_str().unwrap(), "conv002");
}

#[actix_web::test]
async fn test_shared_table_system_columns() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert data
    execute_sql_request(&app, r#"
        INSERT INTO test_ns.conversations (conversation_id, title)
        VALUES ('conv001', 'Test Conversation')
    "#).await;
    
    // Query including system columns
    let response = execute_sql_request(&app, 
        "SELECT conversation_id, title, _updated, _deleted FROM test_ns.conversations"
    ).await;
    
    assert_eq!(response.status, "success", "Failed to query system columns: {:?}", response.error);
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 1);
    
    let row = &rows[0];
    assert!(row.contains_key("_updated"), "System column _updated missing");
    assert!(row.contains_key("_deleted"), "System column _deleted missing");
    
    // _deleted should be false for newly inserted row
    assert_eq!(row.get("_deleted").unwrap().as_bool().unwrap(), false);
}

#[actix_web::test]
async fn test_shared_table_if_not_exists() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE IF NOT EXISTS test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#;
    
    // First create should succeed
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success");
    
    // Second create with IF NOT EXISTS should also succeed (no-op)
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "IF NOT EXISTS should not fail on duplicate: {:?}", response.error);
}

#[actix_web::test]
async fn test_shared_table_flush_policy_rows() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
        FLUSH POLICY ROWS 500
    "#;
    
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "Failed to create table with FLUSH POLICY ROWS: {:?}", response.error);
}

#[actix_web::test]
async fn test_shared_table_flush_policy_time() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
        FLUSH POLICY TIME 300s
    "#;
    
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "Failed to create table with FLUSH POLICY TIME: {:?}", response.error);
}

#[actix_web::test]
async fn test_shared_table_flush_policy_combined() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
        FLUSH POLICY ROWS 1000 OR TIME 600s
    "#;
    
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "Failed to create table with combined FLUSH POLICY: {:?}", response.error);
}

#[actix_web::test]
async fn test_shared_table_deleted_retention() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
        DELETED_RETENTION 14d
    "#;
    
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "Failed to create table with DELETED_RETENTION: {:?}", response.error);
}

#[actix_web::test]
async fn test_shared_table_multiple_types() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    
    let create_sql = r#"
        CREATE TABLE test_ns.test_types (
            id TEXT NOT NULL,
            count BIGINT,
            price DOUBLE,
            is_active BOOLEAN,
            created_at TIMESTAMP
        ) LOCATION '/data/shared/test_types'
    "#;
    
    let response = execute_sql_request(&app, create_sql).await;
    assert_eq!(response.status, "success", "Failed to create table with multiple types: {:?}", response.error);
    
    // Insert data with various types
    let insert_sql = r#"
        INSERT INTO test_ns.test_types (id, count, price, is_active, created_at)
        VALUES ('item1', 42, 99.99, true, '2025-10-19T12:00:00Z')
    "#;
    
    let response = execute_sql_request(&app, insert_sql).await;
    assert_eq!(response.status, "success", "Failed to insert with multiple types: {:?}", response.error);
    
    // Query back
    let response = execute_sql_request(&app, 
        "SELECT id, count, price, is_active FROM test_ns.test_types WHERE id = 'item1'"
    ).await;
    
    assert_eq!(response.status, "success");
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 1);
    
    let row = &rows[0];
    assert_eq!(row.get("id").unwrap().as_str().unwrap(), "item1");
    assert_eq!(row.get("count").unwrap().as_i64().unwrap(), 42);
    assert_eq!(row.get("is_active").unwrap().as_bool().unwrap(), true);
    // Note: DOUBLE and TIMESTAMP parsing may need adjustment based on actual behavior
}

#[actix_web::test]
async fn test_shared_table_query_empty_table() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Query empty table
    let response = execute_sql_request(&app, 
        "SELECT conversation_id, title FROM test_ns.conversations"
    ).await;
    
    assert_eq!(response.status, "success");
    assert_eq!(response.results.len(), 1);
    
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 0, "Empty table should return 0 rows");
}

#[actix_web::test]
async fn test_shared_table_where_clause() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT,
            status TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert multiple rows with different statuses
    execute_sql_request(&app, r#"
        INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv001', 'Active 1', 'active');
        INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv002', 'Active 2', 'active');
        INSERT INTO test_ns.conversations (conversation_id, title, status) VALUES ('conv003', 'Archived', 'archived');
    "#).await;
    
    // Query with WHERE clause
    let response = execute_sql_request(&app, 
        "SELECT conversation_id, title FROM test_ns.conversations WHERE status = 'active' ORDER BY conversation_id"
    ).await;
    
    assert_eq!(response.status, "success");
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 2, "Expected 2 active conversations");
    
    assert_eq!(rows[0].get("conversation_id").unwrap().as_str().unwrap(), "conv001");
    assert_eq!(rows[1].get("conversation_id").unwrap().as_str().unwrap(), "conv002");
}

#[actix_web::test]
async fn test_shared_table_count_aggregation() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert data
    execute_sql_request(&app, r#"
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv001', 'One');
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv002', 'Two');
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv003', 'Three');
    "#).await;
    
    // COUNT aggregation
    let response = execute_sql_request(&app, 
        "SELECT COUNT(*) as total FROM test_ns.conversations"
    ).await;
    
    assert_eq!(response.status, "success");
    let rows = &response.results[0].rows;
    assert_eq!(rows.len(), 1);
    
    // Note: Exact field name and type may vary based on DataFusion behavior
    assert!(rows[0].contains_key("total") || rows[0].contains_key("COUNT(*)"));
}

#[actix_web::test]
async fn test_shared_table_drop_cascade() {
    let (app, _temp_dir) = setup_test_app().await;
    
    // Setup
    execute_sql_request(&app, "CREATE NAMESPACE test_ns").await;
    execute_sql_request(&app, r#"
        CREATE TABLE test_ns.conversations (
            conversation_id TEXT NOT NULL,
            title TEXT
        ) LOCATION '/data/shared/conversations'
    "#).await;
    
    // Insert data
    execute_sql_request(&app, r#"
        INSERT INTO test_ns.conversations (conversation_id, title) VALUES ('conv001', 'Test');
    "#).await;
    
    // Drop table (should delete data and metadata)
    let response = execute_sql_request(&app, "DROP TABLE test_ns.conversations").await;
    assert_eq!(response.status, "success", "Failed to drop table: {:?}", response.error);
    
    // Verify table no longer exists - attempting to query should fail
    let response = execute_sql_request(&app, 
        "SELECT * FROM test_ns.conversations"
    ).await;
    
    assert_eq!(response.status, "error", "Query should fail on dropped table");
}

// Note: Additional tests that may require more infrastructure:
// - test_shared_table_flush_job_execution (requires job scheduler)
// - test_shared_table_parquet_persistence (requires flush job + storage verification)
// - test_shared_table_concurrent_access (requires multi-threaded test setup)
// - test_shared_table_large_batch_insert (performance test)
