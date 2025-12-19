//! AppContext singleton for KalamDB (Phase 5, T204)
//!
//! Provides global access to all core resources with simplified 3-parameter initialization.
//! Uses constants from kalamdb_commons for table prefixes.

use crate::error_extensions::KalamDbResultExt;
use crate::jobs::executors::{
    BackupExecutor, CleanupExecutor, CompactExecutor, FlushExecutor, JobCleanupExecutor,
    JobRegistry, RestoreExecutor, RetentionExecutor, StreamEvictionExecutor, UserCleanupExecutor,
};
use crate::live::ConnectionsManager;
use crate::live_query::LiveQueryManager;
use crate::schema_registry::settings::{SettingsTableProvider, SettingsView};
use crate::schema_registry::stats::StatsTableProvider;
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
use std::time::{Duration, Instant};

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
    /// Node identifier loaded once from server.toml (Phase 10, US0, FR-000)
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

    // ===== Connections Manager (WebSocket connections, subscriptions, heartbeat) =====
    connection_registry: Arc<ConnectionsManager>,

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

    // ===== Manifest Service (unified: hot cache + RocksDB + cold storage) =====
    manifest_service: Arc<crate::manifest::ManifestService>,

    // ===== Shared SqlExecutor =====
    sql_executor: OnceCell<Arc<SqlExecutor>>,

    // ===== Server Start Time (for uptime calculation) =====
    server_start_time: Instant,
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
            .field("connection_registry", &"Arc<ConnectionsManager>")
            .field("storage_registry", &"Arc<StorageRegistry>")
            .field("session_factory", &"Arc<DataFusionSessionFactory>")
            .field("base_session_context", &"Arc<SessionContext>")
            .field("system_tables", &"Arc<SystemTablesRegistry>")
            .field("system_columns_service", &"Arc<SystemColumnsService>")
            .field("slow_query_logger", &"Arc<SlowQueryLogger>")
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
    /// - `node_id`: Node identifier loaded from server.toml (wrapped in Arc internally)
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
    /// let node_id = NodeId::new("prod-node-1".to_string()); // From server.toml
    /// let config = ServerConfig::from_file("server.toml").unwrap();
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

                // Create StatsTableProvider (callback will be set after AppContext is created)
                let stats_provider = Arc::new(StatsTableProvider::new());
                system_tables.set_stats_provider(stats_provider.clone());

                // Inject SettingsTableProvider with config access (Phase 15)
                let settings_view = Arc::new(SettingsView::with_config((*config).clone()));
                let settings_provider = Arc::new(SettingsTableProvider::new(settings_view));
                system_tables.set_settings_provider(settings_provider);

                // Inject ServerLogsTableProvider with logs path (reads JSON log files)
                let server_logs_provider = Arc::new(
                    kalamdb_system::ServerLogsTableProvider::new(&config.logging.logs_path),
                );
                system_tables.set_server_logs_provider(server_logs_provider);

                // Register all system tables in DataFusion
                // Use config-driven DataFusion settings for parallelism
                let session_factory = Arc::new(
                    DataFusionSessionFactory::with_config(&config.datafusion)
                        .expect("Failed to create DataFusion session factory"),
                );
                let base_session_context = Arc::new(session_factory.create_session());

                // Wire up SchemaRegistry with base_session_context for automatic table registration
                schema_registry.set_base_session_context(Arc::clone(&base_session_context));

                // Register system schema
                // Use constant catalog name "kalam" - configured in DataFusionSessionFactory
                let system_schema =
                    Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
                let catalog = base_session_context
                    .catalog("kalam")
                    .expect("Catalog 'kalam' not found - ensure DataFusionSessionFactory is properly configured");

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

                // Register existing namespaces as DataFusion schemas
                // This ensures all namespaces persisted in RocksDB are available for SQL queries
                let namespaces_provider = system_tables.namespaces();
                if let Ok(namespaces) = namespaces_provider.list_namespaces() {
                    let namespace_names: Vec<String> = namespaces
                        .iter()
                        .map(|ns| ns.namespace_id.as_str().to_string())
                        .collect();
                    session_factory.register_namespaces(&base_session_context, &namespace_names);
                }

                // Note: information_schema.tables and information_schema.columns are provided
                // by DataFusion's built-in support (enabled via .with_information_schema(true))

                // Create job registry and register all 8 executors (Phase 9, T154)
                let job_registry = Arc::new(JobRegistry::new());
                job_registry.register(Arc::new(FlushExecutor::new()));
                job_registry.register(Arc::new(CleanupExecutor::new()));
                job_registry.register(Arc::new(JobCleanupExecutor::new()));
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

                // Create connections manager (unified WebSocket connection management)
                // Timeouts from config or defaults
                let client_timeout = Duration::from_secs(config.websocket.client_timeout_secs.unwrap_or(30));
                let auth_timeout = Duration::from_secs(config.websocket.auth_timeout_secs.unwrap_or(10));
                let heartbeat_interval = Duration::from_secs(config.websocket.heartbeat_interval_secs.unwrap_or(5));
                let connection_registry = ConnectionsManager::new(
                    (*node_id).clone(),
                    client_timeout,
                    auth_timeout,
                    heartbeat_interval,
                );

                let live_query_manager = Arc::new(LiveQueryManager::new(
                    system_tables.live_queries(),
                    schema_registry.clone(),
                    connection_registry.clone(),
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

                // Create unified manifest service (hot cache + RocksDB + cold storage)
                let base_storage_path = config.storage.default_storage_path.clone();
                let manifest_service = Arc::new(crate::manifest::ManifestService::new(
                    storage_backend.clone(),
                    base_storage_path,
                    config.manifest_cache.clone(),
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
                    connection_registry,
                    storage_registry,
                    system_tables,
                    session_factory,
                    base_session_context,
                    system_columns_service,
                    slow_query_logger,
                    manifest_service,
                    sql_executor: OnceCell::new(),
                    server_start_time: Instant::now(),
                });

                // Attach AppContext to components that require it (JobsManager)
                job_manager.set_app_context(Arc::clone(&app_ctx));

                // Wire up StatsTableProvider metrics callback now that AppContext exists
                let app_ctx_for_stats = Arc::clone(&app_ctx);
                stats_provider.set_metrics_callback(Arc::new(move || {
                    app_ctx_for_stats.compute_metrics()
                }));

                // Wire up ManifestTableProvider in_memory_checker callback
                // This allows system.manifest to show if a cache entry is in hot memory
                let manifest_for_checker = Arc::clone(&app_ctx.manifest_service);
                app_ctx.system_tables().manifest().set_in_memory_checker(
                    Arc::new(move |cache_key: &str| manifest_for_checker.is_in_hot_cache_by_string(cache_key))
                );

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

        // Create StatsTableProvider (callback will be set after AppContext is created for tests)
        let stats_provider = Arc::new(StatsTableProvider::new());
        system_tables.set_stats_provider(stats_provider.clone());

        // Inject SettingsTableProvider with default config for testing
        let settings_view = Arc::new(SettingsView::with_config(ServerConfig::default()));
        let settings_provider = Arc::new(SettingsTableProvider::new(settings_view));
        system_tables.set_settings_provider(settings_provider);

        // Inject ServerLogsTableProvider with temp path for testing
        let server_logs_provider =
            Arc::new(kalamdb_system::ServerLogsTableProvider::new("/tmp/kalamdb-test/logs"));
        system_tables.set_server_logs_provider(server_logs_provider);

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

        // Create connections manager for tests
        let connection_registry = ConnectionsManager::new(
            (*node_id).clone(),
            Duration::from_secs(30), // client_timeout
            Duration::from_secs(10), // auth_timeout
            Duration::from_secs(5),  // heartbeat_interval
        );

        // Create live query manager
        let live_query_manager = Arc::new(LiveQueryManager::new(
            system_tables.live_queries(),
            schema_registry.clone(),
            connection_registry.clone(),
            Arc::clone(&base_session_context),
        ));

        // Create test config
        let config = Arc::new(ServerConfig::default());

        // Create minimal slow query logger for tests (no async task)
        let slow_query_logger = Arc::new(crate::slow_query_logger::SlowQueryLogger::new_test());

        // Create system columns service with worker_id=0 for tests
        let system_columns_service = Arc::new(crate::system_columns::SystemColumnsService::new(0));

        // Create unified manifest service for tests
        let manifest_service = Arc::new(crate::manifest::ManifestService::new(
            storage_backend.clone(),
            "./data/storage".to_string(),
            config.manifest_cache.clone(),
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
            connection_registry,
            storage_registry,
            system_tables,
            session_factory,
            base_session_context,
            system_columns_service,
            slow_query_logger,
            manifest_service,
            sql_executor: OnceCell::new(),
            server_start_time: Instant::now(),
        }
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
        APP_CONTEXT.get().cloned()
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

    /// Get the connections manager (WebSocket connection state)
    pub fn connection_registry(&self) -> Arc<ConnectionsManager> {
        self.connection_registry.clone()
    }

    /// Get the NodeId loaded from server.toml (Phase 10, US0, FR-000)
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

    /// Get the manifest service (unified: hot cache + RocksDB + cold storage)
    ///
    /// Returns an Arc reference to the ManifestService that provides:
    /// - Hot cache (moka) for sub-millisecond lookups
    /// - RocksDB persistence for crash recovery
    /// - Cold storage access for manifest.json files
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
            .into_kalamdb_error("Failed to insert job")
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
            .into_kalamdb_error("Failed to scan jobs")
    }

    /// Get server uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.server_start_time.elapsed().as_secs()
    }

    /// Compute current system metrics snapshot
    ///
    /// Returns a vector of (metric_name, metric_value) tuples for display
    /// in system.stats virtual view.
    pub fn compute_metrics(&self) -> Vec<(String, String)> {
        let mut metrics = Vec::new();

        // Server uptime
        let uptime = self.uptime_seconds();
        metrics.push(("server_uptime_seconds".to_string(), uptime.to_string()));

        // Format uptime as human-readable
        let days = uptime / 86400;
        let hours = (uptime % 86400) / 3600;
        let minutes = (uptime % 3600) / 60;
        let uptime_human = if days > 0 {
            format!("{}d {}h {}m", days, hours, minutes)
        } else if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else {
            format!("{}m", minutes)
        };
        metrics.push(("server_uptime_human".to_string(), uptime_human));

        // Count entities from system tables
        // Users count
        if let Ok(batch) = self.system_tables.users().scan_all_users() {
            metrics.push(("total_users".to_string(), batch.num_rows().to_string()));
        } else {
            metrics.push(("total_users".to_string(), "0".to_string()));
        }

        // Namespaces count
        if let Ok(namespaces) = self.system_tables.namespaces().scan_all() {
            metrics.push(("total_namespaces".to_string(), namespaces.len().to_string()));
        } else {
            metrics.push(("total_namespaces".to_string(), "0".to_string()));
        }

        // Tables count
        if let Ok(tables) = self.system_tables.tables().scan_all() {
            metrics.push(("total_tables".to_string(), tables.len().to_string()));
        } else {
            metrics.push(("total_tables".to_string(), "0".to_string()));
        }

        // Jobs count
        if let Ok(jobs) = self.system_tables.jobs().list_jobs() {
            metrics.push(("total_jobs".to_string(), jobs.len().to_string()));
        } else {
            metrics.push(("total_jobs".to_string(), "0".to_string()));
        }

        // Storages count
        if let Ok(batch) = self.system_tables.storages().scan_all_storages() {
            metrics.push(("total_storages".to_string(), batch.num_rows().to_string()));
        } else {
            metrics.push(("total_storages".to_string(), "0".to_string()));
        }

        // Live queries count
        if let Ok(batch) = self.system_tables.live_queries().scan_all_live_queries() {
            metrics.push(("total_live_queries".to_string(), batch.num_rows().to_string()));
        } else {
            metrics.push(("total_live_queries".to_string(), "0".to_string()));
        }

        // Active WebSocket connections
        let active_connections = self.connection_registry.connection_count();
        metrics.push(("active_connections".to_string(), active_connections.to_string()));

        // Schema cache size and stats
        let cache_size = self.schema_registry.len();
        metrics.push(("schema_cache_size".to_string(), cache_size.to_string()));

        // Schema cache hit rate
        let (_, hits, misses, hit_rate) = self.schema_registry.stats();
        metrics.push(("schema_cache_hits".to_string(), hits.to_string()));
        metrics.push(("schema_cache_misses".to_string(), misses.to_string()));
        metrics.push(("schema_cache_hit_rate".to_string(), format!("{:.2}%", hit_rate * 100.0)));

        // Node ID
        metrics.push(("node_id".to_string(), self.node_id.as_str().to_string()));

        // Server version (from config)
        metrics.push(("server_version".to_string(), "0.1.1".to_string()));

        metrics
    }
}
