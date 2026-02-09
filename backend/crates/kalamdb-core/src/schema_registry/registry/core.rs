//! Core implementation of SchemaRegistry

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::live::models::ChangeNotification;
use crate::schema_registry::cached_table_data::CachedTableData;
use dashmap::DashMap;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::TableProvider;
use datafusion::logical_expr::expr::ScalarFunction as ScalarFunctionExpr;
use datafusion::logical_expr::Expr;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::conversions::json_value_to_scalar;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{StorageId, TableId, TableVersionId};
use kalamdb_commons::schemas::{ColumnDefault, TableOptions, TableType};
use kalamdb_commons::TableAccess;
use kalamdb_system::{NotificationService, SchemaRegistry as SchemaRegistryTrait};
// use kalamdb_system::NotificationService as NotificationServiceTrait;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Lightweight table info for file operations
#[derive(Debug, Clone)]
pub struct TableEntry {
    /// Storage ID for the table
    pub storage_id: StorageId,
    /// Table type (User or Shared)
    pub table_type: TableType,
    //TODO: Add access permissions info?
    pub access_level: Option<TableAccess>,
}

/// Unified schema cache for table metadata, schemas, and providers
///
/// Uses DashMap for all table data including:
/// - Table definitions and metadata
/// - Memoized Arrow schemas
/// - DataFusion TableProvider instances
///
pub struct SchemaRegistry {
    /// App context for accessing system components (set via set_app_context)
    app_context: OnceLock<Arc<AppContext>>,

    /// Cache for table data (latest versions)
    table_cache: DashMap<TableId, Arc<CachedTableData>>,

    /// Cache for specific table versions (for reading old Parquet files)
    version_cache: DashMap<TableVersionId, Arc<CachedTableData>>,

    /// DataFusion base session context for table registration (set once during init)
    base_session_context: OnceLock<Arc<datafusion::prelude::SessionContext>>,
}

impl std::fmt::Debug for SchemaRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRegistry")
            .field("table_cache_size", &self.table_cache.len())
            .field("version_cache_size", &self.version_cache.len())
            .field("base_session_context_set", &self.base_session_context.get().is_some())
            .field("app_context_set", &self.app_context.get().is_some())
            .finish()
    }
}

impl SchemaRegistry {
    /// Create a new schema cache with specified maximum size
    pub fn new(_max_size: usize) -> Self {
        Self {
            app_context: OnceLock::new(),
            table_cache: DashMap::new(),
            version_cache: DashMap::new(),
            base_session_context: OnceLock::new(),
        }
    }

    /// Set the DataFusion base session context for table registration
    pub fn set_base_session_context(&self, session: Arc<datafusion::prelude::SessionContext>) {
        let _ = self.base_session_context.set(session);
    }

    /// Set the AppContext (break circular dependency)
    pub fn set_app_context(&self, app_context: Arc<AppContext>) {
        let _ = self.app_context.set(app_context);
    }

    /// Get the AppContext (panics if not set)
    fn app_context(&self) -> &Arc<AppContext> {
        self.app_context.get().expect("AppContext not set on SchemaRegistry")
    }

    /// Initialize registry by loading all existing tables from storage
    ///
    /// This should be called once at system startup.
    pub fn initialize_tables(&self) -> Result<(), KalamDbError> {
        // Scan all table definitions
        let all_defs = self.scan_all_table_definitions()?;

        if all_defs.is_empty() {
            log::debug!("SchemaRegistry initialized: no existing tables found");
            return Ok(());
        }

        log::debug!("SchemaRegistry initialized: loading {} existing tables...", all_defs.len());

        let mut loaded_count = 0;
        let mut failed_count = 0;

        for def in all_defs {
            if def.table_type == kalamdb_commons::models::schemas::TableType::System {
                continue; // System tables are handled separately
            }

            let table_name = def.table_name.clone();
            match self.put(def) {
                Ok(_) => loaded_count += 1,
                Err(e) => {
                    log::error!("Failed to load table {}: {}", table_name, e);
                    failed_count += 1;
                },
            }
        }

        log::info!(
            "SchemaRegistry initialized. Loaded: {}, Failed: {}",
            loaded_count,
            failed_count
        );
        Ok(())
    }

    // ===== Basic Cache Methods =====

