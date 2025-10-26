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

pub mod stress_utils;
pub mod flush_helpers;

use anyhow::Result;
use datafusion::catalog::SchemaProvider;
use kalamdb_api::models::{QueryResult, SqlResponse};
use kalamdb_core::live_query::{LiveQueryManager, NodeId};
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use std::sync::Arc;
use tempfile::TempDir;

/// HTTP test server wrapper - simplified to avoid complex type signatures
pub struct HttpTestServer {
    /// Note: We store the TestServer and recreate services on each test call
    /// This avoids complex generic type parameters
    pub _env: TestServer,
}

impl HttpTestServer {
    /// Execute a request against the test server
    pub async fn execute_request(
        &self,
        req: actix_web::test::TestRequest,
    ) -> actix_web::dev::ServiceResponse {
        use actix_web::{test, App};
        use jsonwebtoken::Algorithm;
        use kalamdb_api::auth::jwt::JwtAuth;
        use kalamdb_api::rate_limiter::RateLimiter;
        use std::sync::Arc;

        let jwt_auth = Arc::new(JwtAuth::new(
            "kalamdb-dev-secret-key-change-in-production".to_string(),
            Algorithm::HS256,
        ));
        let rate_limiter = Arc::new(RateLimiter::new());

        let app = test::init_service(
            App::new()
                .app_data(actix_web::web::Data::new(self._env.session_factory.clone()))
                .app_data(actix_web::web::Data::new(self._env.sql_executor.clone()))
                .app_data(actix_web::web::Data::new(
                    self._env.live_query_manager.clone(),
                ))
                .app_data(actix_web::web::Data::new(jwt_auth))
                .app_data(actix_web::web::Data::new(rate_limiter))
                .configure(kalamdb_api::routes::configure_routes),
        )
        .await;

        let req = req.to_request();
        test::call_service(&app, req).await
    }

    /// Convenience delegation to inner TestServer::execute_sql
    pub async fn execute_sql(&self, sql: &str) -> SqlResponse {
        self._env.execute_sql(sql).await
    }

    /// Execute SQL as a specific user via the global REST wiring.
    pub async fn execute_sql_as_user(&self, sql: &str, user_id: &str) -> SqlResponse {
        self._env.execute_sql_as_user(sql, user_id).await
    }

    /// Execute SQL with optional user context.
    pub async fn execute_sql_with_user(&self, sql: &str, user_id: Option<&str>) -> SqlResponse {
        self._env.execute_sql_with_user(sql, user_id).await
    }

    /// Check if a namespace exists in the underlying TestServer.
    pub async fn namespace_exists(&self, namespace: &str) -> bool {
        self._env.namespace_exists(namespace).await
    }

    /// Check if a table exists in the underlying TestServer.
    pub async fn table_exists(&self, namespace: &str, table: &str) -> bool {
        self._env.table_exists(namespace, table).await
    }
}

/// Spin up an Actix test application wired with KalamDB services.
pub async fn start_test_server() -> HttpTestServer {
    // Reuse internal TestServer utilities for database + executor setup
    let env = TestServer::new().await;
    HttpTestServer { _env: env }
}

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
    /// Temporary directory for database files (shared via Arc to allow cloning)
    pub temp_dir: Arc<TempDir>,
    /// RocksDB instance (needed for direct store access in tests)
    pub db: Arc<rocksdb::DB>,
    /// KalamSQL instance for direct database access
    pub kalam_sql: Arc<kalamdb_sql::KalamSql>,
    /// SQL executor for query execution
    pub sql_executor: Arc<SqlExecutor>,
    /// Namespace service for namespace operations
    pub namespace_service: Arc<NamespaceService>,
    /// DataFusion session factory (needed for fallback SQL execution)
    pub session_factory: Arc<DataFusionSessionFactory>,
    /// Live query manager for WebSocket coordination
    pub live_query_manager: Arc<LiveQueryManager>,
}

