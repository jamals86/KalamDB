#![allow(dead_code)]
//! Common test utilities for KalamDB integration tests.
//!
//! This module provides a comprehensive test harness for integration testing:
//! - Test server startup and shutdown
//! - Test database initialization and cleanup
//! - Common fixtures for namespaces and tables
//! - WebSocket connection helpers
//! - Assertion utilities for validating results
//! - Stress testing utilities for concurrent load generation
//!
//! # Architecture
//!
//! The test framework is organized into three main components:
//! 1. **Server Management**: Start/stop test server instances
//! 2. **Fixtures**: Create test data (namespaces, tables, sample data)
//! 3. **WebSocket Utilities**: Connect, subscribe, and validate live queries
//! 4. **Stress Testing**: Concurrent writers, subscribers, and resource monitors
//!
//! # Usage
//!
//! ```no_run
//! use integration::common::{TestServer, fixtures};
//!
//! #[actix_web::test]
//! async fn test_example() {
//!     let server = TestServer::new().await;
//!     
//!     // Create test namespace
//!     fixtures::create_namespace(&server, "test_ns").await;
//!     
//!     // Run your test...
//!     
//!     server.cleanup().await;
//! }
//! ```

pub mod auth_helper;
pub mod flush_helpers;
pub mod stress_utils;

use anyhow::Result;
use kalamdb_api::models::{QueryResult, SqlResponse};
use kalamdb_commons::UserId;
use kalamdb_commons::models::{NamespaceId, StorageId, TableName};
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

pub mod fixtures;
pub mod websocket;

/// Test server instance with full KalamDB stack.
///
/// This struct manages a complete test environment including:
/// - Temporary RocksDB database
/// - AppContext with all services
/// - DataFusion session factory
/// - SQL executor
///
/// The server is automatically cleaned up when dropped.
pub struct TestServer {
    /// Temporary directory for database files (shared via Arc to allow cloning)
    pub temp_dir: Arc<TempDir>,
    /// Base directory for table storage (Parquet outputs)
    pub storage_base_path: Arc<PathBuf>,
    /// Underlying RocksDB handle for tests that need direct access
    pub db: Arc<rocksdb::DB>,
    /// Shared DataFusion session context used by SqlExecutor (providers registered here)
    pub session_context: Arc<datafusion::prelude::SessionContext>,
    /// SQL executor for query execution
    pub sql_executor: Arc<SqlExecutor>,
    /// AppContext singleton with all services
    pub app_context: Arc<AppContext>,
}

impl Clone for TestServer {
    fn clone(&self) -> Self {
        Self {
            temp_dir: Arc::clone(&self.temp_dir),
            storage_base_path: Arc::clone(&self.storage_base_path),
            db: Arc::clone(&self.db),
            session_context: Arc::clone(&self.session_context),
            sql_executor: Arc::clone(&self.sql_executor),
            app_context: Arc::clone(&self.app_context),
        }
    }
}

impl TestServer {
    /// Create a new test server with a fresh database.
    ///
    /// This initializes a complete KalamDB stack in-memory with:
    /// - Fresh RocksDB instance in temporary directory
    /// - All system tables (users, namespaces, live_queries, etc.)
    /// - DataFusion session factory
    /// - REST API handlers
    ///
    /// # Example
    ///
    /// ```no_run
    /// use integration::common::TestServer;
    ///
    /// #[actix_web::test]
    /// async fn test_example() {
    ///     let server = TestServer::new().await;
    ///     // Run your test...
    /// }
    /// ```
    pub async fn new() -> Self {
        Self::new_with_data_dir(None).await
    }

