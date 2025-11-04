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

use crate::catalog::schema_cache::SchemaCache;
use crate::catalog::schema_registry::SchemaRegistry;
use crate::jobs::{JobManager, TokioJobManager};
use crate::live_query::LiveQueryManager;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::storage::storage_registry::StorageRegistry;
use crate::tables::system::registry::SystemTablesRegistry;
use crate::tables::system::schemas::TableSchemaStore;
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use datafusion::prelude::SessionContext;
use kalamdb_commons::{NodeId, constants};
use kalamdb_sql::KalamSql;
use kalamdb_store::StorageBackend;
use std::sync::{Arc, OnceLock};

static APP_CONTEXT: OnceLock<Arc<AppContext>> = OnceLock::new();

/// Configuration for AppContext initialization
#[derive(Clone)]
pub struct AppContextConfig {
    /// Node ID for distributed operations
    pub node_id: NodeId,
    /// User table storage prefix
    pub user_table_prefix: String,
    /// Shared table storage prefix
    pub shared_table_prefix: String,
    /// Stream table storage prefix
    pub stream_table_prefix: String,
    /// Base path for storage files
    pub storage_base_path: String,
    /// Maximum size of schema cache
    pub schema_cache_max_size: usize,
}

impl Default for AppContextConfig {
    fn default() -> Self {
        Self {
            node_id: NodeId::new("default-node".to_string()),
            user_table_prefix: "user_table".to_string(),
            shared_table_prefix: "shared_table".to_string(),
            stream_table_prefix: "stream_table".to_string(),
            storage_base_path: "data/storage".to_string(),
            schema_cache_max_size: 10000,
        }
    }
}

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
    
    /// Schema registry (unified: cache + persistence + Arrow memoization)
    /// **Phase 5 Enhancement**: Consolidates TableSchemaStore for single source of truth
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
    
    // ===== System Tables Registry =====
    /// Centralized registry for all system table providers (Phase 5 completion)
    /// Replaces 10 individual provider fields with single registry pattern
    system_tables: Arc<SystemTablesRegistry>,
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
            .field("job_manager", &"Arc<dyn JobManager>")
            .field("live_query_manager", &"Arc<LiveQueryManager>")
            .field("storage_registry", &"Arc<StorageRegistry>")
            .field("session_factory", &"Arc<DataFusionSessionFactory>")
            .field("base_session_context", &"Arc<SessionContext>")
            .field("system_tables", &"Arc<SystemTablesRegistry>")
            .finish()
    }
}

impl AppContext {
    /// Initialize the AppContext singleton with automatic dependency creation
    ///
    /// This is the simplified initialization that creates all dependencies internally.
    /// Perfect for tests and simple setups.
    ///
    /// # Arguments
    /// * `storage_backend` - Storage backend abstraction (RocksDB implementation)
    ///
    /// # Panics
    /// Panics if called more than once (OnceCell::set returns Err on second call)
    pub fn init(storage_backend: Arc<dyn StorageBackend>) -> Arc<AppContext> {
        Self::init_with_config(storage_backend, Default::default())
    }
    