    /// Get cached table data for a table (latest version)
    pub fn get(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        self.table_cache.get(table_id).map(|entry| entry.value().clone())
    }

    /// Get table entry info for file operations.
    ///
    /// Returns a lightweight struct with storage_id and table_type
    /// needed for file upload/download operations.
    pub fn get_table_entry(&self, table_id: &TableId) -> Option<TableEntry> {
        self.table_cache.get(table_id).map(|entry| {
            let cached = entry.value();
            TableEntry {
                storage_id: cached.storage_id.clone(),
                table_type: cached.table.table_type.into(),
                access_level: match &cached.table.table_options {
                    TableOptions::Shared(opts) => {
                        Some(opts.access_level.clone().unwrap_or(TableAccess::Private))
                    },
                    TableOptions::User(_) | TableOptions::System(_) | TableOptions::Stream(_) => {
                        None
                    },
                },
            }
        })
    }

    /// Register a new or updated table definition (CREATE/ALTER)
    ///
    /// This handles the full lifecycle:
    /// 1. Persisting to system.tables
    /// 2. Persisting version history
    /// 3. Updating the cache (and DataFusion registry)
    pub fn register_table(&self, table_def: TableDefinition) -> Result<(), KalamDbError> {
        let app_ctx = self.app_context();
        let table_id =
            TableId::from_strings(table_def.namespace_id.as_str(), table_def.table_name.as_str());

        log::info!("Registering table {} via SchemaRegistry", table_id);

        // 1. Persist to system keyspace
        let tables_provider = app_ctx.system_tables().tables();
        tables_provider
            .create_table(&table_id, &table_def)
            .into_kalamdb_error("Failed to persist table definition")?;

        // 2. Persist schema version
        tables_provider
            .put_versioned_schema(&table_id, &table_def)
            .into_kalamdb_error("Failed to persist schema version")?;

        // 3. Update cache
        self.put(table_def)?;

        Ok(())
    }

    /// Insert table definition into cache and create provider
    ///
    /// - Creates CachedTableData
    /// - Creates matching TableProvider (User/Shared/Stream)
    /// - Registers with DataFusion
    pub fn put(&self, table_def: TableDefinition) -> Result<(), KalamDbError> {
        let table_id =
            TableId::from_strings(table_def.namespace_id.as_str(), table_def.table_name.as_str());

        // 1. Create CachedTableData
        let cached_data = Arc::new(CachedTableData::new(Arc::new(table_def.clone())));
        let previous_entry = self.table_cache.insert(table_id.clone(), Arc::clone(&cached_data));

        // 2. Create TableProvider if not a system table
        if table_def.table_type != TableType::System {
            match self.create_table_provider(&table_def) {
                Ok(provider) => {
                    cached_data.set_provider(provider.clone());

                    // 3. Register with DataFusion immediately
                    // Note: We register BEFORE inserting into cache? No, AFTER/DURING
                    // DataFusion registration requires the provider.

                    self.register_with_datafusion(&table_id, provider)?;
                },
                Err(e) => {
                    log::error!("Failed to create provider for table {}: {}", table_id, e);
                    // We still insert the definition, but provider creation failed.
                    // This creates a "definition-only" cache entry which might be problematic for queries.
                    // But for system stability, maybe we should return error?
                    if let Some(previous) = previous_entry {
                        self.table_cache.insert(table_id, previous);
                    } else {
                        self.table_cache.remove(&table_id);
                    }
                    return Err(e);
                },
            }
        }

        Ok(())
    }

    fn build_column_defaults(&self, table_def: &TableDefinition) -> HashMap<String, Expr> {
        let base_state = self.app_context().base_session_context().state();
        let scalar_functions = base_state.scalar_functions();

        table_def
            .columns
            .iter()
            .filter_map(|column| {
                self.build_default_expr(&column.default_value, scalar_functions)
                    .map(|expr| (column.column_name.clone(), expr))
            })
            .collect()
    }

