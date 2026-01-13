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
use crate::applier::UnifiedApplier;
use crate::live_query::LiveQueryManager;
use crate::views::settings::{SettingsTableProvider, SettingsView};
use crate::schema_registry::stats::StatsTableProvider;
use crate::views::datatypes::{DatatypesTableProvider, DatatypesView};
use crate::schema_registry::SchemaRegistry;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::sql::executor::SqlExecutor;
use crate::storage::storage_registry::StorageRegistry;
use datafusion::catalog::SchemaProvider;
use datafusion::prelude::SessionContext;
use kalamdb_commons::{constants::ColumnFamilyNames, NodeId, ServerConfig};
use kalamdb_raft::CommandExecutor;
use kalamdb_store::StorageBackend;
use kalamdb_system::SystemTablesRegistry;
use kalamdb_tables::{SharedTableStore, StreamTableStore, UserTableStore};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::metrics::runtime::collect_runtime_metrics;

// Use RwLock instead of OnceLock to allow resetting in tests
// The RwLock is wrapped in a OnceLock for lazy initialization
use std::sync::RwLock as StdRwLock;
use once_cell::sync::Lazy;

static APP_CONTEXT: Lazy<StdRwLock<Option<Arc<AppContext>>>> = Lazy::new(|| StdRwLock::new(None));

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

    // ===== Command Executor (Standalone or Raft-based) =====
    /// Command executor abstraction for standalone/cluster mode
    /// In standalone mode: StandaloneExecutor (direct provider calls)
    /// In cluster mode: RaftExecutor (commands go through Raft consensus)
    executor: Arc<dyn CommandExecutor>,

    // ===== System Columns Service =====
    system_columns_service: Arc<crate::system_columns::SystemColumnsService>,

    // ===== Slow Query Logger =====
    slow_query_logger: Arc<crate::slow_query_logger::SlowQueryLogger>,

    // ===== Manifest Service (unified: hot cache + RocksDB + cold storage) =====
    manifest_service: Arc<crate::manifest::ManifestService>,

    // ===== Unified Applier (single execution path for all commands) =====
    applier: Arc<dyn UnifiedApplier>,

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
            .field("executor", &"Arc<dyn CommandExecutor>")
            .field("applier", &"Arc<dyn UnifiedApplier>")
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
        // Check if already initialized
        {
            let guard = APP_CONTEXT.read().expect("APP_CONTEXT lock poisoned");
            if let Some(existing) = guard.as_ref() {
                return existing.clone();
            }
        }
        
        // Initialize
        let app_ctx = Self::create_impl(storage_backend, node_id, storage_base_path, config);
        {
            let mut guard = APP_CONTEXT.write().expect("APP_CONTEXT lock poisoned");
            *guard = Some(app_ctx.clone());
        }
        app_ctx
    }
    
    /// Create an isolated AppContext instance for tests
    ///
    /// Unlike `init()`, this **replaces** the global singleton, ensuring the
    /// new instance is used by all code that calls `AppContext::get()`.
    /// The previous AppContext is dropped.
    ///
    /// This is essential for test isolation where each test needs its own
    /// independent state (separate RocksDB, Raft groups, etc.)
    ///
    /// **Warning**: Only use this in tests! Production code should use `init()`.
    pub fn create_isolated(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
        config: ServerConfig,
    ) -> Arc<AppContext> {
        let app_ctx = Self::create_impl(storage_backend, node_id, storage_base_path, config);
        // Replace the global singleton so AppContext::get() returns the new instance
        {
            let mut guard = APP_CONTEXT.write().expect("APP_CONTEXT lock poisoned");
            *guard = Some(app_ctx.clone());
        }
        app_ctx
    }

    /// Internal implementation that creates an AppContext
    ///
    /// Extracted to allow both singleton and isolated initialization.
    fn create_impl(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
        config: ServerConfig,
    ) -> Arc<AppContext> {
        let node_id = Arc::new(node_id); // Wrap NodeId in Arc for zero-copy sharing (FR-000)
        let config = Arc::new(config); // Wrap config in Arc for zero-copy sharing
        {
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

                // Inject ServerLogsTableProvider with logs path (reads JSON log files - dev only)
                let server_logs_provider = crate::views::create_server_logs_provider(&config.logging.logs_path);
                system_tables.set_server_logs_provider(Arc::new(server_logs_provider));

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

                // Register system.datatypes virtual view (Arrow → KalamDB type mappings)
                let datatypes_view = Arc::new(DatatypesView::new());
                let datatypes_provider = Arc::new(DatatypesTableProvider::new(datatypes_view));
                system_schema
                    .register_table("datatypes".to_string(), datatypes_provider)
                    .expect("Failed to register system.datatypes");

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
                let base_storage_path = config.storage.storage_dir().to_string_lossy().into_owned();
                let manifest_service = Arc::new(crate::manifest::ManifestService::new_with_registries(
                    storage_backend.clone(),
                    base_storage_path,
                    config.manifest_cache.clone(),
                    schema_registry.clone(),
                    storage_registry.clone(),
                ));

                // Create command executor (Phase 20 - Unified Raft Executor)
                // ALWAYS use RaftExecutor - same code path for standalone and cluster
                // In standalone mode: single-node Raft cluster (no peers, instant leader election)
                // In cluster mode: multi-node Raft cluster with peers
                let raft_config = if let Some(cluster_config) = &config.cluster {
                    // Multi-node cluster mode
                    kalamdb_raft::manager::RaftManagerConfig::from(cluster_config.clone())
                } else {
                    // Single-node mode: create a Raft cluster of 1
                    // Use machine hostname as cluster ID
                    let cluster_id = hostname::get()
                        .ok()
                        .and_then(|h| h.into_string().ok())
                        .unwrap_or_else(|| "kalamdb".to_string());
                    let api_addr = format!("{}:{}", config.server.host, config.server.port);
                    kalamdb_raft::manager::RaftManagerConfig::for_single_node(cluster_id, api_addr)
                };
                
                log::debug!("Creating RaftManager...");
                let snapshots_dir = config.storage.resolved_snapshots_dir();
                let manager = Arc::new(
                    kalamdb_raft::manager::RaftManager::new_persistent(
                        raft_config,
                        storage_backend.clone(),
                        snapshots_dir,
                    )
                    .expect("Failed to create persistent RaftManager"),
                );
                
                log::debug!("Creating RaftExecutor...");
                let executor: Arc<dyn CommandExecutor> = Arc::new(kalamdb_raft::RaftExecutor::new(manager));

                // Note: ClusterLiveNotifier removed - Raft replication now handles
                // data consistency across nodes, and each node notifies its own
                // live query subscribers locally when data is applied.

                // Wire up ClusterTableProvider with the executor (cluster mode only)
                let cluster_provider = Arc::new(crate::views::create_cluster_provider(executor.clone()));
                system_tables.set_cluster_provider(cluster_provider.clone());
                
                // Register cluster with DataFusion system schema
                // This must happen after executor is created since ClusterTableProvider needs it
                system_schema
                    .register_table("cluster".to_string(), cluster_provider)
                    .expect("Failed to register system.cluster");

                // Create unified applier (lazy initialization)
                // All commands flow through Raft (even single-node mode)
                let applier = crate::applier::create_applier();

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
                    executor,
                    applier: applier.clone(),
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
                
                // Wire up UnifiedApplier with AppContext (lazy initialization)
                applier.set_app_context(Arc::clone(&app_ctx));

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

                app_ctx.wire_raft_appliers();

                // Cleanup orphan live queries from previous server run
                // Live queries don't persist across restarts (WebSocket connections are lost)
                match app_ctx.system_tables().live_queries().clear_all() {
                    Ok(count) if count > 0 => {
                        log::info!("Cleared {} orphan live queries from previous server run", count);
                    }
                    Ok(_) => {} // No orphans to clear
                    Err(e) => {
                        log::warn!("Failed to clear orphan live queries: {}", e);
                    }
                }

                app_ctx
            }
    }

    /// Wire Raft appliers that apply replicated commands into local providers.
    ///
    /// Safe to call multiple times.
    pub fn wire_raft_appliers(self: &Arc<Self>) {
        log::debug!("Wiring Raft appliers...");

        let executor = self.executor();

        let Some(raft_executor) = executor.as_any().downcast_ref::<kalamdb_raft::RaftExecutor>() else {
            log::error!("Failed to downcast executor to RaftExecutor - appliers NOT wired!");
            return;
        };

        let manager = raft_executor.manager();

        log::debug!("Wiring MetaApplier with AppContext for table provider registration...");
        let meta_applier = Arc::new(crate::applier::ProviderMetaApplier::new(Arc::clone(self)));
        manager.set_meta_applier(meta_applier);

        log::debug!("Wiring UserDataApplier for user table data replication...");
        let user_data_applier = Arc::new(crate::applier::ProviderUserDataApplier::new(Arc::clone(self)));
        manager.set_user_data_applier(user_data_applier);

        log::debug!("Wiring SharedDataApplier for shared table data replication...");
        let shared_data_applier = Arc::new(crate::applier::ProviderSharedDataApplier::new(Arc::clone(self)));
        manager.set_shared_data_applier(shared_data_applier);

        log::debug!("✓ Raft appliers wired successfully");
    }

    /// Restore state machines from persisted snapshots
    ///
    /// This should be called AFTER `wire_appliers()` to restore state machines'
    /// internal state from persisted snapshots. This prevents duplicate log entry
    /// application on server restart.
    ///
    /// The state machines need their appliers to be set first so they can properly
    /// restore persisted state to the underlying providers.
    pub async fn restore_raft_state_machines(&self) {
        log::debug!("Restoring Raft state machines from snapshots...");

        let executor = self.executor();

        let Some(raft_executor) = executor.as_any().downcast_ref::<kalamdb_raft::RaftExecutor>() else {
            log::error!("Failed to downcast executor to RaftExecutor - state machines NOT restored!");
            return;
        };

        let manager = raft_executor.manager();

        if let Err(e) = manager.restore_state_machines_from_snapshots().await {
            log::error!("Failed to restore state machines from snapshots: {:?}", e);
        } else {
            log::debug!("✓ Raft state machines restored successfully");
        }
    }

    /// Extract worker_id from node_id for Snowflake ID generation
    ///
    /// Maps node_id string to 10-bit integer (0-1023) using CRC32 hash.
    /// This ensures consistent worker_id across server restarts.
    fn extract_worker_id(node_id: &NodeId) -> u16 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        node_id.as_u64().hash(&mut hasher);
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
        let server_logs_provider = crate::views::create_server_logs_provider("/tmp/kalamdb-test/logs");
        system_tables.set_server_logs_provider(Arc::new(server_logs_provider));

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
        let node_id = Arc::new(NodeId::new(22));

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
        let manifest_service = Arc::new(crate::manifest::ManifestService::new_with_registries(
            storage_backend.clone(),
            "./data/storage".to_string(),
            config.manifest_cache.clone(),
            schema_registry.clone(),
            storage_registry.clone(),
        ));

        // Create RaftExecutor with single-node config for tests
        // This uses the same code path as production (unified Raft mode)
        let raft_config = kalamdb_raft::manager::RaftManagerConfig::for_single_node(
            "kalamdb-test".to_string(),
            "127.0.0.1:8080".to_string(),
        );
        let manager = Arc::new(kalamdb_raft::manager::RaftManager::new(raft_config));
        let executor: Arc<dyn CommandExecutor> = Arc::new(kalamdb_raft::RaftExecutor::new(manager));
        
        // Create unified applier for tests
        let applier = crate::applier::create_applier();

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
            executor,
            applier,
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
            .read()
            .expect("APP_CONTEXT lock poisoned")
            .as_ref()
            .expect("AppContext not initialized")
            .clone()
    }

    /// Try to get the AppContext singleton without panicking
    pub fn try_get() -> Option<Arc<AppContext>> {
        APP_CONTEXT
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
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

    /// Get the command executor (Phase 16 - Raft replication foundation)
    ///
    /// Returns the appropriate executor based on server mode:
    /// - Standalone: StandaloneExecutor (direct provider calls, zero overhead)
    /// - Cluster: RaftExecutor (commands go through Raft consensus)
    ///
    /// Handlers should use this instead of directly calling providers to enable
    /// transparent clustering support.
    pub fn executor(&self) -> Arc<dyn CommandExecutor> {
        self.executor.clone()
    }

    /// Check if this node is running in cluster mode
    pub fn is_cluster_mode(&self) -> bool {
        self.executor.is_cluster_mode()
    }

    /// Get the unified applier (Phase 19 - Unified Command Applier)
    ///
    /// Returns the applier that handles all database commands.
    /// All mutations flow through this applier, regardless of mode.
    pub fn applier(&self) -> Arc<dyn UnifiedApplier> {
        self.applier.clone()
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
            .map(|_| ())  // Discard the message, just return ()
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

    /// Get the server start time instant (for computing uptime)
    pub fn server_start_time(&self) -> Instant {
        self.server_start_time
    }

    /// Compute current system metrics snapshot
    ///
    /// Returns a vector of (metric_name, metric_value) tuples for display
    /// in system.stats virtual view.
    pub fn compute_metrics(&self) -> Vec<(String, String)> {
        crate::metrics::runtime::compute_metrics(self)
    }

    /// Log a concise snapshot of runtime metrics to the console.
    pub fn log_runtime_metrics(&self) {
        let runtime = collect_runtime_metrics(self.server_start_time);
        log::info!("Runtime metrics: {}", runtime.to_log_string());
    }
}