impl Clone for TestServer {
    fn clone(&self) -> Self {
        Self {
            temp_dir: Arc::clone(&self.temp_dir),
            db: Arc::clone(&self.db),
            kalam_sql: Arc::clone(&self.kalam_sql),
            sql_executor: Arc::clone(&self.sql_executor),
            namespace_service: Arc::clone(&self.namespace_service),
            session_factory: Arc::clone(&self.session_factory),
            live_query_manager: Arc::clone(&self.live_query_manager),
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

        // Create default 'local' storage if system.storages is empty
        let storages = kalam_sql
            .scan_all_storages()
            .expect("Failed to scan storages");
        if storages.is_empty() {
            let now = chrono::Utc::now().timestamp_millis();
            let default_storage = kalamdb_sql::Storage {
                storage_id: "local".to_string(),
                storage_name: "Local Filesystem".to_string(),
                description: Some("Default local filesystem storage".to_string()),
                storage_type: "filesystem".to_string(),
                base_directory: "".to_string(),
                credentials: None,
                shared_tables_template: "{namespace}/{tableName}".to_string(),
                user_tables_template: "{namespace}/{tableName}/{userId}".to_string(),
                created_at: now,
                updated_at: now,
            };
            kalam_sql
                .insert_storage(&default_storage)
                .expect("Failed to create default storage");
        }

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

        // Create "system" schema in DataFusion and register system table providers
        use datafusion::catalog::schema::MemorySchemaProvider;

        let system_schema = Arc::new(MemorySchemaProvider::new());
        let catalog_name = "kalam";

        session_context
            .catalog(catalog_name)
            .expect("Failed to get kalam catalog")
            .register_schema("system", system_schema.clone())
            .expect("Failed to register system schema");

        // Register all system tables using centralized function
        let jobs_provider = kalamdb_core::system_table_registration::register_system_tables(
            &system_schema,
            kalam_sql.clone(),
        )
        .expect("Failed to register system tables");

        // Initialize StorageRegistry for template validation
        let storage_registry = Arc::new(kalamdb_core::storage::StorageRegistry::new(
            kalam_sql.clone(),
        ));

        // Initialize JobManager for FLUSH TABLE support
        let job_manager = Arc::new(kalamdb_core::jobs::TokioJobManager::new());

        // Initialize live query manager
        let live_query_manager = Arc::new(LiveQueryManager::new(
            kalam_sql.clone(),
            NodeId::new("test-node".to_string()),
            Some(user_table_store.clone()),
            Some(shared_table_store.clone()),
            Some(stream_table_store.clone()),
        ));

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
            .with_storage_registry(storage_registry)
            .with_job_manager(job_manager)
            .with_live_query_manager(live_query_manager.clone())
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
            temp_dir: Arc::new(temp_dir),
            db,
            kalam_sql,
            sql_executor,
            namespace_service,
            session_factory,
            live_query_manager,
        }
    }

    /// Backwards-compatible helper returning a ready TestServer instance.
    pub async fn start() -> Self {
        Self::new().await
    }

    /// Alias maintained for older tests expecting TestServer::start_test_server().
    pub async fn start_test_server() -> Self {
        Self::start().await
    }