    /// Create a new test server with a specific data directory.
    ///
    /// Useful for testing persistence or using pre-populated databases.
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Optional path to existing database directory
    pub async fn new_with_data_dir(data_dir: Option<String>) -> Self {
        use kalamdb_commons::{AuthType, NodeId, Role, StorageMode, UserId};
        
        // Create temporary database
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = data_dir.unwrap_or_else(|| {
            temp_dir
                .path()
                .join("test_db")
                .to_str()
                .unwrap()
                .to_string()
        });

        // Prepare storage base directory for Parquet outputs
        let storage_base_path = temp_dir.path().join("storage");
        std::fs::create_dir_all(&storage_base_path)
            .expect("Failed to create storage base directory for tests");

        // Initialize RocksDB with all system tables
        let db_init = kalamdb_store::RocksDbInit::new(&db_path);
        let db = db_init.open().expect("Failed to open RocksDB");

        // Initialize StorageBackend
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));

        // Initialize AppContext with Phase 10 pattern (3 parameters)
        let app_context = AppContext::init(
            backend.clone(),
            NodeId::new("test-node".to_string()),
            storage_base_path.to_str().unwrap().to_string(),
        );

        // Get session context from AppContext
        let session_context = app_context.base_session_context();

        // Bootstrap: ensure a default 'system' user exists for admin operations in tests
        {
            use kalamdb_commons::types::User;
            let sys_id = UserId::new("system");
            let now = chrono::Utc::now().timestamp_millis();
            let sys_user = User {
                id: sys_id.clone(),
                username: "system".into(),
                password_hash: String::new(),
                role: Role::System,
                email: Some("system@localhost".to_string()),
                auth_type: AuthType::Internal,
                auth_data: None,
                storage_mode: StorageMode::Table,
                storage_id: None,
                created_at: now,
                updated_at: now,
                last_seen: None,
                deleted_at: None,
            };
                        // Insert via system tables provider
            let _ = app_context.system_tables().users().create_user(sys_user);
        }

        // Create default 'local' storage if system.storages is empty
        {
            use kalamdb_commons::types::Storage;
            let storages_provider = app_context.system_tables().storages();
            let storages = storages_provider.list_storages().expect("Failed to scan storages");
            if storages.is_empty() {
                let now = chrono::Utc::now().timestamp_millis();
                let default_storage = Storage {
                    storage_id: StorageId::new("local"),
                    storage_name: "Local Filesystem".to_string(),
                    description: Some("Default local filesystem storage".to_string()),
                    storage_type: "filesystem".to_string(),
                    base_directory: storage_base_path.to_str().unwrap().to_string(),
                    credentials: None,
                    shared_tables_template: "shared/{namespace}/{tableName}".to_string(),
                    user_tables_template: "users/{userId}/tables/{namespace}/{tableName}".to_string(),
                    created_at: now,
                    updated_at: now,
                };
                storages_provider
                    .insert_storage(default_storage)
                    .expect("Failed to create default storage");
            }
        }

        // Initialize SqlExecutor with new pattern (Phase 10: takes app_context + enforce_password_complexity)
        let sql_executor = Arc::new(SqlExecutor::new(
            app_context.clone(),
            false, // disable password complexity enforcement in tests
        ));

        Self {
            temp_dir: Arc::new(temp_dir),
            storage_base_path: Arc::new(storage_base_path),
            db,
            session_context,
            sql_executor,
            app_context,
        }
    }

    /// Create a fully-initialized test server (alias for `new()`).
    ///
    /// Flush-specific tests use this to emphasise background jobs would normally be started.
    pub async fn start_test_server() -> Self {
        Self::new().await
    }

    /// Execute SQL and return the response.
    ///
    /// This is a convenience method for testing SQL execution directly via the executor.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL statement to execute
    ///
    /// # Returns
    ///
    /// `SqlResponse` containing status, results, and any errors
    ///
    /// # Example
    ///
    /// ```no_run
    /// let response = server.execute_sql("CREATE NAMESPACE test_ns").await;
    /// assert_eq!(response.status, "success");
    /// ```
    pub async fn execute_sql(&self, sql: &str) -> SqlResponse {
        self.execute_sql_with_user(sql, None).await
    }

    /// Execute SQL as a specific user (convenience wrapper).
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL statement to execute
    /// * `user_id` - User ID string
    ///
    /// # Returns
    ///
    /// `SqlResponse` containing status, results, and any errors
    ///
    /// # Example
    ///
    /// ```no_run
    /// let response = server.execute_sql_as_user(
    ///     "INSERT INTO test_ns.notes (content) VALUES ('Hello')",
    ///     "user123"
    /// ).await;
    /// ```
    pub async fn execute_sql_as_user(&self, sql: &str, user_id: &str) -> SqlResponse {
        self.execute_sql_with_user(sql, Some(user_id)).await
    }

    /// Execute SQL with optional user_id context and return the response.
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL statement to execute
    /// * `user_id` - Optional user_id for USER table operations
    ///
    /// # Returns
    ///
    /// `SqlResponse` containing status, results, and any errors
    pub async fn execute_sql_with_user(&self, sql: &str, user_id: Option<&str>) -> SqlResponse {
        let mut user_id_obj = user_id.map(UserId::from);

        // Auto-escalate to 'system' for admin-only namespace DDL when no user provided
        if user_id_obj.is_none() {
            let sql_u = sql.trim().to_uppercase();
            if sql_u.starts_with("CREATE NAMESPACE")
                || sql_u.starts_with("ALTER NAMESPACE")
                || sql_u.starts_with("DROP NAMESPACE")
            {
                user_id_obj = Some(UserId::system());
            }
        }

        let is_admin: bool = match user_id_obj.as_ref() {
            Some(id) => {
                let lower = id.as_str().to_lowercase();
                lower == "admin" || lower == "system"
            }
            None => false,
        };
        let mask_credentials = !is_admin;

        // Try custom DDL/DML execution first (same as REST API)
        // Phase 10: execute() now requires ExecutionContext instead of Option<&UserId>
    use kalamdb_core::sql::executor::models::ExecutionContext;
        use kalamdb_commons::Role;
        
        let exec_ctx = match &user_id_obj {
            Some(user_id) => ExecutionContext::new(user_id.clone(), Role::System),
            None => ExecutionContext::new(UserId::system(), Role::System),
        };
        
    match self.sql_executor.execute(&*self.session_context, sql, &exec_ctx, Vec::new()).await {
        Ok(result) => {
            use kalamdb_core::sql::ExecutionResult;
            match result {
                ExecutionResult::Success { message } => SqlResponse {
                    status: "success".to_string(),
                    results: vec![QueryResult {
                        rows: None,
                        row_count: 0,
                        columns: vec![],
                        message: Some(message),
                    }],
                    took_ms: 0,
                    error: None,
                },
                ExecutionResult::Rows { batches, .. } => {
                    if batches.is_empty() {
                        SqlResponse {
                            status: "success".to_string(),
                            results: vec![QueryResult {
                                rows: Some(vec![]),
                                row_count: 0,
                                columns: vec![],
                                message: None,
                            }],
                            took_ms: 0,
                            error: None,
                        }
                    } else {
                        let results: Vec<_> = batches
                            .iter()
                            .map(|batch| record_batch_to_query_result(batch, mask_credentials))
                            .collect();
                        SqlResponse {
                            status: "success".to_string(),
                            results,
                            took_ms: 0,
                            error: None,
                        }
                    }
                }
                ExecutionResult::Subscription { subscription_id, channel } => {
                    let mut row = std::collections::HashMap::new();
                    row.insert("subscription_id".to_string(), serde_json::Value::String(subscription_id));
                    row.insert("channel".to_string(), serde_json::Value::String(channel));
                    let query_result = QueryResult {
                        rows: Some(vec![row]),
                        row_count: 1,
                        columns: vec!["subscription_id".to_string(), "channel".to_string()],
                        message: None,
                    };
                    SqlResponse {
                        status: "success".to_string(),
                        results: vec![query_result],
                        took_ms: 0,
                        error: None,
                    }
                }
                ExecutionResult::Inserted { rows_affected }
                | ExecutionResult::Updated { rows_affected }
                | ExecutionResult::Deleted { rows_affected } => SqlResponse {
                    status: "success".to_string(),
                    results: vec![QueryResult {
                        rows: None,
                        row_count: rows_affected,
                        columns: vec![],
                        message: Some(format!("{} row(s) affected", rows_affected)),
                    }],
                    took_ms: 0,
                    error: None,
                },
                ExecutionResult::Flushed { tables, bytes_written } => SqlResponse {
                    status: "success".to_string(),
                    results: vec![QueryResult {
                        rows: Some(vec![{
                            let mut m = std::collections::HashMap::new();
                            m.insert("tables".to_string(), serde_json::Value::String(tables.join(",")));
                            m.insert("bytes_written".to_string(), serde_json::Value::Number(bytes_written.into()));
                            m
                        }]),
                        row_count: tables.len(),
                        columns: vec!["tables".to_string(), "bytes_written".to_string()],
                        message: None,
                    }],
                    took_ms: 0,
                    error: None,
                },
                ExecutionResult::JobKilled { job_id, status } => SqlResponse {
                    status: "success".to_string(),
                    results: vec![QueryResult {
                        rows: Some(vec![{
                            let mut m = std::collections::HashMap::new();
                            m.insert("job_id".to_string(), serde_json::Value::String(job_id));
                            m.insert("status".to_string(), serde_json::Value::String(status));
                            m
                        }]),
                        row_count: 1,
                        columns: vec!["job_id".to_string(), "status".to_string()],
                        message: None,
                    }],
                    took_ms: 0,
                    error: None,
                },
            }
        }
            Err(kalamdb_core::error::KalamDbError::InvalidSql(_)) => {
                // Any error from custom executor: fall back to DataFusion using the shared session_context
                // where providers are registered by SqlExecutor.
                match self.session_context.sql(sql).await {
                    Ok(df) => match df.collect().await {
                        Ok(batches) => {
                            if batches.is_empty() {
                                // Return empty result with 0 rows instead of empty results array
                                SqlResponse {
                                    status: "success".to_string(),
                                    results: vec![QueryResult {
                                        rows: Some(vec![]),
                                        row_count: 0,
                                        columns: vec![],
                                        message: None,
                                    }],
                                    took_ms: 0,
                                    error: None,
                                }
                            } else {
                                let results: Vec<_> = batches
                                    .iter()
                                    .map(|batch| {
                                        record_batch_to_query_result(batch, mask_credentials)
                                    })
                                    .collect();
                                SqlResponse {
                                    status: "success".to_string(),
                                    results,
                                    took_ms: 0,
                                    error: None,
                                }
                            }
                        }
                        Err(e) => SqlResponse {
                            status: "error".to_string(),
                            results: vec![],
                            took_ms: 0,
                            error: Some(kalamdb_api::models::ErrorDetail {
                                code: "SQL_EXECUTION_ERROR".to_string(),
                                message: format!("Error executing query: {}", e),
                                details: Some(sql.to_string()),
                            }),
                        },
                    },
                    Err(e) => SqlResponse {
                        status: "error".to_string(),
                        results: vec![],
                        took_ms: 0,
                        error: Some(kalamdb_api::models::ErrorDetail {
                            code: "SQL_PARSE_ERROR".to_string(),
                            message: format!("Error parsing SQL: {}", e),
                            details: Some(sql.to_string()),
                        }),
                    },
                }
            }
            Err(e) => SqlResponse {
                status: "error".to_string(),
                results: vec![],
                took_ms: 0,
                error: Some(kalamdb_api::models::ErrorDetail {
                    code: "EXECUTION_ERROR".to_string(),
                    message: format!("{:?}", e),
                    details: None,
                }),
            },
        }
    }
}