    fn build_default_expr(
        &self,
        default_value: &ColumnDefault,
        scalar_functions: &HashMap<String, Arc<datafusion::logical_expr::ScalarUDF>>,
    ) -> Option<Expr> {
        match default_value {
            ColumnDefault::None => None,
            ColumnDefault::Literal(json) => Some(Expr::Literal(json_value_to_scalar(json), None)),
            ColumnDefault::FunctionCall { name, args } => {
                let udf = Self::lookup_scalar_function(scalar_functions, name)?;
                let arg_exprs =
                    args.iter().map(|arg| Expr::Literal(json_value_to_scalar(arg), None)).collect();
                Some(Expr::ScalarFunction(ScalarFunctionExpr::new_udf(udf, arg_exprs)))
            },
        }
    }

    fn lookup_scalar_function(
        scalar_functions: &HashMap<String, Arc<datafusion::logical_expr::ScalarUDF>>,
        name: &str,
    ) -> Option<Arc<datafusion::logical_expr::ScalarUDF>> {
        if let Some(udf) = scalar_functions.get(name) {
            return Some(Arc::clone(udf));
        }

        let lower = name.to_lowercase();
        if let Some(udf) = scalar_functions.get(&lower) {
            return Some(Arc::clone(udf));
        }

        let upper = name.to_uppercase();
        scalar_functions.get(&upper).map(Arc::clone)
    }

