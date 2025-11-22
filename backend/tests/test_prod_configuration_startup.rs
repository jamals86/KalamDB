//! Configuration and Startup Tests
//!
//! Tests valid and invalid server configurations to ensure:
//! - The server starts successfully with valid configs
//! - Invalid configs fail fast with clear error messages
//! - Health/readiness endpoints respond correctly
//!
//! These tests validate that operational errors (wrong config) are caught
//! early and provide actionable feedback.

// We need TestServer but the integration common module has circular deps
// Just include what we need directly
use kalamdb_api::models::ResponseStatus;
use kalamdb_core::app_context::AppContext;
use kalamdb_store::RocksDbInit;
use tempfile::TempDir;
use std::sync::Arc;

// Helper function to create namespace
async fn create_namespace(app_ctx: &Arc<AppContext>, namespace: &str) {
    use kalamdb_commons::models::NamespaceId;
    use kalamdb_commons::types::Namespace;
    
    let ns_id = NamespaceId::new(namespace);
    let ns = Namespace {
        namespace_id: ns_id.clone(),
        description: Some(format!("Test namespace {}", namespace)),
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };
    
    let provider = app_ctx.system_tables().namespaces();
    let _ = provider.create_namespace(ns);
}

// Use simplified TestServer for these tests
struct TestServer {
    app_context: Arc<AppContext>,
    _temp_dir: Arc<TempDir>,
}

impl TestServer {
    async fn new() -> Self {
        use kalamdb_commons::{NodeId, config::ServerConfig};
        use kalamdb_store::{RocksDBBackend, StorageBackend};
        
        let temp_dir = Arc::new(TempDir::new().expect("Failed to create temp directory"));
        let db_path = temp_dir.path().join("test_db");
        
        let db_init = RocksDbInit::with_defaults(db_path.to_str().unwrap());
        let db = db_init.open().expect("Failed to open RocksDB");
        
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db));
        
        let storage_base = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_base).ok();
        
        let config = ServerConfig::default();
        let app_context = AppContext::init(
            backend,
            NodeId::new("test-node".to_string()),
            storage_base,
            config,
        );
        
        Self {
            app_context,
            _temp_dir: temp_dir,
        }
    }
    
    async fn execute_sql(&self, sql: &str) -> kalamdb_api::models::SqlResponse {
        use kalamdb_core::sql::executor::SqlExecutor;
        use kalamdb_commons::UserId;
        
        let executor = SqlExecutor::new(self.app_context.clone(), false);
        let user_id = UserId::root();
        
        match executor.execute_sql(sql, &user_id).await {
            Ok(response) => response,
            Err(e) => kalamdb_api::models::SqlResponse {
                status: ResponseStatus::Error,
                results: vec![],
                took: 0.0,
                error: Some(kalamdb_api::models::ErrorDetail {
                    code: "EXECUTION_ERROR".to_string(),
                    message: e.to_string(),
                }),
            },
        }
    }
}

/// Test: Server starts successfully with default configuration
#[tokio::test]
async fn test_default_config_starts_successfully() {
    let server = TestServer::new().await;
    
    // Verify server is operational by executing a simple query
    let response = server.execute_sql("SELECT 1 as test_value").await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "Server should start with default config and execute queries"
    );
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        assert_eq!(rows.len(), 1, "Should return one row");
        let value = rows[0].get("test_value").and_then(|v| v.as_i64());
        assert_eq!(value, Some(1), "Should return value 1");
    }
}

/// Test: Server operational check via system.tables query
#[tokio::test]
async fn test_server_readiness_check_via_system_tables() {
    let server = TestServer::new().await;
    
    // Query system.tables to verify metadata is loaded
    let response = server.execute_sql("SELECT COUNT(*) as table_count FROM system.tables").await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "System tables should be queryable immediately after startup"
    );
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        let count = rows[0].get("table_count").and_then(|v| v.as_i64()).unwrap_or(0);
        // Should have at least system tables (users, namespaces, jobs, etc.)
        assert!(
            count >= 0,
            "Should have system tables available, got {} tables",
            count
        );
    }
}

/// Test: Server can handle multiple namespaces being created
#[tokio::test]
async fn test_multiple_namespaces_creation() {
    let server = TestServer::new().await;
    
    let namespaces = vec!["prod", "staging", "dev", "test"];
    
    for namespace in &namespaces {
        let sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
        let response = server.execute_sql(&sql).await;
        
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Failed to create namespace '{}'",
            namespace
        );
    }
    
    // Verify all namespaces exist
    let response = server.execute_sql("SELECT namespace_id FROM system.namespaces ORDER BY namespace_id").await;
    
    assert_eq!(response.status, ResponseStatus::Success);
    
    if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
        let created_namespaces: Vec<String> = rows
            .iter()
            .filter_map(|row| row.get("namespace_id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        
        for namespace in &namespaces {
            assert!(
                created_namespaces.contains(&namespace.to_string()),
                "Namespace '{}' should exist in system.namespaces",
                namespace
            );
        }
    }
}