/// Convert Arrow RecordBatch to QueryResult for JSON response
fn record_batch_to_query_result(
    batch: &datafusion::arrow::record_batch::RecordBatch,
    mask_credentials: bool,
) -> kalamdb_api::models::QueryResult {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::DataType;
    use std::collections::HashMap;

    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let mut rows: Vec<HashMap<String, serde_json::Value>> = Vec::with_capacity(num_rows);

    // Extract column names
    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();

    for row_idx in 0..num_rows {
        let mut row_map = HashMap::new();

        for (col_idx, field) in schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);
            let value = match field.data_type() {
                DataType::Utf8 => {
                    let array = column.as_any().downcast_ref::<StringArray>().unwrap();
                    if array.is_null(row_idx) {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::String(array.value(row_idx).to_string())
                    }
                }
                DataType::Int32 => {
                    let array = column.as_any().downcast_ref::<Int32Array>().unwrap();
                    if array.is_null(row_idx) {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::Number(array.value(row_idx).into())
                    }
                }
                DataType::Int64 => {
                    let array = column.as_any().downcast_ref::<Int64Array>().unwrap();
                    if array.is_null(row_idx) {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::Number(array.value(row_idx).into())
                    }
                }
                DataType::Float64 => {
                    let array = column.as_any().downcast_ref::<Float64Array>().unwrap();
                    if array.is_null(row_idx) {
                        serde_json::Value::Null
                    } else {
                        serde_json::Number::from_f64(array.value(row_idx))
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null)
                    }
                }
                DataType::Boolean => {
                    let array = column.as_any().downcast_ref::<BooleanArray>().unwrap();
                    if array.is_null(row_idx) {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::Bool(array.value(row_idx))
                    }
                }
                DataType::Timestamp(_, _) => {
                    let array = column
                        .as_any()
                        .downcast_ref::<TimestampMillisecondArray>()
                        .unwrap();
                    if array.is_null(row_idx) {
                        serde_json::Value::Null
                    } else {
                        serde_json::Value::Number(array.value(row_idx).into())
                    }
                }
                _ => serde_json::Value::String(format!("{:?}", column)),
            };

            row_map.insert(field.name().clone(), value);
        }

        if mask_credentials {
            if let Some(value) = row_map.get_mut("credentials") {
                if !value.is_null() {
                    *value = serde_json::Value::String("***".to_string());
                }
            }
        }

        rows.push(row_map);
    }

    kalamdb_api::models::QueryResult {
        rows: Some(rows),
        row_count: num_rows,
        columns,
        message: None,
    }
}