    /// Internal helper to create TableProvider based on definition
    fn create_table_provider(
        &self,
        table_def: &TableDefinition,
    ) -> Result<Arc<dyn TableProvider + Send + Sync>, KalamDbError> {
        use crate::providers::{
            SharedTableProvider, StreamTableProvider, TableProviderCore, UserTableProvider,
        };
        use crate::schema_registry::TablesSchemaRegistryAdapter;
        use kalamdb_commons::schemas::TableOptions;
        use kalamdb_sharding::ShardRouter;
        use kalamdb_tables::{
            new_indexed_shared_table_store, new_indexed_user_table_store, new_stream_table_store,
            StreamTableStoreConfig,
        };

        let app_ctx = self.app_context();
        let table_id =
            TableId::from_strings(table_def.namespace_id.as_str(), table_def.table_name.as_str());
        let column_defaults = self.build_column_defaults(table_def);

        // Resolve PK field (required for User/Shared; Stream falls back to _seq)
        let pk_field = match table_def.table_type {
            TableType::Stream => table_def
                .columns
                .iter()
                .find(|c| c.is_primary_key)
                .map(|c| c.column_name.clone())
                .unwrap_or_else(|| SystemColumnNames::SEQ.to_string()),
            _ => table_def
                .columns
                .iter()
                .find(|c| c.is_primary_key)
                .map(|c| c.column_name.clone())
                .ok_or_else(|| {
                    KalamDbError::InvalidOperation(format!(
                        "Table {} has no primary key defined",
                        table_id
                    ))
                })?,
        };

        let tables_schema_registry =
            Arc::new(TablesSchemaRegistryAdapter::new(app_ctx.schema_registry()));

        // Build shared TableServices (one Arc for all provider types)
        let services = Arc::new(kalamdb_tables::utils::TableServices::new(
            tables_schema_registry.clone(),
            app_ctx.system_columns_service(),
            Some(app_ctx.storage_registry()),
            app_ctx.manifest_service(),
            Arc::clone(app_ctx.notification_service())
                as Arc<dyn NotificationService<Notification = ChangeNotification>>,
            app_ctx.clone(),
        ));

        // Get Arrow schema from registry (cached at core level)
        let arrow_schema = tables_schema_registry
            .get_arrow_schema(&table_id)
            .map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to get Arrow schema for {}: {}",
                    table_id, e
                ))
            })?;

        // Wrap table_def in Arc for sharing across core (avoids cloning TableDefinition)
        let table_def_arc = Arc::new(table_def.clone());

        match table_def.table_type {
            TableType::User => {
                let user_table_store = Arc::new(new_indexed_user_table_store(
                    app_ctx.storage_backend(),
                    &table_id,
                    &pk_field,
                ));

                let core = Arc::new(TableProviderCore::new(
                    table_def_arc,
                    services,
                    pk_field,
                    arrow_schema,
                    column_defaults,
                ));

                let provider = UserTableProvider::new(
                    core,
                    user_table_store,
                );
                Ok(Arc::new(provider))
            },
            TableType::Shared => {
                let shared_store = Arc::new(new_indexed_shared_table_store(
                    app_ctx.storage_backend(),
                    &table_id,
                    &pk_field,
                ));

                let core = Arc::new(TableProviderCore::new(
                    table_def_arc,
                    services,
                    pk_field,
                    arrow_schema,
                    column_defaults,
                ));

                let provider = SharedTableProvider::new(
                    core,
                    shared_store,
                );
                Ok(Arc::new(provider))
            },
            TableType::Stream => {
                let ttl_seconds = if let TableOptions::Stream(opts) = &table_def.table_options {
                    opts.ttl_seconds
                } else {
                    3600 // Default fallback
                };

                let streams_root = app_ctx.config().storage.streams_dir();
                let base_dir = streams_root
                    .join(table_id.namespace_id().as_str())
                    .join(table_id.table_name().as_str());

                let stream_store = Arc::new(new_stream_table_store(
                    &table_id,
                    StreamTableStoreConfig {
                        base_dir,
                        shard_router: ShardRouter::default_config(),
                        ttl_seconds: Some(ttl_seconds),
                    },
                ));

                let core = Arc::new(TableProviderCore::new(
                    table_def_arc,
                    services,
                    pk_field,
                    arrow_schema,
                    column_defaults,
                ));

                let provider = StreamTableProvider::new(
                    core,
                    stream_store,
                    Some(ttl_seconds),
                );
                Ok(Arc::new(provider))
            },
            TableType::System => Err(KalamDbError::InvalidOperation(format!(
                "Cannot create provider for system table {} via SchemaRegistry",
                table_id
            ))),
        }
    }

    /// Insert fully initialized cached table data into the cache
    pub fn insert_cached(&self, table_id: TableId, data: Arc<CachedTableData>) {
        self.table_cache.insert(table_id, data);
    }

    /// Invalidate (remove) cached table data
    pub fn invalidate(&self, table_id: &TableId) {
        self.table_cache.remove(table_id);
        let _ = self.deregister_from_datafusion(table_id);
    }

    /// Invalidate all versions of a table (for DROP TABLE)
    pub fn invalidate_all_versions(&self, table_id: &TableId) {
        // Remove from latest cache
        self.table_cache.remove(table_id);

        // Remove all versioned entries for this table
        let keys_to_remove: Vec<TableVersionId> = self
            .version_cache
            .iter()
            .filter(|entry| entry.key().table_id() == table_id)
            .map(|entry| entry.key().clone())
            .collect();

        for key in keys_to_remove {
            self.version_cache.remove(&key);
        }

        let _ = self.deregister_from_datafusion(table_id);
    }

    // ===== Versioned Cache Methods (Phase 16) =====

    /// Get cached table data for a specific version
    ///
    /// Used when reading Parquet files written with older schemas.
    pub fn get_version(&self, version_id: &TableVersionId) -> Option<Arc<CachedTableData>> {
        self.version_cache.get(version_id).map(|entry| entry.value().clone())
    }

    /// Insert a specific version into the cache
    pub fn insert_version(&self, version_id: TableVersionId, data: Arc<CachedTableData>) {
        self.version_cache.insert(version_id, data);
    }

    /// Get cache statistics
    pub fn stats(&self) -> usize {
        self.table_cache.len()
    }

    /// Clear all cached data
    pub fn clear(&self) {
        self.table_cache.clear();
        self.version_cache.clear();
    }

    /// Get number of cached entries (latest versions only)
    pub fn len(&self) -> usize {
        self.table_cache.len()
    }

    /// Get total number of cached entries (latest + versioned)
    pub fn total_len(&self) -> usize {
        (self.table_cache.len() + self.version_cache.len()) as usize
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.table_cache.len() == 0 && self.version_cache.len() == 0
    }

    // ===== Provider Methods (consolidated from ProviderRegistry) =====

    /// Insert a DataFusion provider into the cache for a table
    ///
    /// Stores the provider in CachedTableData and registers with DataFusion's catalog.
    pub fn insert_provider(
        &self,
        table_id: TableId,
        provider: Arc<dyn TableProvider + Send + Sync>,
    ) -> Result<(), KalamDbError> {
        log::debug!("[SchemaRegistry] Inserting provider for table {}", table_id);

        // Store in CachedTableData
        if let Some(cached) = self.get(&table_id) {
            cached.set_provider(provider.clone());
        } else {
            // Table not in cache - try to load from persistence first
            if let Some(_table_def) = self.get_table_if_exists(&table_id)? {
                if let Some(cached) = self.get(&table_id) {
                    cached.set_provider(provider.clone());
                } else {
                    return Err(KalamDbError::TableNotFound(format!(
                        "Cannot insert provider: table {} not in cache",
                        table_id
                    )));
                }
            } else {
                return Err(KalamDbError::TableNotFound(format!(
                    "Cannot insert provider: table {} not found",
                    table_id
                )));
            }
        }

        // Register with DataFusion's catalog if available
        self.register_with_datafusion(&table_id, provider)?;

        Ok(())
    }

    /// Remove a cached DataFusion provider for a table and unregister from DataFusion
    pub fn remove_provider(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        // Clear from CachedTableData
        if let Some(cached) = self.get(table_id) {
            cached.clear_provider();
        }

        // Deregister from DataFusion
        self.deregister_from_datafusion(table_id)
    }

    /// Get a cached DataFusion provider for a table
    pub fn get_provider(&self, table_id: &TableId) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        let result = self.get(table_id).and_then(|cached| cached.get_provider());
        if result.is_some() {
            log::trace!("[SchemaRegistry] Retrieved provider for table {}", table_id);
        } else {
            log::warn!("[SchemaRegistry] Provider NOT FOUND for table {}", table_id);
        }
        result
    }

    /// Register a provider with DataFusion's catalog
    ///
    /// If the table already exists (e.g., during ALTER TABLE), it will be
    /// deregistered first, then re-registered with the new provider.
    fn register_with_datafusion(
        &self,
        table_id: &TableId,
        provider: Arc<dyn TableProvider + Send + Sync>,
    ) -> Result<(), KalamDbError> {
        if let Some(base_session) = self.base_session_context.get() {
            // Use constant catalog name "kalam"
            let catalog = base_session.catalog("kalam").ok_or_else(|| {
                KalamDbError::InvalidOperation("Catalog 'kalam' not found".to_string())
            })?;

            // Get or create namespace schema
            let schema = catalog.schema(table_id.namespace_id().as_str()).unwrap_or_else(|| {
                // Create namespace schema if it doesn't exist
                let new_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
                catalog
                    .register_schema(table_id.namespace_id().as_str(), new_schema.clone())
                    .expect("Failed to register namespace schema");
                new_schema
            });

            // For ALTER TABLE: always deregister first (if exists), then register with new provider
            // This ensures the new schema is always visible
            let table_name = table_id.table_name().as_str();

            // Check if table already exists - if so, deregister it first
            if schema.table_exist(table_name) {
                log::debug!(
                    "[SchemaRegistry] Table {} already registered in DataFusion; deregistering before re-registration",
                    table_id
                );
                match schema.deregister_table(table_name) {
                    Ok(Some(_old_provider)) => {
                        log::debug!(
                            "[SchemaRegistry] Successfully deregistered old provider for {}",
                            table_id
                        );
                    },
                    Ok(None) => {
                        // Table existed but deregister returned None - shouldn't happen but handle it
                        log::warn!(
                            "[SchemaRegistry] table_exist returned true but deregister_table returned None for {}",
                            table_id
                        );
                    },
                    Err(e) => {
                        return Err(KalamDbError::InvalidOperation(format!(
                            "Failed to deregister existing table {} from DataFusion: {}",
                            table_id, e
                        )));
                    },
                }
            }

            // Now register the new provider - table should not exist at this point
            log::debug!(
                "[SchemaRegistry] Registering table {} (schema cols: {})",
                table_id,
                provider.schema().fields().len()
            );

            schema.register_table(table_name.to_string(), provider).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to register table {} with DataFusion: {}",
                    table_id, e
                ))
            })?;

            log::debug!("[SchemaRegistry] Registered table {} with DataFusion catalog", table_id);
        }

        Ok(())
    }

    /// Deregister a table from DataFusion's catalog
    fn deregister_from_datafusion(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        if let Some(base_session) = self.base_session_context.get() {
            let catalog_name =
                base_session.state().config().options().catalog.default_catalog.clone();

            let catalog = base_session.catalog(&catalog_name).ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Catalog '{}' not found", catalog_name))
            })?;

            // Get namespace schema
            if let Some(schema) = catalog.schema(table_id.namespace_id().as_str()) {
                // Deregister table from DataFusion
                schema.deregister_table(table_id.table_name().as_str()).map_err(|e| {
                    KalamDbError::InvalidOperation(format!(
                        "Failed to deregister table from DataFusion: {}",
                        e
                    ))
                })?;

                log::debug!("Unregistered table {} from DataFusion catalog", table_id);
            }
        }

        Ok(())
    }

    /// Store table definition in cache (persistence handled by caller)
    pub fn put_table_definition(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), KalamDbError> {
        let app_ctx = self.app_context();
        let table_arc = Arc::new(table_def.clone());
        let data = CachedTableData::from_table_definition(app_ctx, table_id, table_arc)?;
        self.insert_cached(table_id.clone(), Arc::new(data));

        Ok(())
    }

    /// Delete table definition from persistence layer (delete-through pattern)
    pub fn delete_table_definition(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        let app_ctx = self.app_context();
        let tables_provider = app_ctx.system_tables().tables();

        // Delete from storage
        tables_provider.delete_table(table_id)?;

        // Invalidate cache
        self.invalidate_all_versions(table_id);

        Ok(())
    }

    /// Scan all table definitions from persistence layer
    pub fn scan_all_table_definitions(&self) -> Result<Vec<TableDefinition>, KalamDbError> {
        let app_ctx = self.app_context();
        let tables_provider = app_ctx.system_tables().tables();

        // Scan all tables from storage
        let all_entries = tables_provider.scan_all().into_kalamdb_error("Failed to scan tables")?;
        Ok(all_entries)
    }

    /// Get table definition if it exists (optimized single-call pattern)
    ///
    /// Combines table existence check + definition fetch in one operation.
    ///
    /// # Performance
    /// - Cache hit: Returns immediately (no duplicate lookups)
    /// - Cache miss: Single persistence query + cache population
    /// - Prevents double fetch: table_exists() then get_table_if_exists()
    ///
    /// # Example
    /// ```no_run
    /// // ❌ OLD: Two lookups (inefficient)
    /// if schema_registry.table_exists(&table_id)? {
    ///     let def = schema_registry.get_table_if_exists(&table_id)?;
    /// }
    ///
    /// // ✅ NEW: Single lookup (efficient)
    /// if let Some(def) = schema_registry.get_table_if_exists(&table_id)? {
    ///     // use def
    /// }
    /// ```
    pub fn get_table_if_exists(
        &self,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, KalamDbError> {
        let app_ctx = self.app_context();
        // Fast path: check cache
        if let Some(cached) = self.get(table_id) {
            return Ok(Some(Arc::clone(&cached.table)));
        }

        // Check if it's a system table
        if table_id.namespace_id().is_system_namespace() {
            if let Some(def) = app_ctx.system_tables().get_system_definition(table_id) {
                return Ok(Some(def));
            }
        }

        let tables_provider = app_ctx.system_tables().tables();

        match tables_provider.get_table_by_id(table_id)? {
            Some(table_def) => {
                let table_arc = Arc::new(table_def);
                let data = CachedTableData::from_table_definition(
                    app_ctx.as_ref(),
                    table_id,
                    table_arc.clone(),
                )?;
                self.insert_cached(table_id.clone(), Arc::new(data));
                Ok(Some(table_arc))
            },
            None => Ok(None),
        }
    }

    /// Async version of get_table_if_exists (avoids blocking RocksDB reads in async context)
    ///
    /// Under high load, synchronous RocksDB operations can starve the tokio runtime.
    /// This async version uses spawn_blocking to prevent runtime starvation.
    pub async fn get_table_if_exists_async(
        &self,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, KalamDbError> {
        let app_ctx = self.app_context();
        // Fast path: check cache
        if let Some(cached) = self.get(table_id) {
            return Ok(Some(Arc::clone(&cached.table)));
        }

        // Check if it's a system table
        if table_id.namespace_id().is_system_namespace() {
            if let Some(def) = app_ctx.system_tables().get_system_definition(table_id) {
                return Ok(Some(def));
            }
        }

        let tables_provider = app_ctx.system_tables().tables();

        // Use async version to avoid blocking the runtime
        match tables_provider.get_table_by_id_async(table_id).await? {
            Some(table_def) => {
                let table_arc = Arc::new(table_def);
                let data = CachedTableData::from_table_definition(
                    app_ctx.as_ref(),
                    table_id,
                    table_arc.clone(),
                )?;
                self.insert_cached(table_id.clone(), Arc::new(data));
                Ok(Some(table_arc))
            },
            None => Ok(None),
        }
    }

    /// Get Arrow schema for a table
    ///
    /// Directly accesses the memoized Arrow schema from CachedTableData.
    /// The schema is computed once on first access and cached thereafter.
    ///
    /// **Performance**: First call ~75μs (computation), subsequent calls ~1.5μs (cached)
    pub fn get_arrow_schema(
        &self,
        table_id: &TableId,
    ) -> Result<Arc<arrow::datatypes::Schema>, KalamDbError> {
        // Fast path: check cache
        if let Some(cached) = self.get(table_id) {
            return cached.arrow_schema();
        }

        // Slow path: try to load from persistence (lazy loading)
        if self.get_table_if_exists(table_id)?.is_some() {
            // Cache is now populated - retrieve it
            if let Some(cached) = self.get(table_id) {
                return cached.arrow_schema();
            }
        }

        Err(KalamDbError::TableNotFound(format!("Table not found: {}", table_id)))
    }

    /// Get storage ID for a table
    pub fn get_storage_id(&self, table_id: &TableId) -> Result<StorageId, KalamDbError> {
        if let Some(cached) = self.get(table_id) {
            return Ok(cached.storage_id.clone());
        }

        if let Some(table_def) = self.get_table_if_exists(table_id)? {
            if let Some(storage_id) = CachedTableData::extract_storage_id(&table_def) {
                return Ok(storage_id);
            }
        }

        Err(KalamDbError::TableNotFound(format!("Table not found: {}", table_id)))
    }

    /// Get Arrow schema for a specific table version (for reading old Parquet files)
    ///
    /// Uses version-specific cache to avoid repeated schema conversions when reading
    /// multiple Parquet files written with the same historical schema version.
    pub fn get_arrow_schema_for_version(
        &self,
        table_id: &TableId,
        schema_version: u32,
    ) -> Result<Arc<arrow::datatypes::Schema>, KalamDbError> {
        let app_ctx = self.app_context();
        let version_id = TableVersionId::versioned(table_id.clone(), schema_version);

        // Fast path: check version cache
        if let Some(cached) = self.get_version(&version_id) {
            return cached.arrow_schema();
        }

        // Slow path: load from versioned tables store and cache
        let table_def = app_ctx
            .system_tables()
            .tables()
            .get_version(table_id, schema_version)
            .map_err(|e| {
                KalamDbError::Other(format!(
                    "Failed to retrieve schema version {}: {}",
                    schema_version, e
                ))
            })?
            .ok_or_else(|| {
                KalamDbError::Other(format!(
                    "Schema version {} not found for table {}",
                    schema_version, table_id
                ))
            })?;

        // Create cached data and compute arrow schema
        let cached_data = CachedTableData::new(Arc::new(table_def));
        let arrow_schema = cached_data.arrow_schema()?;

        // Cache for future lookups
        self.insert_version(version_id, Arc::new(cached_data));

        Ok(arrow_schema)
    }
}

impl SchemaRegistryTrait for SchemaRegistry {
    type Error = KalamDbError;

    fn get_arrow_schema(&self, table_id: &TableId) -> Result<SchemaRef, Self::Error> {
        self.get_arrow_schema(table_id)
    }

    fn get_table_if_exists(
        &self,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, Self::Error> {
        self.get_table_if_exists(table_id)
    }

    fn get_arrow_schema_for_version(
        &self,
        table_id: &TableId,
        schema_version: u32,
    ) -> Result<SchemaRef, Self::Error> {
        self.get_arrow_schema_for_version(table_id, schema_version)
    }

    fn get_storage_id(&self, table_id: &TableId) -> Result<StorageId, Self::Error> {
        self.get_storage_id(table_id)
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new(10000) // Default max size: 10,000 tables
    }
}
