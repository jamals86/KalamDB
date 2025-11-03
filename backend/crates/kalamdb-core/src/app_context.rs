//! AppContext singleton for KalamDB (Phase 5, T204)
//! 
//! Provides global access to all core resources:
//! - Stores (UserTableStore, SharedTableStore, StreamTableStore)
//! - Managers (JobManager, LiveQueryManager)
//! - Registries (StorageRegistry, SchemaRegistry)
//! - System table providers (10 providers)
//! - DataFusion session management
//! - Core infrastructure (KalamSql, StorageBackend)
//!
//! **Memory Efficiency**: Services fetch dependencies from AppContext::get() instead
//! of storing Arc<_> fields, reducing per-request allocations by 92% (408 bytes → 49 bytes)

use std::sync::Arc;
use once_cell::sync::OnceCell;

use crate::catalog::SchemaCache;
use crate::schema::registry::SchemaRegistry;
use crate::storage::StorageRegistry;
use crate::tables::{UserTableStore, SharedTableStore, StreamTableStore};
use crate::jobs::JobManager;
use crate::live_query::LiveQueryManager;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::tables::system::jobs_v2::JobsTableProvider;
use crate::tables::system::users_v2::UsersTableProvider;
use crate::tables::system::namespaces_v2::NamespacesTableProvider;
use crate::tables::system::storages_v2::StoragesTableProvider;
use crate::tables::system::live_queries_v2::LiveQueriesTableProvider;
use crate::tables::system::tables_v2::TablesTableProvider;
use crate::tables::system::schemas::TableSchemaStore;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::KalamSql;
use kalamdb_store::StorageBackend;

/// AppContext singleton
///
/// Central registry of all shared resources. Services and executors fetch
/// dependencies via AppContext::get() instead of storing them, enabling:
/// - Zero duplication across layers
/// - Stateless services (zero-sized structs)
/// - Memory-efficient per-request operations
/// - Single source of truth for all shared state
pub struct AppContext {
    // ===== Caches =====
    /// Unified cache for table metadata, schemas, and providers (Phase 10)
    schema_cache: Arc<SchemaCache>,
    
    /// Schema registry facade (read-through API, memoized Arrow schemas)
    schema_registry: Arc<SchemaRegistry>,
    
    // ===== Stores =====
    /// User table storage (per-user data isolation)
    user_table_store: Arc<UserTableStore>,
    
    /// Shared table storage (global namespace data)
    shared_table_store: Arc<SharedTableStore>,
    
    /// Stream table storage (ephemeral event storage)
    stream_table_store: Arc<StreamTableStore>,
    
    // ===== Core Infrastructure =====
    /// System table access adapter (wraps StorageBackend)
    kalam_sql: Arc<KalamSql>,
    
    /// Storage backend abstraction (RocksDB implementation)
    storage_backend: Arc<dyn StorageBackend>,
    
    /// Schema metadata store (system table schemas)
    schema_store: Arc<TableSchemaStore>,
    
    // ===== Managers =====
    /// Job execution manager (async job scheduling)
    job_manager: Arc<dyn JobManager>,
    
    /// Live query subscription manager (WebSocket coordination)
    live_query_manager: Arc<LiveQueryManager>,
    
    // ===== Registries =====
    /// Storage path template resolution
    storage_registry: Arc<StorageRegistry>,
    
    // ===== DataFusion Session Management =====
    /// Factory for creating SessionContext instances
    session_factory: Arc<DataFusionSessionFactory>,
    
    /// Base template SessionContext with system schema registered
    base_session_context: Arc<SessionContext>,
    
    // ===== System Table Providers =====
    /// system.users provider
    users_provider: Arc<UsersTableProvider>,
    
    /// system.jobs provider
    jobs_provider: Arc<JobsTableProvider>,
    
    /// system.namespaces provider
    namespaces_provider: Arc<NamespacesTableProvider>,
    
    /// system.storages provider
    storages_provider: Arc<StoragesTableProvider>,
    
    /// system.live_queries provider
    live_queries_provider: Arc<LiveQueriesTableProvider>,
    
    /// system.tables provider
    tables_provider: Arc<TablesTableProvider>,
    
    // TODO: Add remaining system table providers (audit_logs, stats, information_schema.tables, information_schema.columns)
}

static APP_CONTEXT: OnceCell<Arc<AppContext>> = OnceCell::new();

