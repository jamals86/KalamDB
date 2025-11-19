//! AppContext singleton for KalamDB (Phase 5, T204)
//!
//! Provides global access to all core resources with simplified 3-parameter initialization.
//! Uses constants from kalamdb_commons for table prefixes.

use crate::jobs::executors::{
    BackupExecutor, CleanupExecutor, CompactExecutor, FlushExecutor, JobRegistry, RestoreExecutor,
    RetentionExecutor, StreamEvictionExecutor, UserCleanupExecutor,
};
use crate::live_query::LiveQueryManager;
use crate::schema_registry::SchemaRegistry;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::sql::executor::SqlExecutor;
use crate::storage::storage_registry::StorageRegistry;
use datafusion::catalog::SchemaProvider;
use datafusion::prelude::SessionContext;
use kalamdb_commons::{constants::ColumnFamilyNames, NodeId, ServerConfig};
use kalamdb_store::StorageBackend;
use kalamdb_system::SystemTablesRegistry;
use kalamdb_tables::{SharedTableStore, StreamTableStore, UserTableStore};
use once_cell::sync::OnceCell;
use std::sync::{Arc, OnceLock};

static APP_CONTEXT: OnceLock<Arc<AppContext>> = OnceLock::new();

/// AppContext singleton
///
/// Central registry of all shared resources. Services and executors fetch
/// dependencies via AppContext::get() instead of storing them, enabling:
/// - Zero duplication across layers
/// - Stateless services (zero-sized structs)
/// - Memory-efficient per-request operations
/// - Single source of truth for all shared state
pub struct AppContext {
    /// Node identifier loaded once from config.toml (Phase 10, US0, FR-000)
    /// Wrapped in Arc for zero-copy sharing across all components
    node_id: Arc<NodeId>,

    /// Server configuration loaded once at startup (Phase 11, T062-T064)
    /// Provides access to all config settings (execution, limits, storage, etc.)
    config: Arc<ServerConfig>,

    // ===== Caches =====
    schema_registry: Arc<SchemaRegistry>,

    // ===== Stores =====
    user_table_store: Arc<UserTableStore>,
    shared_table_store: Arc<SharedTableStore>,
    stream_table_store: Arc<StreamTableStore>,

    // ===== Core Infrastructure =====
    storage_backend: Arc<dyn StorageBackend>,

    // ===== Managers =====
    job_manager: Arc<crate::jobs::JobsManager>,
    live_query_manager: Arc<LiveQueryManager>,

    // ===== Registries =====
    storage_registry: Arc<StorageRegistry>,

    // ===== DataFusion Session Management =====
    session_factory: Arc<DataFusionSessionFactory>,
    base_session_context: Arc<SessionContext>,

    // ===== System Tables Registry =====
    system_tables: Arc<SystemTablesRegistry>,

    // ===== System Columns Service =====
    system_columns_service: Arc<crate::system_columns::SystemColumnsService>,

    // ===== Slow Query Logger =====
    slow_query_logger: Arc<crate::slow_query_logger::SlowQueryLogger>,

    // ===== Manifest Cache Service (Phase 4, US6) =====
    manifest_cache_service: Arc<crate::manifest::ManifestCacheService>,

    // ===== Manifest Service (Phase 5, US2, T107-T113) =====
    manifest_service: Arc<crate::manifest::ManifestService>,

    // ===== Shared SqlExecutor =====
    sql_executor: OnceCell<Arc<SqlExecutor>>,
}