impl TestServer {
    /// Cleanup all test namespaces and tables.
    ///
    /// This drops all non-system namespaces and their tables.
    /// System tables (users, live_queries, etc.) are preserved.
    pub async fn cleanup(&self) -> Result<()> {
        // Get all namespaces from system tables
        let namespaces = self
            .app_context
            .system_tables()
            .namespaces()
            .scan_all()
            .map_err(|e| anyhow::anyhow!("Failed to list namespaces: {:?}", e))?;

        // Drop each namespace (this will cascade delete tables)
        for ns in namespaces {
            let sql = format!("DROP NAMESPACE {} CASCADE", ns.namespace_id);
            let response = self.execute_sql(&sql).await;
            if response.status != "success" {
                eprintln!(
                    "Warning: Failed to drop namespace {}: {:?}",
                    ns.namespace_id, response.error
                );
            }
        }

        Ok(())
    }

    /// Drop all tables in a specific namespace.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Name of the namespace
    pub async fn cleanup_namespace(&self, namespace: &str) -> Result<()> {
        // Get all tables from system tables
        let tables = self
            .app_context
            .system_tables()
            .tables()
            .scan_all()
            .map_err(|e| anyhow::anyhow!("Failed to list tables: {:?}", e))?;

        // Filter tables in this namespace
        let ns_tables: Vec<_> = tables
            .into_iter()
            .filter(|t| t.namespace_id == NamespaceId::new(namespace))
            .collect();

        // Drop each table
        for table in ns_tables {
            let sql = format!("DROP TABLE {}.{}", namespace, table.table_name);
            let response = self.execute_sql(&sql).await;
            if response.status != "success" {
                eprintln!(
                    "Warning: Failed to drop table {}.{}: {:?}",
                    namespace, table.table_name, response.error
                );
            }
        }

        Ok(())
    }