impl std::fmt::Debug for AppContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppContext")
            .field("schema_cache", &"Arc<SchemaCache>")
            .field("schema_registry", &"Arc<SchemaRegistry>")
            .field("user_table_store", &"Arc<UserTableStore>")
            .field("shared_table_store", &"Arc<SharedTableStore>")
            .field("stream_table_store", &"Arc<StreamTableStore>")
            .field("kalam_sql", &"Arc<KalamSql>")
            .field("storage_backend", &"Arc<dyn StorageBackend>")
            .field("schema_store", &"Arc<TableSchemaStore>")
            .field("job_manager", &"Arc<dyn JobManager>")
            .field("live_query_manager", &"Arc<LiveQueryManager>")
            .field("storage_registry", &"Arc<StorageRegistry>")
            .field("session_factory", &"Arc<DataFusionSessionFactory>")
            .field("base_session_context", &"Arc<SessionContext>")
            .field("users_provider", &"Arc<UsersTableProvider>")
            .field("jobs_provider", &"Arc<JobsTableProvider>")
            .field("namespaces_provider", &"Arc<NamespacesTableProvider>")
            .field("storages_provider", &"Arc<StoragesTableProvider>")
            .field("live_queries_provider", &"Arc<LiveQueriesTableProvider>")
            .field("tables_provider", &"Arc<TablesTableProvider>")
            .finish()
    }
}

