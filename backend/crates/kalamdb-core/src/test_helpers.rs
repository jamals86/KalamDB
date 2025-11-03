//! Test helpers for initializing AppContext with minimal dependencies
//!
//! This module provides utilities for setting up AppContext in unit tests,
//! eliminating the need to manually construct 18+ dependencies.

use crate::app_context::AppContext;
use crate::catalog::schema_cache::SchemaCache;
use crate::jobs::tokio_job_manager::TokioJobManager;
use crate::live_query::LiveQueryManager;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::storage::storage_registry::StorageRegistry;
use crate::tables::system::jobs_v2::JobsTableProvider;
use crate::tables::system::live_queries_v2::LiveQueriesTableProvider;
use crate::tables::system::namespaces_v2::NamespacesTableProvider;
use crate::tables::system::storages_v2::StoragesTableProvider;
use crate::tables::system::tables_v2::TablesTableProvider;
use crate::tables::system::users_v2::UsersTableProvider;
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use crate::tables::system::schemas::TableSchemaStore;
use datafusion::prelude::SessionContext;
use kalamdb_commons::models::NodeId;
use kalamdb_sql::KalamSql;
use kalamdb_store::test_utils::TestDb;
use kalamdb_store::{RocksDBBackend, StorageBackend};
use std::sync::Arc;
use once_cell::sync::OnceCell;

static TEST_DB: OnceCell<Arc<TestDb>> = OnceCell::new();

/// Initialize AppContext with minimal test dependencies
///
/// Creates all required stores and providers with test-friendly defaults.
/// Safe to call multiple times - only the first call initializes the singleton.
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
    // Return existing test DB if already initialized
    if let Some(test_db) = TEST_DB.get() {
        return test_db.clone();
    }

    // Check if AppContext is already initialized (would happen if another test ran first)
    // If so, just create and return a dummy TestDB
    if AppContext::try_get().is_some() {
        let dummy_db = Arc::new(
            TestDb::new(&["dummy_cf"])
                .expect("Failed to create dummy test database")
        );
        TEST_DB.set(dummy_db.clone()).ok();
        return dummy_db;
    }

    // Create test database with all required column families
    let test_db = Arc::new(
        TestDb::new(&[
            "system_tables",
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

    let storage_backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(test_db.db.clone()));

    // Create stores with proper constructors (they need backend + namespace prefix)
    let user_table_store = Arc::new(UserTableStore::new(storage_backend.clone(), "user_table".to_string()));
    let shared_table_store = Arc::new(SharedTableStore::new(storage_backend.clone(), "shared_table".to_string()));
    let stream_table_store = Arc::new(StreamTableStore::new(storage_backend.clone(), "stream_table".to_string()));
    let schema_store = Arc::new(TableSchemaStore::new(storage_backend.clone()));

    // Create KalamSql adapter (may fail, unwrap for tests)
    let kalam_sql = Arc::new(KalamSql::new(storage_backend.clone()).expect("Failed to create KalamSql"));

    // Create caches and registries
    let schema_cache = Arc::new(SchemaCache::new(1000, None));
    let storage_registry = Arc::new(StorageRegistry::new(
        kalam_sql.clone(),
        "data/storage".to_string(),
    ));

    // Create JobManager
    let job_manager = Arc::new(TokioJobManager::new());

    // Create LiveQueryManager
    let live_query_manager = Arc::new(LiveQueryManager::new(
        kalam_sql.clone(),
        NodeId::new("test-node".to_string()),
        Some(user_table_store.clone()),
        Some(shared_table_store.clone()),
        Some(stream_table_store.clone()),
    ));

    // Create DataFusion session
    let session_factory = Arc::new(DataFusionSessionFactory {});
    let base_session_context = Arc::new(SessionContext::new());

    // Create system table providers (they need the storage backend)
    let users_provider = Arc::new(UsersTableProvider::new(storage_backend.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(storage_backend.clone()));
    let namespaces_provider = Arc::new(NamespacesTableProvider::new(storage_backend.clone()));
    let storages_provider = Arc::new(StoragesTableProvider::new(storage_backend.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(storage_backend.clone()));
    let tables_provider = Arc::new(TablesTableProvider::new(storage_backend.clone()));

    // Initialize AppContext (only if not already done)
    if AppContext::try_get().is_none() {
        AppContext::init(
            schema_cache,
            user_table_store,
            shared_table_store,
            stream_table_store,
            kalam_sql.clone(),
            storage_backend.clone(),
            schema_store,
            job_manager,
            live_query_manager,
            storage_registry,
            session_factory,
            base_session_context,
            users_provider,
            jobs_provider,
            namespaces_provider,
            storages_provider.clone(),
            live_queries_provider,
            tables_provider,
        );

        // Create default 'local' storage entry for tests
        use kalamdb_commons::system::Storage;
        use kalamdb_commons::models::StorageId;
        let default_storage = Storage {
            storage_id: StorageId::new("local"),
            storage_name: "local".to_string(),
            description: Some("Default local storage for tests".to_string()),
            storage_type: "filesystem".to_string(),
            base_directory: "data/storage/local".to_string(),
            credentials: None,
            shared_tables_template: "{namespace}/{tableName}".to_string(),
            user_tables_template: "{namespace}/{tableName}/{userId}/{shard}".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };
        
        // Insert using KalamSql
        if let Err(e) = kalam_sql.insert_storage(&default_storage) {
            eprintln!("Warning: Failed to create default storage: {:?}", e);
        }
    }

    test_db
}
