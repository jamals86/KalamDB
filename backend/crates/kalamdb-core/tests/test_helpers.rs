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
use kalamdb_system::{StoragePartition, SystemTable};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::sync::Once;
use tempfile::TempDir;

static TEST_DB: OnceCell<Arc<TestDb>> = OnceCell::new();
static TEST_RUNTIME: OnceCell<Arc<tokio::runtime::Runtime>> = OnceCell::new();
static TEST_APP_CONTEXT: OnceCell<Arc<AppContext>> = OnceCell::new();
static TEST_DATA_DIR: OnceCell<TempDir> = OnceCell::new();
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
///     // AppContext is initialized for tests
/// }
/// ```
pub fn init_test_app_context() -> Arc<TestDb> {
    // Ensure runtime is initialized first (outside INIT to allow reuse if INIT called once)
    let rt = TEST_RUNTIME
        .get_or_init(|| Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime")))
        .clone();

    // Use Once to ensure we only initialize once across all tests
    INIT.call_once(|| {
        let _guard = rt.enter(); // Enter shared runtime for AppContext initialization tasks

        // Create test database with all required column families
        let mut column_families: Vec<&'static str> = SystemTable::all_tables()
            .iter()
            .filter_map(|t| t.column_family_name())
            .collect();
        column_families.push(StoragePartition::InformationSchemaTables.name());
        column_families.push("shared_table:app:config");
        column_families.push("stream_table:app:events");

        let test_db = Arc::new(TestDb::new(&column_families).unwrap());

        // Store in static for reuse
        TEST_DB.set(test_db.clone()).ok();

        let storage_backend: Arc<dyn StorageBackend> =
            Arc::new(RocksDBBackend::new(test_db.db.clone()));

        // Create isolated temp dir for Raft snapshots and storage
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let data_path = temp_dir.path().to_str().unwrap().to_string();
        TEST_DATA_DIR.set(temp_dir).expect("Failed to set TEST_DATA_DIR");

        // Create minimal test config using Default + overrides
        let mut test_config = kalamdb_configs::ServerConfig::default();
        test_config.storage.data_path = data_path;
        test_config.execution.max_parameters = 50;
        test_config.execution.max_parameter_size_bytes = 512 * 1024;

        // Phase 5: Simple 4-parameter initialization with config!
        // Uses constants from kalamdb_commons for table prefixes
        let app_ctx = AppContext::init(
            storage_backend,
            NodeId::new(1), // test node ID
            "data/storage".to_string(),
            test_config,
        );
        TEST_APP_CONTEXT
            .set(app_ctx)
            .expect("TEST_APP_CONTEXT already initialized");
    });

    BOOTSTRAP_INIT.call_once(|| {
        let app_ctx = test_app_context();
        let executor = app_ctx.executor();

        // Use the existing shared runtime
        let rt = TEST_RUNTIME
            .get()
            .expect("TEST_RUNTIME should be initialized")
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

        let app_ctx = test_app_context();

        // Ensure default namespace exists
        let namespaces = app_ctx.system_tables().namespaces();
        let default_namespace = NamespaceId::default();
        if namespaces.get_namespace_by_id(&default_namespace).unwrap().is_none() {
            namespaces
                .create_namespace(kalamdb_system::Namespace {
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
                .create_storage(kalamdb_system::Storage {
                    storage_id,
                    storage_name: "Local Storage".to_string(),
                    description: Some("Default local storage for tests".to_string()),
                    storage_type: kalamdb_system::providers::storages::models::StorageType::Filesystem,
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

pub fn test_app_context() -> Arc<AppContext> {
    init_test_app_context();
    TEST_APP_CONTEXT
        .get()
        .expect("TEST_APP_CONTEXT should be initialized")
        .clone()
}

/// Create a JobsTableProvider for testing
///
/// Returns the jobs provider from the initialized AppContext.
pub fn create_test_jobs_provider() -> Arc<kalamdb_system::JobsTableProvider> {
    let ctx = test_app_context();
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
    let app_ctx = test_app_context();
    Arc::new(app_ctx.session_factory().create_session())
}

/// Returns an AppContext without starting Raft bootstrap.
///
/// Use this for tests that only need schema registry, system tables, etc.
/// but do NOT need Raft consensus or leader election.
///
/// This creates a fresh AppContext per call (no shared statics) and is much
/// faster than `test_app_context()` which starts a full Raft cluster.
pub fn test_app_context_simple() -> Arc<AppContext> {
    let mut column_families: Vec<&'static str> = SystemTable::all_tables()
        .iter()
        .filter_map(|t| t.column_family_name())
        .collect();
    column_families.push(StoragePartition::InformationSchemaTables.name());
    column_families.push("shared_table:app:config");
    column_families.push("stream_table:app:events");

    let test_db = TestDb::new(&column_families).expect("create test db");

    let storage_backend: Arc<dyn StorageBackend> =
        Arc::new(RocksDBBackend::new(test_db.db.clone()));

    let mut test_config = kalamdb_configs::ServerConfig::default();
    test_config.storage.data_path = "data".to_string();
    test_config.execution.max_parameters = 50;
    test_config.execution.max_parameter_size_bytes = 512 * 1024;

    AppContext::init(
        storage_backend,
        NodeId::new(1),
        "data/storage".to_string(),
        test_config,
    )
}

/// Creates a SessionContext using test_app_context_simple() (no Raft bootstrap).
pub fn create_test_session_simple() -> Arc<SessionContext> {
    let app_ctx = test_app_context_simple();
    Arc::new(app_ctx.session_factory().create_session())
}