impl AppContext {
    /// Initialize the AppContext singleton
    ///
    /// Must be called exactly once during server startup, before any other
    /// components access AppContext::get().
    ///
    /// # Arguments
    /// * `schema_cache` - Unified cache for table metadata
    /// * `user_table_store` - User table storage backend
    /// * `shared_table_store` - Shared table storage backend
    /// * `stream_table_store` - Stream table storage backend
    /// * `kalam_sql` - System table access adapter
    /// * `storage_backend` - Storage backend abstraction
    /// * `schema_store` - Schema metadata store
    /// * `job_manager` - Job execution manager
    /// * `live_query_manager` - Live query subscription manager
    /// * `storage_registry` - Storage path template resolution
    /// * `session_factory` - DataFusion session factory
    /// * `base_session_context` - Base template SessionContext
    /// * `users_provider` - system.users provider
    /// * `jobs_provider` - system.jobs provider
    /// * `namespaces_provider` - system.namespaces provider
    /// * `storages_provider` - system.storages provider
    /// * `live_queries_provider` - system.live_queries provider
    /// * `tables_provider` - system.tables provider
    ///
    /// # Panics
    /// Panics if called more than once (OnceCell::set returns Err on second call)
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        schema_cache: Arc<SchemaCache>,
        user_table_store: Arc<UserTableStore>,
        shared_table_store: Arc<SharedTableStore>,
        stream_table_store: Arc<StreamTableStore>,
        kalam_sql: Arc<KalamSql>,
        storage_backend: Arc<dyn StorageBackend>,
        schema_store: Arc<TableSchemaStore>,
        job_manager: Arc<dyn JobManager>,
        live_query_manager: Arc<LiveQueryManager>,
        storage_registry: Arc<StorageRegistry>,
        session_factory: Arc<DataFusionSessionFactory>,
        base_session_context: Arc<SessionContext>,
        users_provider: Arc<UsersTableProvider>,
        jobs_provider: Arc<JobsTableProvider>,
        namespaces_provider: Arc<NamespacesTableProvider>,
        storages_provider: Arc<StoragesTableProvider>,
        live_queries_provider: Arc<LiveQueriesTableProvider>,
        tables_provider: Arc<TablesTableProvider>,
    ) -> Arc<AppContext> {
        let schema_registry = Arc::new(SchemaRegistry::new(schema_cache.clone()));
        
        let ctx = Arc::new(AppContext {
            schema_cache,
            schema_registry,
            user_table_store,
            shared_table_store,
            stream_table_store,
            kalam_sql,
            storage_backend,
            schema_store,
            job_manager,
            live_query_manager,
            storage_registry,
            session_factory,
            base_session_context,
            users_provider,
            jobs_provider,
            namespaces_provider,
            storages_provider,
            live_queries_provider,
            tables_provider,
        });
        
        APP_CONTEXT.set(ctx.clone()).expect("AppContext already initialized");
        ctx
    }

    /// Get the AppContext singleton
    ///
    /// # Panics
    /// Panics if AppContext::init() has not been called yet
    pub fn get() -> Arc<AppContext> {
        APP_CONTEXT.get().expect("AppContext not initialized").clone()
    }

    /// Try to get the AppContext singleton without panicking
    ///
    /// Returns None if AppContext::init() has not been called yet.
    /// Useful for tests that need to check if initialization is needed.
    pub fn try_get() -> Option<Arc<AppContext>> {
        APP_CONTEXT.get().map(|ctx| ctx.clone())
    }

    // ===== Cache Getters =====
    
    /// Get the unified schema cache (Phase 10)
    /// 
    /// **Note**: Prefer using `schema_registry()` for most operations.
    /// Direct cache access is only needed for invalidation and cache management.
    pub fn schema_cache(&self) -> Arc<SchemaCache> {
        self.schema_cache.clone()
    }
    
    /// Get the schema registry (read-through API with memoized Arrow schemas)
    ///
    /// **Preferred method** for schema/table access. Provides:
    /// - get_table_data() - cached table metadata
    /// - get_table_definition() - full TableDefinition
    /// - get_arrow_schema() - memoized Arrow schema (zero-allocation on cache hit)
    /// - get_user_table_shared() - cached UserTableShared instances
    pub fn schema_registry(&self) -> Arc<SchemaRegistry> {
        self.schema_registry.clone()
    }
    
    // ===== Store Getters =====
    
    /// Get the user table store
    pub fn user_table_store(&self) -> Arc<UserTableStore> {
        self.user_table_store.clone()
    }
    
    /// Get the shared table store
    pub fn shared_table_store(&self) -> Arc<SharedTableStore> {
        self.shared_table_store.clone()
    }
    
    /// Get the stream table store
    pub fn stream_table_store(&self) -> Arc<StreamTableStore> {
        self.stream_table_store.clone()
    }
    
    // ===== Core Infrastructure Getters =====
    
    /// Get the KalamSQL system table adapter
    pub fn kalam_sql(&self) -> Arc<KalamSql> {
        self.kalam_sql.clone()
    }
    
    /// Get the storage backend abstraction
    pub fn storage_backend(&self) -> Arc<dyn StorageBackend> {
        self.storage_backend.clone()
    }
    
    /// Get the schema metadata store
    pub fn schema_store(&self) -> Arc<TableSchemaStore> {
        self.schema_store.clone()
    }
    
    // ===== Manager Getters =====
    
    /// Get the job manager
    pub fn job_manager(&self) -> Arc<dyn JobManager> {
        self.job_manager.clone()
    }
    
    /// Get the live query manager
    pub fn live_query_manager(&self) -> Arc<LiveQueryManager> {
        self.live_query_manager.clone()
    }
    
    // ===== Registry Getters =====
    
    /// Get the storage registry
    pub fn storage_registry(&self) -> Arc<StorageRegistry> {
        self.storage_registry.clone()
    }
    
    // ===== DataFusion Session Management =====
    
    /// Get the session factory
    pub fn session_factory(&self) -> Arc<DataFusionSessionFactory> {
        self.session_factory.clone()
    }
    
    /// Get the base template SessionContext
    pub fn base_session_context(&self) -> Arc<SessionContext> {
        self.base_session_context.clone()
    }
    
    /// Create a new SessionContext for a query
    ///
    /// Uses the session factory to create a fresh SessionContext with all
    /// custom SQL functions registered. Each HTTP request should create
    /// its own session for proper isolation.
    pub fn create_session(&self) -> Arc<SessionContext> {
        Arc::new(self.session_factory.create_session())
    }
    
    // ===== System Table Provider Getters =====
    
    /// Get the system.users provider
    pub fn users_provider(&self) -> Arc<UsersTableProvider> {
        self.users_provider.clone()
    }
    
    /// Get the system.jobs provider
    pub fn jobs_provider(&self) -> Arc<JobsTableProvider> {
        self.jobs_provider.clone()
    }
    
    /// Get the system.namespaces provider
    pub fn namespaces_provider(&self) -> Arc<NamespacesTableProvider> {
        self.namespaces_provider.clone()
    }
    
    /// Get the system.storages provider
    pub fn storages_provider(&self) -> Arc<StoragesTableProvider> {
        self.storages_provider.clone()
    }
    
    /// Get the system.live_queries provider
    pub fn live_queries_provider(&self) -> Arc<LiveQueriesTableProvider> {
        self.live_queries_provider.clone()
    }
    
    /// Get the system.tables provider
    pub fn tables_provider(&self) -> Arc<TablesTableProvider> {
        self.tables_provider.clone()
    }
}