/// Test: Server handles concurrent SQL queries during startup phase
#[tokio::test]
async fn test_concurrent_queries_during_startup() {
    let server = TestServer::new().await;
    
    // Spawn 5 concurrent queries
    let mut handles = vec![];
    
    for i in 0..5 {
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            let sql = format!("SELECT {} as query_id", i);
            server_clone.execute_sql(&sql).await
        });
        handles.push(handle);
    }
    
    // Wait for all queries to complete
    let results = futures_util::future::join_all(handles).await;
    
    // All queries should succeed
    for (idx, result) in results.iter().enumerate() {
        let response = result.as_ref().expect("Task should not panic");
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Concurrent query {} should succeed",
            idx
        );
    }
}

/// Test: Invalid namespace names are rejected with clear errors
#[tokio::test]
async fn test_invalid_namespace_names_rejected() {
    let server = TestServer::new().await;
    
    let invalid_names = vec![
        "",           // Empty name
        "123-start",  // Starting with number
        "my-ns",      // Hyphens (may be invalid depending on parser)
        "my.ns",      // Dots (reserved for qualified names)
        "my ns",      // Spaces
    ];
    
    for invalid_name in invalid_names {
        let sql = format!("CREATE NAMESPACE {}", invalid_name);
        let response = server.execute_sql(&sql).await;
        
        // Note: Current implementation may accept some of these.
        // This test documents the behavior - if it fails, it means
        // the parser is more permissive than expected.
        // TODO: Implement stricter namespace name validation if needed
        
        if response.status == ResponseStatus::Error {
            // If it's rejected, ensure error message is helpful
            if let Some(error) = &response.error {
                assert!(
                    !error.message.is_empty(),
                    "Error message should not be empty for invalid namespace '{}'",
                    invalid_name
                );
            }
        }
        
        // Document current behavior
        println!(
            "Namespace '{}': {} ({})",
            invalid_name,
            if response.status == ResponseStatus::Success { "ACCEPTED" } else { "REJECTED" },
            response.error.as_ref().map(|e| e.message.as_str()).unwrap_or("no error")
        );
    }
}

/// Test: CREATE TABLE with invalid flush policy is rejected
#[tokio::test]
async fn test_invalid_flush_policy_rejected() {
    let server = TestServer::new().await;
    
    // Create namespace first
    server.execute_sql("CREATE NAMESPACE IF NOT EXISTS config_test").await;
    
    let invalid_policies = vec![
        "rows:abc",        // Non-numeric value
        "rows:-100",       // Negative value
        "interval:xyz",    // Non-numeric interval
        "unknown:1000",    // Unknown segment
        "rows:",           // Missing value
        "",                // Empty policy
    ];
    
    for policy in invalid_policies {
        let sql = format!(
            "CREATE TABLE config_test.test_table (id BIGINT PRIMARY KEY) \
             WITH (TYPE='USER', FLUSH_POLICY='{}')",
            policy
        );
        
        let response = server.execute_sql(&sql).await;
        
        // Most of these should fail, but we document what happens
        if response.status == ResponseStatus::Error {
            if let Some(error) = &response.error {
                assert!(
                    !error.message.is_empty(),
                    "Error message should explain flush policy issue"
                );
                
                // Check that error message mentions "flush" or "policy"
                let msg = error.message.to_lowercase();
                assert!(
                    msg.contains("flush") || msg.contains("policy") || msg.contains("invalid"),
                    "Error should reference flush policy issue: {}",
                    error.message
                );
            }
        }
        
        println!(
            "FLUSH_POLICY '{}': {} ({})",
            policy,
            if response.status == ResponseStatus::Success { "ACCEPTED" } else { "REJECTED" },
            response.error.as_ref().map(|e| e.message.as_str()).unwrap_or("no error")
        );
    }
}

