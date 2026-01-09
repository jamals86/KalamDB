//! Test helpers for initializing AppContext with minimal dependencies
//!
//! This module provides utilities for setting up AppContext in unit tests.

use crate::app_context::AppContext;
use datafusion::prelude::SessionContext;
use kalamdb_commons::models::{NodeId, StorageId};
use kalamdb_store::test_utils::TestDb;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::sync::Once;

static TEST_DB: OnceCell<Arc<TestDb>> = OnceCell::new();
static INIT: Once = Once::new();
static STORAGE_INIT: Once = Once::new();

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
        test_config.storage.default_storage_path = "data/storage".to_string();
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

    // Create default 'local' storage entry for tests (separate Once to avoid deadlock)
    STORAGE_INIT.call_once(|| {
        use kalamdb_commons::system::Storage;

        let _ctx = AppContext::get();

        let _default_storage = Storage {
            storage_id: StorageId::local(),
            storage_name: "local".to_string(),
            description: Some("Default local storage for tests".to_string()),
            storage_type: kalamdb_commons::models::StorageType::Filesystem,
            base_directory: "data/storage/local".to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: "{namespace}/{tableName}".to_string(),
            user_tables_template: "{namespace}/{tableName}/{userId}/{shard}".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };
    });

    // Return the test DB (guaranteed to be set by Once)
    TEST_DB
        .get()
        .expect("TEST_DB should be initialized")
        .clone()
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
pub fn create_test_job_registry() -> crate::jobs::JobRegistry {
    use crate::jobs::executors::*;
    let registry = crate::jobs::JobRegistry::new();

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
    Arc::new(SessionContext::new())
}
