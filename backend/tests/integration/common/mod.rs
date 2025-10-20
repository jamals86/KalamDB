//! Common test utilities for KalamDB integration tests.
//!
//! This module provides a comprehensive test harness for integration testing:
//! - Test server startup and shutdown
//! - Test database initialization and cleanup
//! - Common fixtures for namespaces and tables
//! - WebSocket connection helpers
//! - Assertion utilities for validating results
//!
//! # Architecture
//!
//! The test framework is organized into three main components:
//! 1. **Server Management**: Start/stop test server instances
//! 2. **Fixtures**: Create test data (namespaces, tables, sample data)
//! 3. **WebSocket Utilities**: Connect, subscribe, and validate live queries
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

use anyhow::Result;
use kalamdb_api::models::SqlResponse;
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use std::sync::Arc;
use tempfile::TempDir;

pub mod fixtures;
pub mod websocket;

/// Test server instance with full KalamDB stack.
///
/// This struct manages a complete test environment including:
/// - Temporary RocksDB database
/// - KalamSQL instance
/// - DataFusion session factory
/// - SQL executor
///
/// The server is automatically cleaned up when dropped.
pub struct TestServer {
    /// Temporary directory for database files
    temp_dir: TempDir,
    /// KalamSQL instance for direct database access
    pub kalam_sql: Arc<kalamdb_sql::KalamSql>,
    /// SQL executor for query execution
    pub sql_executor: Arc<SqlExecutor>,
    /// Namespace service for namespace operations
    pub namespace_service: Arc<NamespaceService>,
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

        // Initialize RocksDB with all system tables
        let db_init = kalamdb_core::storage::RocksDbInit::new(&db_path);
        let db = db_init.open().expect("Failed to open RocksDB");

        // Initialize KalamSQL
        let kalam_sql =
            Arc::new(kalamdb_sql::KalamSql::new(db.clone()).expect("Failed to create KalamSQL"));

        // Initialize stores (needed by some services)
        let user_table_store = Arc::new(
            kalamdb_store::UserTableStore::new(db.clone())
                .expect("Failed to create UserTableStore"),
        );
        let shared_table_store = Arc::new(
            kalamdb_store::SharedTableStore::new(db.clone())
                .expect("Failed to create SharedTableStore"),
        );
        let stream_table_store = Arc::new(
            kalamdb_store::StreamTableStore::new(db.clone())
                .expect("Failed to create StreamTableStore"),
        );

        // Initialize services
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
        let user_table_service = Arc::new(UserTableService::new(
            kalam_sql.clone(),
            user_table_store.clone(),
        ));
        let shared_table_service = Arc::new(SharedTableService::new(
            shared_table_store.clone(),
            kalam_sql.clone(),
        ));
        let stream_table_service = Arc::new(StreamTableService::new(
            stream_table_store.clone(),
            kalam_sql.clone(),
        ));

        // Initialize TableDeletionService for DROP TABLE support
        let table_deletion_service = Arc::new(TableDeletionService::new(
            user_table_store.clone(),
            shared_table_store.clone(),
            stream_table_store.clone(),
            kalam_sql.clone(),
        ));

        // Initialize DataFusion session factory
        let session_factory = Arc::new(
            DataFusionSessionFactory::new().expect("Failed to create DataFusion session factory"),
        );

        // Create session context
        let session_context = Arc::new(session_factory.create_session());

        // Initialize SqlExecutor with builder pattern
        let sql_executor = Arc::new(
            SqlExecutor::new(
                namespace_service.clone(),
                session_context.clone(),
                user_table_service.clone(),
                shared_table_service.clone(),
                stream_table_service.clone(),
            )
            .with_table_deletion_service(table_deletion_service)
            .with_stores(
                user_table_store.clone(),
                shared_table_store.clone(),
                stream_table_store.clone(),
                kalam_sql.clone(),
            ),
        );

        // Load existing tables from system_tables
        let default_user_id = kalamdb_core::catalog::UserId::from("system");
        sql_executor
            .load_existing_tables(default_user_id)
            .await
            .expect("Failed to load existing tables");

        Self {
            temp_dir,
            kalam_sql,
            sql_executor,
            namespace_service,
        }
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
        let user_id_obj = user_id.map(kalamdb_core::catalog::UserId::from);
        match self.sql_executor.execute(sql, user_id_obj.as_ref()).await {
            Ok(result) => {
                use kalamdb_core::sql::ExecutionResult;
                match result {
                    ExecutionResult::Success(_msg) => SqlResponse {
                        status: "success".to_string(),
                        results: vec![],
                        execution_time_ms: 0,
                        error: None,
                    },
                    ExecutionResult::RecordBatch(batch) => {
                        // Convert single batch to JSON
                        let query_result = record_batch_to_query_result(&batch);
                        SqlResponse {
                            status: "success".to_string(),
                            results: vec![query_result],
                            execution_time_ms: 0,
                            error: None,
                        }
                    }
                    ExecutionResult::RecordBatches(batches) => {
                        // Convert multiple batches to JSON
                        let results: Vec<_> =
                            batches.iter().map(record_batch_to_query_result).collect();
                        SqlResponse {
                            status: "success".to_string(),
                            results,
                            execution_time_ms: 0,
                            error: None,
                        }
                    }
                }
            }
            Err(e) => SqlResponse {
                status: "error".to_string(),
                results: vec![],
                execution_time_ms: 0,
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
        // Get all namespaces
        let namespaces = self
            .kalam_sql
            .scan_all_namespaces()
            .map_err(|e| anyhow::anyhow!("Failed to list namespaces: {:?}", e))?;

        // Drop each namespace (this will cascade delete tables)
        for ns in namespaces {
            let sql = format!("DROP NAMESPACE {} CASCADE", ns.name);
            let response = self.execute_sql(&sql).await;
            if response.status != "success" {
                eprintln!(
                    "Warning: Failed to drop namespace {}: {:?}",
                    ns.name, response.error
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
        // Get all tables in namespace
        let tables = self
            .kalam_sql
            .scan_all_tables()
            .map_err(|e| anyhow::anyhow!("Failed to list tables: {:?}", e))?;

        // Filter tables in this namespace
        let ns_tables: Vec<_> = tables
            .into_iter()
            .filter(|t| t.namespace == namespace)
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

    /// Check if a namespace exists.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Name of the namespace to check
    pub async fn namespace_exists(&self, namespace: &str) -> bool {
        match self.kalam_sql.scan_all_namespaces() {
            Ok(namespaces) => namespaces.iter().any(|ns| ns.name == namespace),
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
        match self.kalam_sql.scan_all_tables() {
            Ok(tables) => tables
                .iter()
                .any(|t| t.namespace == namespace && t.table_name == table_name),
            Err(_) => false,
        }
    }
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