    /// Alias maintained for websocket integration tests.
    pub async fn start_http_server_for_websocket_tests() -> (Self, String) {
        start_http_server_for_websocket_tests().await
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

        let is_admin = user_id_obj
            .as_ref()
            .map(|id| {
                let lower = id.as_ref().to_lowercase();
                lower == "admin" || lower == "system"
            })
            .unwrap_or(false);
        let mask_credentials = !is_admin;

        // Try custom DDL/DML execution first (same as REST API)
        match self.sql_executor.execute(sql, user_id_obj.as_ref()).await {
            Ok(result) => {
                use kalamdb_core::sql::ExecutionResult;
                match result {
                    ExecutionResult::Success(msg) => SqlResponse {
                        status: "success".to_string(),
                        results: vec![QueryResult {
                            rows: None,
                            row_count: 0,
                            columns: vec![],
                            message: Some(msg),
                        }],
                        took_ms: 0,
                        error: None,
                    },
                    ExecutionResult::RecordBatch(batch) => {
                        // Convert single batch to JSON
                        let query_result = record_batch_to_query_result(&batch, mask_credentials);
                        SqlResponse {
                            status: "success".to_string(),
                            results: vec![query_result],
                            took_ms: 0,
                            error: None,
                        }
                    }
                    ExecutionResult::RecordBatches(batches) => {
                        // Convert multiple batches to JSON
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
                    ExecutionResult::Subscription(subscription_data) => {
                        // Convert subscription metadata to SqlResponse
                        let mut row = std::collections::HashMap::new();
                        if let serde_json::Value::Object(map) = subscription_data {
                            for (key, value) in map {
                                row.insert(key, value);
                            }
                        }
                        let query_result = QueryResult {
                            rows: Some(vec![row]),
                            row_count: 1,
                            columns: vec![
                                "status".to_string(),
                                "ws_url".to_string(),
                                "subscription".to_string(),
                                "message".to_string(),
                            ],
                            message: None,
                        };
                        SqlResponse {
                            status: "success".to_string(),
                            results: vec![query_result],
                            took_ms: 0,
                            error: None,
                        }
                    }
                }
            }
            Err(kalamdb_core::error::KalamDbError::InvalidSql(_)) => {
                // Not a custom DDL, fall back to DataFusion (same as REST API)
                // This ensures integration tests use the SAME code path as /v1/api/sql
                match self.session_factory.create_session().sql(sql).await {
                    Ok(df) => match df.collect().await {
                        Ok(batches) => {
                            if batches.is_empty() {
                                SqlResponse {
                                    status: "success".to_string(),
                                    results: vec![],
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

/// Start a real HTTP server on a random available port for WebSocket testing.
///
/// Returns the TestServer instance and the base URL of the server.
/// The server runs in a background task and will be automatically stopped when dropped.
///
/// # Example
///
/// ```no_run
/// let (server, base_url) = start_http_server_for_websocket_tests().await;
/// let ws_url = format!("{}/v1/ws", base_url.replace("http", "ws"));
/// // Connect WebSocket and run tests...
/// ```
pub async fn start_http_server_for_websocket_tests() -> (TestServer, String) {
    use actix_web::{web, App, HttpServer};
    use jsonwebtoken::Algorithm;
    use kalamdb_api::auth::jwt::JwtAuth;
    use kalamdb_api::rate_limiter::RateLimiter;
    use std::sync::Arc;

    let server = TestServer::new().await;

    let session_factory = server.session_factory.clone();
    let sql_executor = server.sql_executor.clone();
    let live_query_manager = server.live_query_manager.clone();

    let jwt_auth = Arc::new(JwtAuth::new(
        "kalamdb-dev-secret-key-change-in-production".to_string(),
        Algorithm::HS256,
    ));
    let rate_limiter = Arc::new(RateLimiter::new());

    // Bind to an ephemeral port to avoid collisions between tests
    let http_server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .app_data(web::Data::new(live_query_manager.clone()))
            .app_data(web::Data::new(jwt_auth.clone()))
            .app_data(web::Data::new(rate_limiter.clone()))
            .configure(kalamdb_api::routes::configure_routes)
    })
    .bind(("127.0.0.1", 0))
    .expect("Failed to bind to ephemeral port");

    let bound_addrs = http_server.addrs().to_vec();
    let server_future = http_server.run();

    // Spawn server in background
    tokio::spawn(server_future);

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let addr = bound_addrs
        .get(0)
        .expect("Expected at least one listening address");
    let base_url = format!("http://{}:{}", addr.ip(), addr.port());

    (server, base_url)
}

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
