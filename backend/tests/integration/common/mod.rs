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
use kalamdb_api::models::{QueryResult, ResponseStatus, SqlResponse};
use kalamdb_commons::models::{NamespaceId, StorageId, TableName};
use kalamdb_commons::UserId;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

pub mod fixtures;
pub mod websocket;

/// Extension trait for QueryResult to provide test-friendly row access
pub trait QueryResultTestExt {
    /// Get a row as a HashMap by row index (converts array format to map)
    fn row_as_map(&self, row_idx: usize) -> Option<HashMap<String, serde_json::Value>>;
    
    /// Get all rows as HashMaps (converts array format to maps)
    fn rows_as_maps(&self) -> Vec<HashMap<String, serde_json::Value>>;
}

impl QueryResultTestExt for QueryResult {
    fn row_as_map(&self, row_idx: usize) -> Option<HashMap<String, serde_json::Value>> {
        let row = self.rows.as_ref()?.get(row_idx)?;
        let mut map = HashMap::new();
        for (i, field) in self.schema.iter().enumerate() {
            if let Some(value) = row.get(i) {
                map.insert(field.name.clone(), value.clone());
            }
        }
        Some(map)
    }
    
    fn rows_as_maps(&self) -> Vec<HashMap<String, serde_json::Value>> {
        let Some(rows) = &self.rows else { return vec![] };
        rows.iter()
            .enumerate()
            .filter_map(|(i, _)| self.row_as_map(i))
            .collect()
    }
}

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
        Self::new_with_options(data_dir, false).await
    }

    /// Create a new test server with custom options.
    pub async fn new_with_options(
        data_dir: Option<String>,
        enforce_password_complexity: bool,
    ) -> Self {
        use kalamdb_commons::{AuthType, NodeId, Role, StorageMode, UserId};
        use once_cell::sync::Lazy;
        use std::sync::Mutex;

        // Global shared test resources (initialized once for all tests)
        static TEST_RESOURCES: Lazy<Mutex<Option<(Arc<TempDir>, Arc<rocksdb::DB>, String)>>> =
            Lazy::new(|| Mutex::new(None));

        // Get or create shared test resources
        let (temp_dir, db, storage_base_path) = {
            let mut resources = TEST_RESOURCES.lock().unwrap();
            if let Some((ref td, ref db, ref path)) = *resources {
                (td.clone(), db.clone(), path.clone())
            } else {
                // First test: initialize shared resources
                let temp_dir = Arc::new(TempDir::new().expect("Failed to create temp directory"));
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
                let db_init = kalamdb_store::RocksDbInit::with_defaults(&db_path);
                let db_arc = db_init.open().expect("Failed to open RocksDB");

                let storage_path_str = storage_base_path.to_str().unwrap().to_string();

                *resources = Some((temp_dir.clone(), db_arc.clone(), storage_path_str.clone()));
                (temp_dir, db_arc, storage_path_str)
            }
        };

        // Initialize StorageBackend
        let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db.clone()));

        // Create minimal test config
        let mut test_config = kalamdb_commons::config::ServerConfig::default();
        // Align JWT settings with integration test expectations
        test_config.auth.jwt_secret = "test-secret-key-for-jwt-authentication".to_string();
        test_config.auth.jwt_trusted_issuers = "kalamdb-test".to_string();
        // Expose to extractor via env for JWT path (non-invasive)
        std::env::set_var("KALAMDB_JWT_SECRET", &test_config.auth.jwt_secret);
        std::env::set_var(
            "KALAMDB_JWT_TRUSTED_ISSUERS",
            &test_config.auth.jwt_trusted_issuers,
        );
        test_config.server.node_id = "test-node".to_string();
        test_config.storage.default_storage_path = storage_base_path.clone();

        // Initialize AppContext using singleton pattern (only once for all tests)
        // All tests in the same process will share both AppContext AND RocksDB
        let app_context = AppContext::init(
            backend.clone(),
            NodeId::new("test-node".to_string()),
            storage_base_path.clone(),
            test_config,
        );

        // Get session context from AppContext
        let session_context = app_context.base_session_context();

        // Bootstrap: ensure a default 'system' user exists for admin operations in tests (idempotent)
        {
            use kalamdb_commons::constants::AuthConstants;
            use kalamdb_commons::types::User;
            let sys_id = UserId::root();
            let users_provider = app_context.system_tables().users();

            // Only create if doesn't exist
            if users_provider
                .get_user_by_id(&sys_id)
                .ok()
                .flatten()
                .is_none()
            {
                let now = chrono::Utc::now().timestamp_millis();
                // Create a valid bcrypt hash for testing
                let password_hash = bcrypt::hash("admin", bcrypt::DEFAULT_COST).unwrap_or_default();

                let sys_user = User {
                    id: sys_id.clone(),
                    username: AuthConstants::DEFAULT_SYSTEM_USERNAME.into(),
                    password_hash,
                    role: Role::System,
                    email: Some("system@localhost".to_string()),
                    auth_type: AuthType::Internal,
                    auth_data: None,
                    storage_mode: StorageMode::Table,
                    storage_id: None,
                    failed_login_attempts: 0,
                    locked_until: None,
                    last_login_at: None,
                    created_at: now,
                    updated_at: now,
                    last_seen: None,
                    deleted_at: None,
                };
                let _ = users_provider.create_user(sys_user);
            }
        }

        // Create default 'local' storage if system.storages is empty (idempotent)
        {
            use kalamdb_commons::types::Storage;
            let storages_provider = app_context.system_tables().storages();
            let storage_id = StorageId::new("local");

            // Only create if doesn't exist
            if storages_provider
                .get_storage_by_id(&storage_id)
                .ok()
                .flatten()
                .is_none()
            {
                let now = chrono::Utc::now().timestamp_millis();
                let default_storage = Storage {
                    storage_id: storage_id.clone(),
                    storage_name: "Local Filesystem".to_string(),
                    description: Some("Default local filesystem storage".to_string()),
                    storage_type: kalamdb_commons::models::StorageType::Filesystem,
                    base_directory: storage_base_path.clone(),
                    credentials: None,
                    config_json: None,
                    shared_tables_template: "shared/{namespace}/{tableName}".to_string(),
                    user_tables_template: "users/{userId}/tables/{namespace}/{tableName}"
                        .to_string(),
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
            enforce_password_complexity,
        ));

        // Ensure all existing tables are registered with providers/DataFusion for this process
        if let Err(e) = sql_executor.load_existing_tables().await {
            eprintln!(
                "Warning: Failed to load existing table providers in test harness: {:?}",
                e
            );
        }

        // Start background job processing loop so queued jobs (e.g., FLUSH TABLE) run in tests
        {
            let jm = app_context.job_manager();
            tokio::spawn(async move {
                // Best-effort: run until test process exits
                let _ = jm.run_loop(2).await; // small concurrency is fine for tests
            });
        }

        Self {
            temp_dir,
            storage_base_path: Arc::new(PathBuf::from(storage_base_path)),
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
                user_id_obj = Some(UserId::root());
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
        use kalamdb_commons::Role;
        use kalamdb_core::sql::executor::models::ExecutionContext;

        let session = self.app_context.base_session_context();

        // Look up user's actual role from the users table
        let exec_ctx = match &user_id_obj {
            Some(user_id) => {
                // Try to get user's actual role from system tables
                let role = self
                    .app_context
                    .system_tables()
                    .users()
                    .get_user_by_id(user_id)
                    .ok()
                    .flatten()
                    .map(|user| user.role)
                    .unwrap_or_else(|| {
                        // For test convenience: auto-escalate based on user ID patterns
                        let id_lower = user_id.as_str().to_lowercase();
                        if id_lower == "system"
                            || id_lower == "admin"
                            || id_lower == "root"
                            || id_lower.starts_with("e2e_")
                        {
                            Role::System
                        } else if id_lower.contains("dba") {
                            Role::Dba
                        } else if id_lower.contains("service") || id_lower.starts_with("svc") {
                            Role::Service
                        } else {
                            Role::User
                        }
                    });

                ExecutionContext::new(user_id.clone(), role, session.clone())
            }
            None => ExecutionContext::new(UserId::root(), Role::System, session),
        };

        match self.sql_executor.execute(sql, &exec_ctx, Vec::new()).await {
            Ok(result) => {
                println!(
                    "[DEBUG TestServer] execute() succeeded, result variant: {:?}",
                    std::mem::discriminant(&result)
                );
                use kalamdb_core::sql::ExecutionResult;
                match result {
                    ExecutionResult::Success { message } => SqlResponse {
                        status: ResponseStatus::Success,
                        results: vec![QueryResult {
                            schema: vec![],
                            rows: None,
                            row_count: 0,
                            message: Some(message),
                        }],
                        took: 0.0,
                        error: None,
                    },
                    ExecutionResult::Rows { batches, .. } => {
                        if batches.is_empty() {
                            SqlResponse {
                                status: ResponseStatus::Success,
                                results: vec![QueryResult {
                                    schema: vec![],
                                    rows: Some(vec![]),
                                    row_count: 0,
                                    message: None,
                                }],
                                took: 0.0,
                                error: None,
                            }
                        } else {
                            let results: Vec<_> = batches
                                .iter()
                                .map(|batch| record_batch_to_query_result(batch, mask_credentials))
                                .collect();
                            SqlResponse {
                                status: ResponseStatus::Success,
                                results,
                                took: 0.0,
                                error: None,
                            }
                        }
                    }
                    ExecutionResult::Subscription {
                        subscription_id,
                        channel,
                        select_query: _,
                    } => {
                        use kalamdb_commons::models::datatypes::KalamDataType;
                        use kalamdb_commons::schemas::SchemaField;
                        let row = vec![
                            serde_json::Value::String(subscription_id),
                            serde_json::Value::String(channel),
                        ];
                        let query_result = QueryResult {
                            schema: vec![
                                SchemaField::new("subscription_id", KalamDataType::Text, 0),
                                SchemaField::new("channel", KalamDataType::Text, 1),
                            ],
                            rows: Some(vec![row]),
                            row_count: 1,
                            message: None,
                        };
                        SqlResponse {
                            status: ResponseStatus::Success,
                            results: vec![query_result],
                            took: 0.0,
                            error: None,
                        }
                    }
                    ExecutionResult::Inserted { rows_affected }
                    | ExecutionResult::Updated { rows_affected }
                    | ExecutionResult::Deleted { rows_affected } => {
                        println!(
                            "[DEBUG TestServer] Matched DML variant with rows_affected={}",
                            rows_affected
                        );
                        SqlResponse {
                            status: ResponseStatus::Success,
                            results: vec![QueryResult {
                                schema: vec![],
                                rows: None,
                                row_count: rows_affected,
                                message: Some(format!("{} row(s) affected", rows_affected)),
                            }],
                            took: 0.0,
                            error: None,
                        }
                    }
                    ExecutionResult::Flushed {
                        tables,
                        bytes_written,
                    } => {
                        use kalamdb_commons::models::datatypes::KalamDataType;
                        use kalamdb_commons::schemas::SchemaField;
                        SqlResponse {
                            status: ResponseStatus::Success,
                            results: vec![QueryResult {
                                schema: vec![
                                    SchemaField::new("tables", KalamDataType::Text, 0),
                                    SchemaField::new("bytes_written", KalamDataType::BigInt, 1),
                                ],
                                rows: Some(vec![vec![
                                    serde_json::Value::String(tables.join(",")),
                                    serde_json::Value::Number(bytes_written.into()),
                                ]]),
                                row_count: tables.len(),
                                message: None,
                            }],
                            took: 0.0,
                            error: None,
                        }
                    }
                    ExecutionResult::JobKilled { job_id, status } => {
                        use kalamdb_commons::models::datatypes::KalamDataType;
                        use kalamdb_commons::schemas::SchemaField;
                        SqlResponse {
                            status: ResponseStatus::Success,
                            results: vec![QueryResult {
                                schema: vec![
                                    SchemaField::new("job_id", KalamDataType::Text, 0),
                                    SchemaField::new("status", KalamDataType::Text, 1),
                                ],
                                rows: Some(vec![vec![
                                    serde_json::Value::String(job_id),
                                    serde_json::Value::String(status),
                                ]]),
                                row_count: 1,
                                message: None,
                            }],
                            took: 0.0,
                            error: None,
                        }
                    }
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
                                    status: ResponseStatus::Success,
                                    results: vec![QueryResult {
                                        schema: vec![],
                                        rows: Some(vec![]),
                                        row_count: 0,
                                        message: None,
                                    }],
                                    took: 0.0,
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
                                    status: ResponseStatus::Success,
                                    results,
                                    took: 0.0,
                                    error: None,
                                }
                            }
                        }
                        Err(e) => SqlResponse {
                            status: ResponseStatus::Error,
                            results: vec![],
                            took: 0.0,
                            error: Some(kalamdb_api::models::ErrorDetail {
                                code: "SQL_EXECUTION_ERROR".to_string(),
                                message: format!("Error executing query: {}", e),
                                details: Some(sql.to_string()),
                            }),
                        },
                    },
                    Err(e) => SqlResponse {
                        status: ResponseStatus::Error,
                        results: vec![],
                        took: 0.0,
                        error: Some(kalamdb_api::models::ErrorDetail {
                            code: "SQL_PARSE_ERROR".to_string(),
                            message: format!("Error parsing SQL: {}", e),
                            details: Some(sql.to_string()),
                        }),
                    },
                }
            }
            Err(e) => {
                eprintln!("[DEBUG TestServer] execute() error: {:?}", e);
                SqlResponse {
                    status: ResponseStatus::Error,
                    results: vec![],
                    took: 0.0,
                    error: Some(kalamdb_api::models::ErrorDetail {
                        code: "EXECUTION_ERROR".to_string(),
                        message: format!("{:?}", e),
                        details: None,
                    }),
                }
            }
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
    use kalamdb_commons::models::datatypes::arrow_conversion::FromArrowType;
    use kalamdb_commons::models::datatypes::KalamDataType;
    use kalamdb_commons::schemas::SchemaField;

    let arrow_schema = batch.schema();
    let num_rows = batch.num_rows();
    
    // Build schema with KalamDataType
    let schema: Vec<SchemaField> = arrow_schema
        .fields()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let kalam_type = field
                .metadata()
                .get("kalam_data_type")
                .and_then(|s| serde_json::from_str::<KalamDataType>(s).ok())
                .or_else(|| KalamDataType::from_arrow_type(field.data_type()).ok())
                .unwrap_or(KalamDataType::Text);
            SchemaField::new(field.name().clone(), kalam_type, index)
        })
        .collect();

    let mut rows: Vec<Vec<serde_json::Value>> = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let mut row_values: Vec<serde_json::Value> = Vec::with_capacity(arrow_schema.fields().len());

        for (col_idx, field) in arrow_schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);
            let mut value = match field.data_type() {
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
                DataType::Timestamp(unit, _tz) => {
                    match unit {
                        datafusion::arrow::datatypes::TimeUnit::Millisecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampMicrosecondArray>()
                                .unwrap();
                            if array.is_null(row_idx) {
                                serde_json::Value::Null
                            } else {
                                serde_json::Value::Number(array.value(row_idx).into())
                            }
                        }
                        datafusion::arrow::datatypes::TimeUnit::Microsecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampMicrosecondArray>()
                                .unwrap();
                            if array.is_null(row_idx) {
                                serde_json::Value::Null
                            } else {
                                // Preserve value in microseconds since epoch
                                serde_json::Value::Number(array.value(row_idx).into())
                            }
                        }
                        datafusion::arrow::datatypes::TimeUnit::Nanosecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampNanosecondArray>()
                                .unwrap();
                            if array.is_null(row_idx) {
                                serde_json::Value::Null
                            } else {
                                serde_json::Value::Number(array.value(row_idx).into())
                            }
                        }
                        _ => serde_json::Value::String(format!("{:?}", column)),
                    }
                }
                _ => serde_json::Value::String(format!("{:?}", column)),
            };

            // Mask credentials column if requested
            if mask_credentials && field.name() == "credentials" && !value.is_null() {
                value = serde_json::Value::String("***".to_string());
            }

            row_values.push(value);
        }

        rows.push(row_values);
    }

    kalamdb_api::models::QueryResult {
        schema,
        rows: Some(rows),
        row_count: num_rows,
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

        // Only drop namespaces with the dedicated cleanup prefix to avoid cross-test interference.
        for ns in namespaces {
            let name = ns.namespace_id.as_str();
            if !name.starts_with("cleanup_") {
                continue;
            }
            let sql = format!("DROP NAMESPACE {} CASCADE", ns.namespace_id);
            let response = self.execute_sql(&sql).await;
            if response.status != kalamdb_api::models::ResponseStatus::Success {
                eprintln!(
                    "Warning: Failed to drop namespace {}: {:?}",
                    ns.namespace_id, response.error
                );
            }
        }

        self.cleanup_storages().await
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
            if response.status != kalamdb_api::models::ResponseStatus::Success {
                eprintln!(
                    "Warning: Failed to drop table {}.{}: {:?}",
                    namespace, table.table_name, response.error
                );
            }
        }

        Ok(())
    }

    /// Remove all non-default storage configurations created during tests.
    pub async fn cleanup_storages(&self) -> Result<()> {
        let storages_provider = self.app_context.system_tables().storages();
        let storages = storages_provider
            .list_storages()
            .map_err(|e| anyhow::anyhow!("Failed to list storages: {:?}", e))?;

        for storage in storages {
            if storage.storage_id.is_local() {
                continue;
            }
            let sql = format!("DROP STORAGE {}", storage.storage_id.as_str());
            let response = self.execute_sql(&sql).await;
            if response.status != ResponseStatus::Success {
                eprintln!(
                    "Warning: Failed to drop storage {}: {:?}",
                    storage.storage_id.as_str(),
                    response.error
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
            Ok(namespaces) => namespaces
                .iter()
                .any(|ns| ns.namespace_id.as_str() == namespace),
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

    /// Get a CoreUsersRepo instance connected to the server's user table
    pub fn users_repo(&self) -> Arc<dyn kalamdb_auth::UserRepository> {
        Arc::new(kalamdb_api::repositories::CoreUsersRepo::new(
            self.app_context.system_tables().users(),
        ))
    }

    /// Helper to create a test user
    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: kalamdb_commons::Role,
    ) -> kalamdb_commons::UserId {
        use kalamdb_commons::{AuthType, StorageMode, UserId};

        let user_id = UserId::new(username);
        let password_hash =
            bcrypt::hash(password, bcrypt::DEFAULT_COST).expect("Failed to hash password");

        let user = kalamdb_commons::system::User {
            id: user_id.clone(),
            username: username.into(),
            password_hash,
            role,
            email: Some(format!("{}@test.com", username)),
            auth_type: AuthType::Password,
            auth_data: None,
            storage_mode: StorageMode::Table,
            storage_id: None,
            failed_login_attempts: 0,
            locked_until: None,
            last_login_at: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
            last_seen: None,
            deleted_at: None,
        };

        // Ignore error if user already exists (idempotent)
        let _ = self.app_context.system_tables().users().create_user(user);

        user_id
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
        let response = server
            .execute_sql("CREATE NAMESPACE IF NOT EXISTS test_ns")
            .await;
        assert_eq!(response.status, ResponseStatus::Success);
    }

    #[actix_web::test]
    async fn test_cleanup() {
        let server = TestServer::new().await;

        // Use unique namespace to avoid test interference
        let ns = format!("cleanup_ns_{}", std::process::id());

        // Create namespace
        server
            .execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns))
            .await;
        assert!(server.namespace_exists(&ns).await);

        // Cleanup
        server.cleanup().await.unwrap();
        assert!(!server.namespace_exists(&ns).await);
    }

    #[actix_web::test]
    async fn test_namespace_exists() {
        let server = TestServer::new().await;

        // Use unique namespace to avoid test interference
        let ns = format!("test_ns_exists_{}", std::process::id());

        assert!(!server.namespace_exists("nonexistent").await);

        server
            .execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns))
            .await;
        assert!(server.namespace_exists(&ns).await);

        // Cleanup
        server
            .execute_sql(&format!("DROP NAMESPACE {} CASCADE", ns))
            .await;
    }
}