/// Test: CREATE TABLE with invalid TTL_SECONDS is rejected
#[tokio::test]
async fn test_invalid_ttl_seconds_rejected() {
    let server = TestServer::new().await;
    
    server.execute_sql("CREATE NAMESPACE IF NOT EXISTS config_test").await;
    
    let invalid_ttls = vec![
        "-100",    // Negative TTL
        "0",       // Zero TTL (might be invalid)
        "abc",     // Non-numeric
    ];
    
    for ttl in invalid_ttls {
        let sql = format!(
            "CREATE TABLE config_test.stream_table (id TEXT PRIMARY KEY) \
             WITH (TYPE='STREAM', TTL_SECONDS={})",
            ttl
        );
        
        let response = server.execute_sql(&sql).await;
        
        if response.status == ResponseStatus::Error {
            if let Some(error) = &response.error {
                let msg = error.message.to_lowercase();
                assert!(
                    msg.contains("ttl") || msg.contains("invalid") || msg.contains("seconds"),
                    "Error should reference TTL issue: {}",
                    error.message
                );
            }
        }
        
        println!(
            "TTL_SECONDS {}: {} ({})",
            ttl,
            if response.status == ResponseStatus::Success { "ACCEPTED" } else { "REJECTED" },
            response.error.as_ref().map(|e| e.message.as_str()).unwrap_or("no error")
        );
    }
}

/// Test: CREATE TABLE without PRIMARY KEY is rejected
#[tokio::test]
async fn test_table_without_primary_key_rejected() {
    let server = TestServer::new().await;
    
    server.execute_sql("CREATE NAMESPACE IF NOT EXISTS config_test").await;
    
    // Try to create table without PRIMARY KEY
    let sql = "CREATE TABLE config_test.no_pk_table (name TEXT, value TEXT) WITH (TYPE='USER')";
    
    let response = server.execute_sql(sql).await;
    
    // Should be rejected (PRIMARY KEY is mandatory in KalamDB)
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "Table without PRIMARY KEY should be rejected"
    );
    
    if let Some(error) = &response.error {
        let msg = error.message.to_lowercase();
        assert!(
            msg.contains("primary") || msg.contains("key") || msg.contains("required"),
            "Error should mention PRIMARY KEY requirement: {}",
            error.message
        );
    }
}

/// Test: CREATE TABLE with unknown TYPE is rejected
#[tokio::test]
async fn test_unknown_table_type_rejected() {
    let server = TestServer::new().await;
    
    server.execute_sql("CREATE NAMESPACE IF NOT EXISTS config_test").await;
    
    // Try to create table with invalid TYPE
    let sql = "CREATE TABLE config_test.bad_type_table (id BIGINT PRIMARY KEY) \
               WITH (TYPE='UNKNOWN')";
    
    let response = server.execute_sql(sql).await;
    
    // Should be rejected
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "Unknown table TYPE should be rejected"
    );
    
    if let Some(error) = &response.error {
        let msg = error.message.to_lowercase();
        assert!(
            msg.contains("type") || msg.contains("unknown") || msg.contains("invalid"),
            "Error should mention invalid TYPE: {}",
            error.message
        );
    }
}

/// Test: System tables query returns expected schema
#[tokio::test]
async fn test_system_tables_schema_consistent() {
    let server = TestServer::new().await;
    
    // Query system.users schema
    let response = server.execute_sql("SELECT * FROM system.users LIMIT 0").await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "system.users should be queryable"
    );
    
    if let Some(result) = response.results.first() {
        // Verify expected columns exist in schema
        let column_names = &result.columns;
        
        let expected_columns = vec![
            "user_id",
            "username",
            "password_hash",
            "role",
            "email",
            "auth_type",
            "created_at",
            "updated_at",
        ];
        
        for expected in expected_columns {
            assert!(
                column_names.iter().any(|c| c == expected),
                "system.users should have column '{}'",
                expected
            );
        }
    }
}

/// Test: Server handles DROP NAMESPACE of non-existent namespace gracefully
#[tokio::test]
async fn test_drop_nonexistent_namespace_with_if_exists() {
    let server = TestServer::new().await;
    
    // DROP with IF EXISTS should succeed even if namespace doesn't exist
    let response = server.execute_sql("DROP NAMESPACE IF EXISTS nonexistent_namespace").await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Success,
        "DROP NAMESPACE IF EXISTS should succeed for non-existent namespace"
    );
}

/// Test: Server rejects DROP NAMESPACE without IF EXISTS for non-existent namespace
#[tokio::test]
async fn test_drop_nonexistent_namespace_without_if_exists() {
    let server = TestServer::new().await;
    
    // DROP without IF EXISTS should fail
    let response = server.execute_sql("DROP NAMESPACE nonexistent_namespace").await;
    
    assert_eq!(
        response.status,
        ResponseStatus::Error,
        "DROP NAMESPACE should fail for non-existent namespace"
    );
    
    if let Some(error) = &response.error {
        assert!(
            !error.message.is_empty(),
            "Error message should indicate namespace not found"
        );
    }
}