impl std::fmt::Debug for AppContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppContext")
            .field("schema_registry", &"Arc<SchemaRegistry>")
            .field("user_table_store", &"Arc<UserTableStore>")
            .field("shared_table_store", &"Arc<SharedTableStore>")
            .field("stream_table_store", &"Arc<StreamTableStore>")
            .field("storage_backend", &"Arc<dyn StorageBackend>")
            .field("job_manager", &"Arc<JobsManager>")
            .field("live_query_manager", &"Arc<LiveQueryManager>")
            .field("storage_registry", &"Arc<StorageRegistry>")
            .field("session_factory", &"Arc<DataFusionSessionFactory>")
            .field("base_session_context", &"Arc<SessionContext>")
            .field("system_tables", &"Arc<SystemTablesRegistry>")
            .field("system_columns_service", &"Arc<SystemColumnsService>")
            .field("slow_query_logger", &"Arc<SlowQueryLogger>")
            .field("manifest_cache_service", &"Arc<ManifestCacheService>")
            .field("manifest_service", &"Arc<ManifestService>")
            .field("sql_executor", &"OnceCell<Arc<SqlExecutor>>")
            .finish()
    }
}

impl AppContext {
    /// Initialize AppContext singleton with config-driven NodeId
    ///
    /// Creates all dependencies internally using constants from kalamdb_commons.
    /// Table prefixes are read from constants::USER_TABLE_PREFIX, etc.
    ///
    /// **Phase 10, US0 (FR-000)**: NodeId is now loaded from config and wrapped in Arc
    /// for zero-copy sharing across all components, eliminating duplicate instantiations.
    ///
    /// # Parameters
    /// - `storage_backend`: Storage abstraction (RocksDB implementation)
    /// - `node_id`: Node identifier loaded from config.toml (wrapped in Arc internally)
    /// - `storage_base_path`: Base directory for storage files
    ///
    /// # Example
    /// ```no_run
    /// use kalamdb_core::app_context::AppContext;
    /// use kalamdb_commons::NodeId;
    /// # use kalamdb_store::{RocksDBBackend, StorageBackend};
    /// # use std::sync::Arc;
    ///
    /// let backend: Arc<dyn StorageBackend> = todo!();
    /// let node_id = NodeId::new("prod-node-1".to_string()); // From config.toml
    /// let config = ServerConfig::from_file("config.toml").unwrap();
    /// AppContext::init(
    ///     backend,
    ///     node_id,
    ///     "data/storage".to_string(),
    ///     config,
    /// );
    /// ```
    pub fn init(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
        config: ServerConfig,
    ) -> Arc<AppContext> {
        let node_id = Arc::new(node_id); // Wrap NodeId in Arc for zero-copy sharing (FR-000)
        let config = Arc::new(config); // Wrap config in Arc for zero-copy sharing
        APP_CONTEXT
            .get_or_init(|| {
                // Create stores using constants from kalamdb_commons
                let user_table_store = Arc::new(UserTableStore::new(
                    storage_backend.clone(),
                    ColumnFamilyNames::USER_TABLE_PREFIX.to_string(),
                ));
                let shared_table_store = Arc::new(SharedTableStore::new(
                    storage_backend.clone(),
                    ColumnFamilyNames::SHARED_TABLE_PREFIX.to_string(),
                ));
                let stream_table_store = Arc::new(StreamTableStore::new(
                    storage_backend.clone(),
                    ColumnFamilyNames::STREAM_TABLE_PREFIX.to_string(),
                ));

                // Create system table providers registry FIRST (needed by StorageRegistry and information_schema)
                let system_tables = Arc::new(SystemTablesRegistry::new(storage_backend.clone()));

                // Create storage registry (uses StoragesTableProvider from system_tables)
                let storage_registry = Arc::new(StorageRegistry::new(
                    system_tables.storages(),
                    storage_base_path,
                ));

                // Create schema cache (Phase 10 unified cache)
                let schema_registry = Arc::new(SchemaRegistry::new(10000));

                // Register all system tables in DataFusion
                let session_factory = Arc::new(
                    DataFusionSessionFactory::new()
                        .expect("Failed to create DataFusion session factory"),
                );
                let base_session_context = Arc::new(session_factory.create_session());

                // Wire up SchemaRegistry with base_session_context for automatic table registration
                schema_registry.set_base_session_context(Arc::clone(&base_session_context));

                // Register system schema
                let system_schema =
                    Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
                let catalog_name = base_session_context
                    .catalog_names()
                    .first()
                    .expect("No catalogs available")
                    .clone();
                let catalog = base_session_context
                    .catalog(&catalog_name)
                    .expect("Failed to get catalog");

                // Register the system schema with the catalog
                catalog
                    .register_schema("system", system_schema.clone())
                    .expect("Failed to register system schema");

                // Register all system tables in the system schema
                for (table_name, provider) in system_tables.all_system_providers() {
                    system_schema
                        .register_table(table_name.to_string(), provider)
                        .expect("Failed to register system table");
                }

                // NOW wire up information_schema providers with schema_registry
                // This must happen BEFORE registering them with DataFusion
                system_tables.set_information_schema_dependencies(schema_registry.clone());

                // Register information_schema AFTER set_information_schema_dependencies()
                let info_schema =
                    Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
                base_session_context
                    .catalog(&catalog_name)
                    .expect("Failed to get catalog")
                    .register_schema("information_schema", info_schema.clone())
                    .expect("Failed to register information_schema");

                for (table_name, provider) in system_tables.all_information_schema_providers() {
                    info_schema
                        .register_table(table_name.to_string(), provider)
                        .expect("Failed to register information_schema table");
                }

                // Create job registry and register all 8 executors (Phase 9, T154)
                let job_registry = Arc::new(JobRegistry::new());
                job_registry.register(Arc::new(FlushExecutor::new()));
                job_registry.register(Arc::new(CleanupExecutor::new()));
                job_registry.register(Arc::new(RetentionExecutor::new()));
                job_registry.register(Arc::new(StreamEvictionExecutor::new()));
                job_registry.register(Arc::new(UserCleanupExecutor::new()));
                job_registry.register(Arc::new(CompactExecutor::new()));
                job_registry.register(Arc::new(BackupExecutor::new()));
                job_registry.register(Arc::new(RestoreExecutor::new()));

                // Create unified job manager (Phase 9, T154)
                let jobs_provider = system_tables.jobs();
                let job_manager =
                    Arc::new(crate::jobs::JobsManager::new(jobs_provider, job_registry));

                let live_query_manager = Arc::new(LiveQueryManager::new(
                    system_tables.live_queries(),
                    schema_registry.clone(),
                    (*node_id).clone(), // Dereference Arc<NodeId> to NodeId for LiveQueryManager
                    Arc::clone(&base_session_context),
                ));

                // Create slow query logger (Phase 11)
                let slow_log_path = format!("{}/slow.log", config.logging.logs_path);
                let slow_query_logger = crate::slow_query_logger::SlowQueryLogger::new(
                    slow_log_path,
                    config.logging.slow_query_threshold_ms,
                );

                // Create system columns service (Phase 12, US5, T027)
                // Extract worker_id from node_id for Snowflake ID generation
                let worker_id = Self::extract_worker_id(&node_id);
                let system_columns_service =
                    Arc::new(crate::system_columns::SystemColumnsService::new(worker_id));

                // Create manifest cache service (Phase 4, US6, T074-T080)
                let manifest_cache_service = Arc::new(crate::manifest::ManifestCacheService::new(
                    storage_backend.clone(),
                    config.manifest_cache.clone(),
                ));

                // Create manifest service (Phase 5, US2, T107-T113)
                let base_storage_path = config.storage.default_storage_path.clone();
                let manifest_service = Arc::new(crate::manifest::ManifestService::new(
                    storage_backend.clone(),
                    base_storage_path,
                ));

                let app_ctx = Arc::new(AppContext {
                    node_id,
                    config,
                    schema_registry,
                    user_table_store,
                    shared_table_store,
                    stream_table_store,
                    storage_backend,
                    job_manager: job_manager.clone(),
                    live_query_manager,
                    storage_registry,
                    system_tables,
                    session_factory,
                    base_session_context,
                    system_columns_service,
                    slow_query_logger,
                    manifest_cache_service,
                    manifest_service,
                    sql_executor: OnceCell::new(),
                });

                // Attach AppContext to components that require it (JobsManager)
                job_manager.set_app_context(Arc::clone(&app_ctx));

                app_ctx
            })
            .clone()
    }