// =============================================================================
// Common Test Helpers - Shared utilities for cleanup job waiting and path checking
// =============================================================================

/// Wait for a cleanup job to complete
pub async fn wait_for_cleanup_job_completion(
    server: &TestServer,
    job_id: &str,
    max_wait: std::time::Duration,
) -> Result<String, String> {
    use tokio::time::{sleep, Duration};
    
    let start = std::time::Instant::now();
    let check_interval = Duration::from_millis(200);

    loop {
        if start.elapsed() > max_wait {
            return Err(format!(
                "Timeout waiting for cleanup job {} to complete after {:?}",
                job_id, max_wait
            ));
        }

        let query = format!(
            "SELECT status, result, error_message FROM system.jobs WHERE job_id = '{}'",
            job_id
        );

        let response = server.execute_sql(&query).await;

        if response.status != ResponseStatus::Success {
            // system.jobs might not be accessible in some test setups
            sleep(max_wait).await;
            return Ok("Job executed (system.jobs not queryable in test)".to_string());
        }

        if let Some(rows) = response.results.first().and_then(|r| r.rows.as_ref()) {
            if rows.is_empty() {
                sleep(check_interval).await;
                continue;
            }

            if let Some(row) = rows.first() {
                let status = row.first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                match status {
                    "new" | "queued" | "retrying" | "running" => {
                        sleep(check_interval).await;
                        continue;
                    }
                    "completed" => {
                        let result = row.get(1)
                            .and_then(|v| v.as_str())
                            .unwrap_or("completed");
                        return Ok(result.to_string());
                    }
                    "failed" | "cancelled" => {
                        let error = row.get(2)
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error");
                        return Err(format!("Cleanup job {}: {}", status, error));
                    }
                    _ => {
                        return Err(format!("Unknown job status: {}", status));
                    }
                }
            }
        }

        sleep(check_interval).await;
    }
}

/// Extract cleanup job ID from DROP TABLE response message
pub fn extract_cleanup_job_id(message: &str) -> Option<String> {
    // Message format: "Table ns.table dropped successfully. Cleanup job: CL-xxxxxxxx"
    message.split("Cleanup job: ")
        .nth(1)
        .map(|s| s.trim().to_string())
}

/// Wait for a path to be removed from filesystem (for cleanup verification)
pub async fn wait_for_path_absent(path: &std::path::Path, timeout: std::time::Duration) -> bool {
    use tokio::time::{sleep, Duration, Instant};
    
    let deadline = Instant::now() + timeout;
    while path.exists() {
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(50)).await;
    }
    true
}
