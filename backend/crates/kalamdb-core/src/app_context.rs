//! AppContext singleton for KalamDB (Phase 5, T204)
//! 
//! Provides global access to all core resources with simplified 3-parameter initialization.
//! Uses constants from kalamdb_commons for table prefixes.

use crate::schema_registry::SchemaRegistry;
// use crate::jobs::UnifiedJobsManager; // TODO: Implement UnifiedJobsManager
use crate::jobs::executors::{
    BackupExecutor, CleanupExecutor, CompactExecutor, FlushExecutor,
    JobRegistry, RestoreExecutor, RetentionExecutor, StreamEvictionExecutor,
    UserCleanupExecutor,
};
use crate::live_query::LiveQueryManager;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::storage::storage_registry::StorageRegistry;
use crate::tables::system::registry::SystemTablesRegistry;
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use datafusion::catalog::SchemaProvider;
use datafusion::prelude::SessionContext;
use kalamdb_commons::{NodeId, constants::ColumnFamilyNames};
use kalamdb_store::StorageBackend;
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
}

impl std::fmt::Debug for AppContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppContext")
            .field("schema_cache", &"Arc<SchemaCache>")
            .field("schema_registry", &"Arc<SchemaRegistry>")
            .field("user_table_store", &"Arc<UserTableStore>")
            .field("shared_table_store", &"Arc<SharedTableStore>")
            .field("stream_table_store", &"Arc<StreamTableStore>")
            .field("storage_backend", &"Arc<dyn StorageBackend>")
            .field("job_manager", &"Arc<UnifiedJobsManager>")
            .field("live_query_manager", &"Arc<LiveQueryManager>")
            .field("storage_registry", &"Arc<StorageRegistry>")
            .field("session_factory", &"Arc<DataFusionSessionFactory>")
            .field("base_session_context", &"Arc<SessionContext>")
            .field("system_tables", &"Arc<SystemTablesRegistry>")
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
    /// AppContext::init(
    ///     backend,
    ///     node_id,
    ///     "data/storage".to_string(),
    /// );
    /// ```
    pub fn init(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
    ) -> Arc<AppContext> {
        let node_id = Arc::new(node_id); // Wrap NodeId in Arc for zero-copy sharing (FR-000)
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
                let system_tables = Arc::new(SystemTablesRegistry::new(
                    storage_backend.clone(),
                ));

                // Create storage registry (uses StoragesTableProvider from system_tables)
                let storage_registry = Arc::new(StorageRegistry::new(
                    system_tables.storages(),
                    storage_base_path,
                ));

                // Create schema cache (Phase 10 unified cache)
                let schema_cache = Arc::new(SchemaRegistry::new(10000, Some(storage_registry.clone())));

                // Register all system tables in DataFusion
                let session_factory = Arc::new(DataFusionSessionFactory::new()
                    .expect("Failed to create DataFusion session factory"));
                let base_session_context = Arc::new(session_factory.create_session());

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

                // SchemaRegistry is just an alias for SchemaCache (Phase 5 complete)
                // No separate schema_store needed - persistence methods use AppContext to access TablesTableProvider
                let schema_registry = schema_cache.clone();

                // NOW wire up information_schema providers with schema_registry
                system_tables.set_information_schema_dependencies(schema_registry.clone());

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
                let job_manager = Arc::new(crate::jobs::JobsManager::new(
                    jobs_provider,
                    job_registry,
                ));

                // Create live query manager
                let live_query_manager = Arc::new(LiveQueryManager::new(
                    system_tables.live_queries(),
                    schema_registry.clone(),
                    (*node_id).clone(), // Dereference Arc<NodeId> to NodeId for LiveQueryManager
                    Some(user_table_store.clone()),
                    Some(shared_table_store.clone()),
                    Some(stream_table_store.clone()),
                ));

                Arc::new(AppContext {
                    node_id,
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
                })
            })
            .clone()
    }

    /// Get the AppContext singleton
    ///
    /// # Panics
    /// Panics if AppContext::init() has not been called yet
    pub fn get() -> Arc<AppContext> {
        APP_CONTEXT.get().expect("AppContext not initialized").clone()
    }

    /// Try to get the AppContext singleton without panicking
    pub fn try_get() -> Option<Arc<AppContext>> {
        APP_CONTEXT.get().map(|ctx| ctx.clone())
    }

    // ===== Getters =====
    
    pub fn schema_cache(&self) -> Arc<SchemaRegistry> {
        self.schema_registry.clone()
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
    
    pub fn create_session(&self) -> Arc<SessionContext> {
        Arc::new(self.session_factory.create_session())
    }
    
    pub fn system_tables(&self) -> Arc<SystemTablesRegistry> {
        self.system_tables.clone()
    }
    
    // ===== Convenience methods for backward compatibility =====
    
    /// Insert a job into the jobs table
    /// 
    /// Convenience wrapper for system_tables().jobs().create_job()
    pub fn insert_job(&self, job: &kalamdb_commons::system::Job) -> Result<(), crate::error::KalamDbError> {
        self.system_tables.jobs().create_job(job.clone())
    }
    
    /// Scan all jobs from the jobs table
    ///
    /// Convenience wrapper for system_tables().jobs().list_jobs()
    pub fn scan_all_jobs(&self) -> Result<Vec<kalamdb_commons::system::Job>, crate::error::KalamDbError> {
        self.system_tables.jobs().list_jobs()
    }
}