    /// Extract worker_id from node_id for Snowflake ID generation
    ///
    /// Maps node_id string to 10-bit integer (0-1023) using CRC32 hash.
    /// This ensures consistent worker_id across server restarts.
    fn extract_worker_id(node_id: &NodeId) -> u16 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        node_id.as_str().hash(&mut hasher);
        let hash = hasher.finish();

        // Map to 10-bit range (0-1023)
        (hash % 1024) as u16
    }

    /// Create a minimal AppContext for unit testing
    ///
    /// This factory method creates a lightweight AppContext with only essential
    /// dependencies initialized. Use this in unit tests instead of AppContext::get()
    /// to avoid singleton initialization requirements.
    ///
    /// **Phase 12, US5**: Test-specific factory with SystemColumnsService (worker_id=0)
    ///
    /// # Example
    /// ```no_run
    /// use kalamdb_core::app_context::AppContext;
    /// use std::sync::Arc;
    ///
    /// let app_context = Arc::new(AppContext::new_test());
    /// let sys_cols = app_context.system_columns_service();
    /// let (snowflake_id, updated_ns, deleted) = sys_cols.handle_insert(None).unwrap();
    /// ```
    #[cfg(test)]
    pub fn new_test() -> Self {
        use kalamdb_store::test_utils::InMemoryBackend;

        // Create minimal in-memory backend for tests
        let storage_backend: Arc<dyn StorageBackend> = Arc::new(InMemoryBackend::new());

        // Create stores
        let user_table_store = Arc::new(UserTableStore::new(
            storage_backend.clone(),
            ColumnFamilyNames::USER_TABLE_PREFIX.to_string(),
        ));
        let shared_table_store = Arc::new(SharedTableStore::new(
            storage_backend.clone(),
            ColumnFamilyNames::SHARED_TABLE_PREFIX.to_string(),
        ));
        let stream_table_store = Arc::new(StreamTableStore::new(
            storage_backend.clone(),
            ColumnFamilyNames::STREAM_TABLE_PREFIX.to_string(),
        ));

        // Create system tables registry
        let system_tables = Arc::new(SystemTablesRegistry::new(storage_backend.clone()));

        // Create storage registry with temp path
        let storage_registry = Arc::new(StorageRegistry::new(
            system_tables.storages(),
            "/tmp/kalamdb-test".to_string(),
        ));

        // Create minimal schema registry
        let schema_registry = Arc::new(SchemaRegistry::new(100));

        // Create DataFusion session
        let session_factory = Arc::new(
            DataFusionSessionFactory::new().expect("Failed to create test session factory"),
        );
        let base_session_context = Arc::new(session_factory.create_session());

        // Create minimal job manager
        let job_registry = Arc::new(JobRegistry::new());
        let jobs_provider = system_tables.jobs();
        let job_manager = Arc::new(crate::jobs::JobsManager::new(jobs_provider, job_registry));

        // Create test NodeId
        let node_id = Arc::new(NodeId::new("test-node".to_string()));

        // Create live query manager
        let live_query_manager = Arc::new(LiveQueryManager::new(
            system_tables.live_queries(),
            schema_registry.clone(),
            (*node_id).clone(),
            Arc::clone(&base_session_context),
        ));

        // Create test config
        let config = Arc::new(ServerConfig::default());

        // Create minimal slow query logger for tests (no async task)
        let slow_query_logger = Arc::new(crate::slow_query_logger::SlowQueryLogger::new_test());

        // Create system columns service with worker_id=0 for tests
        let system_columns_service = Arc::new(crate::system_columns::SystemColumnsService::new(0));

        // Create manifest cache service for tests
        let manifest_cache_service = Arc::new(crate::manifest::ManifestCacheService::new(
            storage_backend.clone(),
            config.manifest_cache.clone(),
        ));

        // Create manifest service for tests
        let manifest_service = Arc::new(crate::manifest::ManifestService::new(
            storage_backend.clone(),
            "./data/storage".to_string(),
        ));

        AppContext {
            node_id,
            config,
            schema_registry,
            user_table_store,
            shared_table_store,
            stream_table_store,
            storage_backend,
            job_manager,
            live_query_manager,
            storage_registry,
            system_tables,
            session_factory,
            base_session_context,
            system_columns_service,
            slow_query_logger,
            manifest_cache_service,
            manifest_service,
            sql_executor: OnceCell::new(),
        }
    }

    /// Create AppContext for integration tests without using singleton
    ///
    /// This allows each integration test to have its own isolated AppContext
    /// with its own RocksDB instance, avoiding singleton conflicts.
    ///
    /// # Arguments
    /// * `storage_backend` - Storage backend (usually RocksDB for integration tests)
    /// * `node_id` - Node identifier  
    /// * `storage_base_path` - Base directory for storage files
    /// * `config` - Server configuration
    ///
    /// # Returns
    /// A new Arc<AppContext> that is NOT stored in the singleton
    /// TODO: Its not used anywhere we can remove it?
    #[cfg(test)]
    pub fn new_for_integration_test(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
        config: ServerConfig,
    ) -> Arc<AppContext> {
        let node_id = Arc::new(node_id);
        let config = Arc::new(config);

        // Create stores using constants from kalamdb_commons
        let user_table_store = Arc::new(UserTableStore::new(
            storage_backend.clone(),
            ColumnFamilyNames::USER_TABLE_PREFIX.to_string(),
        ));
        let shared_table_store = Arc::new(SharedTableStore::new(
            storage_backend.clone(),
            ColumnFamilyNames::SHARED_TABLE_PREFIX.to_string(),
        ));
        let stream_table_store = Arc::new(StreamTableStore::new(
            storage_backend.clone(),
            ColumnFamilyNames::STREAM_TABLE_PREFIX.to_string(),
        ));

        // Create system table providers registry
        let system_tables = Arc::new(SystemTablesRegistry::new(storage_backend.clone()));

        // Create storage registry
        let storage_registry = Arc::new(StorageRegistry::new(
            system_tables.storages(),
            storage_base_path.clone(),
        ));

        // Create schema cache
        let schema_registry = Arc::new(SchemaRegistry::new(10000));

        // Register all system tables in DataFusion
        let session_factory = Arc::new(
            DataFusionSessionFactory::new().expect("Failed to create DataFusion session factory"),
        );
        let base_session_context = Arc::new(session_factory.create_session());

        // Wire up SchemaRegistry with base_session_context
        schema_registry.set_base_session_context(Arc::clone(&base_session_context));

        // Register system schema
        let system_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
        let catalog_name = base_session_context
            .catalog_names()
            .first()
            .expect("No catalogs available")
            .clone();
        base_session_context
            .catalog(&catalog_name)
            .expect("Failed to get catalog")
            .register_schema("system", system_schema.clone())
            .expect("Failed to register system schema");

        // Register all system table providers
        for (table_name, provider) in system_tables.all_system_providers() {
            system_schema
                .register_table(table_name.to_string(), provider)
                .expect("Failed to register system table");
        }

        // Wire up information_schema providers
        system_tables.set_information_schema_dependencies(schema_registry.clone());

        // Register information_schema
        let info_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
        base_session_context
            .catalog(&catalog_name)
            .expect("Failed to get catalog")
            .register_schema("information_schema", info_schema.clone())
            .expect("Failed to register information_schema");

        for (table_name, provider) in system_tables.all_information_schema_providers() {
            info_schema
                .register_table(table_name.to_string(), provider)
                .expect("Failed to register information_schema table");
        }

        // Create job registry and register all executors
        let job_registry = Arc::new(JobRegistry::new());
        job_registry.register(Arc::new(FlushExecutor::new()));
        job_registry.register(Arc::new(CleanupExecutor::new()));
        job_registry.register(Arc::new(RetentionExecutor::new()));
        job_registry.register(Arc::new(StreamEvictionExecutor::new()));
        job_registry.register(Arc::new(UserCleanupExecutor::new()));
        job_registry.register(Arc::new(CompactExecutor::new()));
        job_registry.register(Arc::new(BackupExecutor::new()));
        job_registry.register(Arc::new(RestoreExecutor::new()));

        // Create unified job manager
        let jobs_provider = system_tables.jobs();
        let job_manager = Arc::new(crate::jobs::JobsManager::new(jobs_provider, job_registry));

        // Create live query manager
        let live_query_manager = Arc::new(LiveQueryManager::new(
            system_tables.live_queries(),
            schema_registry.clone(),
            (*node_id).clone(),
            Arc::clone(&base_session_context),
        ));

        // Create slow query logger
        let slow_log_path = format!("{}/slow.log", config.logging.logs_path);
        let slow_query_logger = crate::slow_query_logger::SlowQueryLogger::new(
            slow_log_path,
            config.logging.slow_query_threshold_ms,
        );

        // Create system columns service
        let worker_id = Self::extract_worker_id(&node_id);
        let system_columns_service =
            Arc::new(crate::system_columns::SystemColumnsService::new(worker_id));

        // Create manifest cache service
        let manifest_cache_service = Arc::new(crate::manifest::ManifestCacheService::new(
            storage_backend.clone(),
            config.manifest_cache.clone(),
        ));

        // Create manifest service
        let manifest_service = Arc::new(crate::manifest::ManifestService::new(
            storage_backend.clone(),
            storage_base_path.clone(),
        ));

        let app_ctx = Arc::new(AppContext {
            node_id,
            config,
            schema_registry,
            user_table_store,
            shared_table_store,
            stream_table_store,
            storage_backend,
            job_manager: job_manager.clone(),
            live_query_manager,
            storage_registry,
            system_tables,
            session_factory,
            base_session_context,
            system_columns_service,
            slow_query_logger,
            manifest_cache_service,
            manifest_service,
            sql_executor: OnceCell::new(),
        });

        // Attach AppContext to job_manager
        job_manager.set_app_context(Arc::clone(&app_ctx));

        app_ctx
    }

    /// Get the AppContext singleton
    ///
    /// # Panics
    /// Panics if AppContext::init() has not been called yet
    pub fn get() -> Arc<AppContext> {
        APP_CONTEXT
            .get()
            .expect("AppContext not initialized")
            .clone()
    }

    /// Try to get the AppContext singleton without panicking
    pub fn try_get() -> Option<Arc<AppContext>> {
        APP_CONTEXT.get().map(|ctx| ctx.clone())
    }

    // ===== Getters =====

    /// Get the server configuration
    ///
    /// Returns an Arc reference for zero-copy sharing. This config is loaded
    /// once during AppContext::init() and shared across all components.
    pub fn config(&self) -> &Arc<ServerConfig> {
        &self.config
    }

    pub fn schema_registry(&self) -> Arc<SchemaRegistry> {
        self.schema_registry.clone()
    }

    pub fn user_table_store(&self) -> Arc<UserTableStore> {
        self.user_table_store.clone()
    }

    pub fn shared_table_store(&self) -> Arc<SharedTableStore> {
        self.shared_table_store.clone()
    }

    pub fn stream_table_store(&self) -> Arc<StreamTableStore> {
        self.stream_table_store.clone()
    }

    pub fn storage_backend(&self) -> Arc<dyn StorageBackend> {
        self.storage_backend.clone()
    }

    pub fn job_manager(&self) -> Arc<crate::jobs::JobsManager> {
        self.job_manager.clone()
    }

    pub fn live_query_manager(&self) -> Arc<LiveQueryManager> {
        self.live_query_manager.clone()
    }

    /// Get the NodeId loaded from config.toml (Phase 10, US0, FR-000)
    ///
    /// Returns an Arc reference for zero-copy sharing. This NodeId is allocated
    /// exactly once per server instance during AppContext::init().
    pub fn node_id(&self) -> &Arc<NodeId> {
        &self.node_id
    }

    pub fn storage_registry(&self) -> Arc<StorageRegistry> {
        self.storage_registry.clone()
    }

    pub fn session_factory(&self) -> Arc<DataFusionSessionFactory> {
        self.session_factory.clone()
    }

    pub fn base_session_context(&self) -> Arc<SessionContext> {
        self.base_session_context.clone()
    }

    pub fn system_tables(&self) -> Arc<SystemTablesRegistry> {
        self.system_tables.clone()
    }

    /// Get the system columns service (Phase 12, US5, T027)
    ///
    /// Returns an Arc reference to the SystemColumnsService that manages
    /// all system column operations (_seq, _deleted).
    pub fn system_columns_service(&self) -> Arc<crate::system_columns::SystemColumnsService> {
        self.system_columns_service.clone()
    }

    /// Get the slow query logger
    ///
    /// Returns an Arc reference to the lightweight slow query logger that writes
    /// to a separate slow.log file for queries exceeding the configured threshold.
    pub fn slow_query_logger(&self) -> Arc<crate::slow_query_logger::SlowQueryLogger> {
        self.slow_query_logger.clone()
    }

    /// Get the manifest cache service (Phase 4, US6, T074-T080)
    ///
    /// Returns an Arc reference to the ManifestCacheService that provides
    /// fast manifest access with two-tier caching (hot cache + RocksDB).
    pub fn manifest_cache_service(&self) -> Arc<crate::manifest::ManifestCacheService> {
        self.manifest_cache_service.clone()
    }

    /// Get the manifest service (Phase 5, US2, T107-T113)
    ///
    /// Returns an Arc reference to the ManifestService that provides
    /// read/write access to manifest.json files in storage backends.
    pub fn manifest_service(&self) -> Arc<crate::manifest::ManifestService> {
        self.manifest_service.clone()
    }

    /// Register the shared SqlExecutor (called once during bootstrap)
    pub fn set_sql_executor(&self, executor: Arc<SqlExecutor>) {
        if self.sql_executor.set(executor).is_err() {
            log::warn!(
                "SqlExecutor already initialized in AppContext; ignoring duplicate registration"
            );
        }
    }

    /// Try to access the shared SqlExecutor if initialized
    pub fn try_sql_executor(&self) -> Option<Arc<SqlExecutor>> {
        self.sql_executor.get().map(Arc::clone)
    }

    /// Get the shared SqlExecutor (panics if not yet initialized)
    pub fn sql_executor(&self) -> Arc<SqlExecutor> {
        self.try_sql_executor()
            .expect("SqlExecutor not initialized in AppContext")
    }

    // ===== Convenience methods for backward compatibility =====

    /// Insert a job into the jobs table
    ///
    /// Convenience wrapper for system_tables().jobs().create_job()
    pub fn insert_job(
        &self,
        job: &kalamdb_commons::system::Job,
    ) -> Result<(), crate::error::KalamDbError> {
        self.system_tables
            .jobs()
            .create_job(job.clone())
            .map_err(|e| crate::error::KalamDbError::Other(format!("Failed to insert job: {}", e)))
    }

    /// Scan all jobs from the jobs table
    ///
    /// Convenience wrapper for system_tables().jobs().list_jobs()
    pub fn scan_all_jobs(
        &self,
    ) -> Result<Vec<kalamdb_commons::system::Job>, crate::error::KalamDbError> {
        self.system_tables
            .jobs()
            .list_jobs()
            .map_err(|e| crate::error::KalamDbError::Other(format!("Failed to scan jobs: {}", e)))
    }
}
