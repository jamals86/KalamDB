//! Test helpers for initializing AppContext with minimal dependencies
//!
//! This module provides utilities for setting up AppContext in unit tests.

use datafusion::prelude::SessionContext;
use kalamdb_commons::models::{NamespaceId, NodeId, StorageId};
use kalamdb_core::app_context::AppContext;
use kalamdb_core::jobs::executors::{
    BackupExecutor, CleanupExecutor, CompactExecutor, FlushExecutor, JobRegistry, RestoreExecutor,
    RetentionExecutor, StreamEvictionExecutor, UserCleanupExecutor,
};
use kalamdb_store::test_utils::TestDb;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::sync::Once;

static TEST_DB: OnceCell<Arc<TestDb>> = OnceCell::new();
static TEST_RUNTIME: OnceCell<Arc<tokio::runtime::Runtime>> = OnceCell::new();
static INIT: Once = Once::new();
static BOOTSTRAP_INIT: Once = Once::new();

/// Initialize AppContext with minimal test dependencies
///
/// Uses AppContext::init() with 4 parameters including test config.
/// Thread-safe: uses Once to ensure single initialization.
///
/// # Example
/// ```no_run
/// use kalamdb_core::test_helpers::init_test_app_context;
///
/// #[test]
/// fn my_test() {
///     init_test_app_context();
///     // Now AppContext::get() will work
/// }
/// ```
pub fn init_test_app_context() -> Arc<TestDb> {
    // Use Once to ensure we only initialize once across all tests
    INIT.call_once(|| {
        // Create test database with all required column families
        let test_db = Arc::new(
            TestDb::new(&[
                "system_tables",
                "system_audit_log",
                "system_namespaces",
                "system_storages",
                "system_users",
                "system_jobs",
                "system_live_queries",
                "information_schema_tables",
                "shared_table:app:config",
                "stream_table:app:events",
            ])
            .unwrap(),
        );

        // Store in static for reuse
        TEST_DB.set(test_db.clone()).ok();

        let storage_backend: Arc<dyn StorageBackend> =
            Arc::new(RocksDBBackend::new(test_db.db.clone()));

        // Create minimal test config using Default + overrides
        let mut test_config = kalamdb_commons::config::ServerConfig::default();
        test_config.storage.data_path = "data".to_string();
        test_config.execution.max_parameters = 50;
        test_config.execution.max_parameter_size_bytes = 512 * 1024;

        // Phase 5: Simple 4-parameter initialization with config!
        // Uses constants from kalamdb_commons for table prefixes
        AppContext::init(
            storage_backend,
            NodeId::new(1), // test node ID
            "data/storage".to_string(),
            test_config,
        );
    });

    BOOTSTRAP_INIT.call_once(|| {
        let app_ctx = AppContext::get();
        let executor = app_ctx.executor();

        let rt = TEST_RUNTIME
            .get_or_init(|| Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime")))
            .clone();

        let (tx, rx) = std::sync::mpsc::channel();
        rt.spawn(async move {
            let result = async {
                executor.start().await.map_err(|e| format!("start raft: {e}"))?;
                executor
                    .initialize_cluster()
                    .await
                    .map_err(|e| format!("initialize single-node raft: {e}"))?;
                Ok::<(), String>(())
            }
            .await;
            let _ = tx.send(result);
        });

        rx.recv()
            .expect("raft bootstrap result")
            .expect("raft bootstrap should succeed");

        let app_ctx = AppContext::get();

        // Ensure default namespace exists
        let namespaces = app_ctx.system_tables().namespaces();
        let default_namespace = NamespaceId::new("default");
        if namespaces.get_namespace_by_id(&default_namespace).unwrap().is_none() {
            namespaces
                .create_namespace(kalamdb_commons::system::Namespace {
                    namespace_id: default_namespace,
                    name: "default".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    options: Some("{}".to_string()),
                    table_count: 0,
                })
                .unwrap();
        }

        // Ensure default local storage exists
        let storages = app_ctx.system_tables().storages();
        let storage_id = StorageId::from("local");
        if storages.get_storage_by_id(&storage_id).unwrap().is_none() {
            storages
                .create_storage(kalamdb_commons::system::Storage {
                    storage_id,
                    storage_name: "Local Storage".to_string(),
                    description: Some("Default local storage for tests".to_string()),
                    storage_type: kalamdb_commons::models::StorageType::Filesystem,
                    base_directory: "/tmp/kalamdb_test".to_string(),
                    credentials: None,
                    config_json: None,
                    shared_tables_template: "shared/{namespace}/{table}".to_string(),
                    user_tables_template: "user/{namespace}/{table}/{userId}".to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                    updated_at: chrono::Utc::now().timestamp_millis(),
                })
                .unwrap();
        }
    });

    // Return the test DB (guaranteed to be set by Once)
    TEST_DB.get().expect("TEST_DB should be initialized").clone()
}

/// Create a JobsTableProvider for testing
///
/// Returns the jobs provider from the initialized AppContext.
pub fn create_test_jobs_provider() -> Arc<kalamdb_system::JobsTableProvider> {
    init_test_app_context();
    let ctx = AppContext::get();
    ctx.system_tables().jobs().clone()
}

/// Create a JobRegistry with all executors for testing
pub fn create_test_job_registry() -> JobRegistry {
    let registry = JobRegistry::new();

    // Register all 8 executors
    registry.register(Arc::new(FlushExecutor::new()));
    registry.register(Arc::new(CleanupExecutor::new()));
    registry.register(Arc::new(RetentionExecutor::new()));
    registry.register(Arc::new(StreamEvictionExecutor::new()));
    registry.register(Arc::new(UserCleanupExecutor::new()));
    registry.register(Arc::new(CompactExecutor::new()));
    registry.register(Arc::new(BackupExecutor::new()));
    registry.register(Arc::new(RestoreExecutor::new()));

    registry
}

/// Create a test SessionContext
///
/// Returns an Arc<SessionContext> for use in tests.
/// Each call creates a new session, but they share the same DataFusion config.
///
/// # Example
/// ```no_run
/// use kalamdb_core::test_helpers::create_test_session;
///
/// #[test]
/// fn my_test() {
///     let session = create_test_session();
///     // Use session in ExecutionContext::new(user_id, role, session)
/// }
/// ```
pub fn create_test_session() -> Arc<SessionContext> {
    init_test_app_context();
    Arc::new(AppContext::get().session_factory().create_session())
}
