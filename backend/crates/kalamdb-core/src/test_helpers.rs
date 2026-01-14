//! Test helpers compiled only for kalamdb-core unit tests.

use crate::app_context::AppContext;
use crate::jobs::executors::{
    BackupExecutor, CleanupExecutor, CompactExecutor, FlushExecutor, JobRegistry, RestoreExecutor,
    RetentionExecutor, StreamEvictionExecutor, UserCleanupExecutor,
};
use datafusion::prelude::SessionContext;
use kalamdb_commons::models::{NamespaceId, NodeId, StorageId};
use kalamdb_commons::{StoragePartition, SystemTable};
use kalamdb_store::test_utils::TestDb;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::sync::Once;

static TEST_DB: OnceCell<Arc<TestDb>> = OnceCell::new();
static TEST_RUNTIME: OnceCell<Arc<tokio::runtime::Runtime>> = OnceCell::new();
static INIT: Once = Once::new();
static BOOTSTRAP_INIT: Once = Once::new();

/// Initialize AppContext with minimal test dependencies.
///
/// This is used by unit tests inside `kalamdb-core/src/**` that run with `cfg(test)`.
pub fn init_test_app_context() -> Arc<TestDb> {
    INIT.call_once(|| {
        let mut column_families: Vec<&'static str> = SystemTable::all_tables()
            .iter()
            .filter_map(|t| t.column_family_name())
            .collect();
        column_families.push(StoragePartition::InformationSchemaTables.name());
        column_families.push("shared_table:app:config");
        column_families.push("stream_table:app:events");

        let test_db = Arc::new(TestDb::new(&column_families).unwrap());

        TEST_DB.set(test_db.clone()).ok();

        let storage_backend: Arc<dyn StorageBackend> =
            Arc::new(RocksDBBackend::new(test_db.db.clone()));

        let mut test_config = kalamdb_commons::config::ServerConfig::default();
        test_config.storage.data_path = "data".to_string();
        test_config.execution.max_parameters = 50;
        test_config.execution.max_parameter_size_bytes = 512 * 1024;

        AppContext::init(
            storage_backend,
            NodeId::new(1),
            "data/storage".to_string(),
            test_config,
        );
    });

    // One-time bootstrap that matches server startup behavior closely:
    // - Start + initialize single-node Raft so meta operations have a leader
    // - Seed default namespace + default local storage so scans can resolve storage paths
    BOOTSTRAP_INIT.call_once(|| {
        let app_ctx = AppContext::get();
        let executor = app_ctx.executor();

        // Keep a dedicated Tokio runtime alive for the lifetime of the test process.
        // Raft spawns background tasks that must keep running after init.
        let rt = TEST_RUNTIME
            .get_or_init(|| Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime")))
            .clone();

        // Kick off raft start+bootstrap on the dedicated runtime and synchronously wait.
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
        if namespaces
            .get_namespace_by_id(&default_namespace)
            .unwrap()
            .is_none()
        {
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

    TEST_DB
        .get()
        .expect("TEST_DB should be initialized")
        .clone()
}

pub fn create_test_job_registry() -> JobRegistry {
    let registry = JobRegistry::new();

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

pub fn create_test_session() -> Arc<SessionContext> {
    init_test_app_context();
    Arc::new(AppContext::get().session_factory().create_session())
}