    /// Get the database path.
    pub fn db_path(&self) -> String {
        self.temp_dir
            .path()
            .join("test_db")
            .to_str()
            .unwrap()
            .to_string()
    }

    /// Base directory where table storage (Parquet files) is written.
    pub fn storage_root(&self) -> PathBuf {
        (*self.storage_base_path).clone()
    }

    /// Check if a namespace exists.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Name of the namespace to check
    pub async fn namespace_exists(&self, namespace: &str) -> bool {
        match self.app_context.system_tables().namespaces().scan_all() {
            Ok(namespaces) => namespaces.iter().any(|ns| ns.namespace_id.as_str() == namespace),
            Err(_) => false,
        }
    }

    /// Check if a table exists.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Name of the namespace
    /// * `table_name` - Name of the table
    pub async fn table_exists(&self, namespace: &str, table_name: &str) -> bool {
        match self.app_context.system_tables().tables().scan_all() {
            Ok(tables) => tables.iter().any(|t| {
                t.namespace_id == NamespaceId::new(namespace)
                    && t.table_name == TableName::new(table_name)
            }),
            Err(_) => false,
        }
    }
}

/// Start a real HTTP server on a random available port for WebSocket testing.
///
/// Returns the TestServer instance and the base URL of the server.
/// The server runs in a background task and will be automatically stopped when dropped.
///
/// # Example
/// Create a test JWT token for WebSocket authentication
///
/// # Arguments
///
/// * `user_id` - User ID for the token
/// * `secret` - JWT secret (should match server configuration)
/// * `exp_seconds` - Token expiration in seconds from now
///
/// # Example
///
/// ```no_run
/// let token = create_test_jwt("user1", "kalamdb-dev-secret-key-change-in-production", 3600);
/// let ws = WebSocketClient::connect_with_auth(&ws_url, Some(&token)).await.unwrap();
/// ```
pub fn create_test_jwt(user_id: &str, secret: &str, exp_seconds: i64) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        iat: u64,
        exp: u64,
        user_id: String,
        roles: Vec<String>,
    }

    let now = chrono::Utc::now().timestamp() as u64;
    let claims = Claims {
        sub: user_id.to_string(),
        iat: now,
        exp: (now as i64 + exp_seconds) as u64,
        user_id: user_id.to_string(),
        roles: vec!["user".to_string()],
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to create JWT token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn test_server_creation() {
        let server = TestServer::new().await;
        assert!(!server.db_path().is_empty());
    }

    #[actix_web::test]
    async fn test_execute_sql() {
        let server = TestServer::new().await;
        let response = server.execute_sql("CREATE NAMESPACE test_ns").await;
        assert_eq!(response.status, "success");
    }

    #[actix_web::test]
    async fn test_cleanup() {
        let server = TestServer::new().await;

        // Create namespace
        server.execute_sql("CREATE NAMESPACE test_ns").await;
        assert!(server.namespace_exists("test_ns").await);

        // Cleanup
        server.cleanup().await.unwrap();
        assert!(!server.namespace_exists("test_ns").await);
    }

    #[actix_web::test]
    async fn test_namespace_exists() {
        let server = TestServer::new().await;

        assert!(!server.namespace_exists("nonexistent").await);

        server.execute_sql("CREATE NAMESPACE test_ns").await;
        assert!(server.namespace_exists("test_ns").await);
    }
}