    /// Initialize the AppContext singleton with custom configuration
    ///
    /// # Arguments
    /// * `storage_backend` - Storage backend abstraction
    /// * `config` - Configuration options for AppContext initialization
    ///
    /// # Panics
    /// Panics if called more than once
    pub fn init_with_config(
        storage_backend: Arc<dyn StorageBackend>,
        config: AppContextConfig,
    ) -> Arc<AppContext> {
        // Create KalamSQL adapter
        let kalam_sql = Arc::new(
            KalamSql::new(storage_backend.clone())
                .expect("Failed to initialize KalamSQL")
        );
        
        // Create table stores
        let user_table_store = Arc::new(UserTableStore::new(
            storage_backend.clone(),
            config.user_table_prefix.clone(),
        ));
        let shared_table_store = Arc::new(SharedTableStore::new(
            storage_backend.clone(),
            config.shared_table_prefix.clone(),
        ));
        let stream_table_store = Arc::new(StreamTableStore::new(
            storage_backend.clone(),
            config.stream_table_prefix.clone(),
        ));
        
        // Create storage registry
        let storage_registry = Arc::new(StorageRegistry::new(
            kalam_sql.clone(),
            config.storage_base_path.clone(),
        ));
        
        // Create unified schema cache
        let schema_cache = Arc::new(SchemaCache::new(
            config.schema_cache_max_size,
            Some(storage_registry.clone()),
        ));
        
        // Register system tables and get schema store
        use datafusion::catalog::memory::MemorySchemaProvider;
        let system_schema = Arc::new(MemorySchemaProvider::new());
        
        let providers = crate::system_table_registration::register_system_tables(
            &system_schema,
            storage_backend.clone(),
        ).expect("Failed to register system tables");
        
        let schema_store = providers.schema_store;
        
        // Create SchemaRegistry with both cache and persistent store
        let schema_registry = Arc::new(SchemaRegistry::new(
            schema_cache.clone(),
            schema_store,
        ));
        
        // Create managers
        let job_manager: Arc<dyn JobManager> = Arc::new(crate::jobs::TokioJobManager::new());
        
        let live_query_manager = Arc::new(LiveQueryManager::new(
            kalam_sql.clone(),
            config.node_id.clone(),
            Some(user_table_store.clone()),
            Some(shared_table_store.clone()),
            Some(stream_table_store.clone()),
        ));
        
        // Create DataFusion session factory and base context
        let session_factory = Arc::new(
            DataFusionSessionFactory::new()
                .expect("Failed to create DataFusion session factory")
        );
        let base_session_context = Arc::new(session_factory.create_session());
        
        // Register system schema in base session context
        let catalog_name = "kalam";
        base_session_context
            .catalog(catalog_name)
            .expect("Failed to get kalam catalog")
            .register_schema("system", system_schema.clone())
            .expect("Failed to register system schema");
        
        // Create system tables registry
        let system_tables = Arc::new(SystemTablesRegistry::new(
            storage_backend.clone(),
            kalam_sql.clone(),
        ));
        
        let ctx = Arc::new(AppContext {
            schema_cache,
            schema_registry,
            user_table_store,
            shared_table_store,
            stream_table_store,
            kalam_sql,
            storage_backend,
            job_manager,
            live_query_manager,
            storage_registry,
            session_factory,
            base_session_context,
            system_tables,
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
    
    /// Get the schema registry (unified: cache + persistence + Arrow memoization)
    ///
    /// **Preferred method** for schema/table access. Provides:
    /// - get_table_data() - cached table metadata
    /// - get_table_definition() - read-through (cache → store fallback)
    /// - get_arrow_schema() - memoized Arrow schema (zero-allocation on cache hit)
    /// - get_user_table_shared() - cached UserTableShared instances
    /// - put_table_definition() - write-through (persist + invalidate cache)
    /// - delete_table_definition() - write-through (delete + invalidate cache)
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
    
    // ===== Manager Getters =====
    
    /// Get the job manager
    pub fn job_manager(&self) -> Arc<dyn JobManager> {
        self.job_manager.clone()
    }
    
    /// Get the live query manager
    pub fn live_query_manager(&self) -> Arc<LiveQueryManager> {
        self.live_query_manager.clone()
    }
    
    /// Get the node_id from the LiveQueryManager
    pub fn node_id(&self) -> NodeId {
        self.live_query_manager.node_id().clone()
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
    
    // ===== System Tables Registry =====
    
    /// Get the system tables registry
    ///
    /// Provides access to all 10 system table providers:
    /// - system.users, system.jobs, system.namespaces, system.storages
    /// - system.live_queries, system.tables, system.audit_logs, system.stats
    /// - information_schema.tables, information_schema.columns
    pub fn system_tables(&self) -> Arc<SystemTablesRegistry> {
        self.system_tables.clone()
    }
}
