//! AppContext singleton for KalamDB (Phase 5, T204)
//! 
//! Provides global access to all core resources with simplified 3-parameter initialization.
//! Uses constants from kalamdb_commons for table prefixes.

use crate::catalog::SchemaCache;
use crate::schema::registry::SchemaRegistry;
use crate::jobs::{JobManager, TokioJobManager};
use crate::live_query::LiveQueryManager;
use crate::sql::datafusion_session::DataFusionSessionFactory;
use crate::storage::storage_registry::StorageRegistry;
use crate::tables::system::registry::SystemTablesRegistry;
use crate::tables::system::schemas::TableSchemaStore;
use crate::tables::{SharedTableStore, StreamTableStore, UserTableStore};
use datafusion::catalog::SchemaProvider;
use datafusion::prelude::SessionContext;
use kalamdb_commons::{NodeId, constants::ColumnFamilyNames};
use kalamdb_sql::KalamSql;
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
    // ===== Caches =====
    schema_cache: Arc<SchemaCache>,
    schema_registry: Arc<SchemaRegistry>,
    
    // ===== Stores =====
    user_table_store: Arc<UserTableStore>,
    shared_table_store: Arc<SharedTableStore>,
    stream_table_store: Arc<StreamTableStore>,
    
    // ===== Core Infrastructure =====
    kalam_sql: Arc<KalamSql>,
    storage_backend: Arc<dyn StorageBackend>,
    
    // ===== Managers =====
    job_manager: Arc<dyn JobManager>,
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
    /// Initialize AppContext singleton with 3 parameters
    ///
    /// Creates all dependencies internally using constants from kalamdb_commons.
    /// Table prefixes are read from constants::USER_TABLE_PREFIX, etc.
    ///
    /// # Parameters
    /// - `storage_backend`: Storage abstraction (RocksDB implementation)
    /// - `node_id`: Node identifier for distributed coordination
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
    /// AppContext::init(
    ///     backend,
    ///     NodeId::new("prod-node-1".to_string()),
    ///     "data/storage".to_string(),
    /// );
    /// ```
    pub fn init(
        storage_backend: Arc<dyn StorageBackend>,
        node_id: NodeId,
        storage_base_path: String,
    ) -> Arc<AppContext> {
        APP_CONTEXT
            .get_or_init(|| {
                // Create KalamSQL adapter
                let kalam_sql = Arc::new(
                    KalamSql::new(storage_backend.clone())
                        .expect("Failed to initialize KalamSQL"),
                );

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

                // Create storage registry
                let storage_registry = Arc::new(StorageRegistry::new(
                    kalam_sql.clone(),
                    storage_base_path,
                ));

                // Create schema cache (Phase 10 unified cache)
                let schema_cache = Arc::new(SchemaCache::new(10000, Some(storage_registry.clone())));

                // Create system table providers registry (needs kalam_sql too)
                let system_tables = Arc::new(SystemTablesRegistry::new(
                    storage_backend.clone(),
                    kalam_sql.clone(),
                ));

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

                // Create schema store (for table definitions)
                let schema_store = Arc::new(TableSchemaStore::new(storage_backend.clone()));

                // Create schema registry (Phase 5: unified cache + persistence)
                let schema_registry = Arc::new(SchemaRegistry::new(
                    schema_cache.clone(),
                    schema_store,
                ));

                // Create job manager
                let job_manager: Arc<dyn JobManager> = Arc::new(TokioJobManager::new());

                // Create live query manager
                let live_query_manager = Arc::new(LiveQueryManager::new(
                    kalam_sql.clone(),
                    node_id,
                    Some(user_table_store.clone()),
                    Some(shared_table_store.clone()),
                    Some(stream_table_store.clone()),
                ));

                Arc::new(AppContext {
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
    
    pub fn schema_cache(&self) -> Arc<SchemaCache> {
        self.schema_cache.clone()
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
    
    pub fn kalam_sql(&self) -> Arc<KalamSql> {
        self.kalam_sql.clone()
    }
    
    pub fn storage_backend(&self) -> Arc<dyn StorageBackend> {
        self.storage_backend.clone()
    }
    
    pub fn job_manager(&self) -> Arc<dyn JobManager> {
        self.job_manager.clone()
    }
    
    pub fn live_query_manager(&self) -> Arc<LiveQueryManager> {
        self.live_query_manager.clone()
    }
    
    pub fn node_id(&self) -> NodeId {
        self.live_query_manager.node_id().clone()
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
}
