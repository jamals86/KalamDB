//! Core implementation of SchemaRegistry

use crate::error::KalamDbError;
use crate::schema_registry::cached_table_data::CachedTableData;
use crate::schema_registry::path_resolver::PathResolver;
use crate::schema_registry::persistence::SchemaPersistence;
use crate::schema_registry::table_cache::TableCache;
use datafusion::datasource::TableProvider;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{TableId, TableVersionId, UserId};
use std::sync::{Arc, OnceLock};

/// Unified schema cache for table metadata, schemas, and providers
///
/// Single DashMap architecture for all table data including:
/// - Table definitions and metadata
/// - Memoized Arrow schemas
/// - DataFusion TableProvider instances
///
/// **Performance Optimization**: Cache access updates `last_accessed_ms` via an
/// atomic field stored inside `CachedTableData`. This avoids a separate
/// timestamps map while keeping per-access work O(1) and avoiding any deep clones.
pub struct SchemaRegistry {
    /// Cache for table data (includes provider storage)
    table_cache: TableCache,

    /// DataFusion base session context for table registration (set once during init)
    base_session_context: OnceLock<Arc<datafusion::prelude::SessionContext>>,
}

impl std::fmt::Debug for SchemaRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaRegistry")
            .field("table_cache", &self.table_cache)
            .field("base_session_context_set", &self.base_session_context.get().is_some())
            .finish()
    }
}

impl SchemaRegistry {
    /// Create a new schema cache with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            table_cache: TableCache::new(max_size),
            base_session_context: OnceLock::new(),
        }
    }

    /// Set the DataFusion base session context for table registration
    pub fn set_base_session_context(&self, session: Arc<datafusion::prelude::SessionContext>) {
        let _ = self.base_session_context.set(session);
    }

    /// Get cached table data by TableId
    pub fn get(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        self.table_cache.get(table_id)
    }

    /// Insert or update cached table data
    pub fn insert(&self, table_id: TableId, data: Arc<CachedTableData>) {
        if let Some(evicted) = self.table_cache.insert(table_id, data) {
            // Deregister evicted table from DataFusion catalog
            let _ = self.deregister_from_datafusion(&evicted);
        }
    }

    /// Invalidate (remove) cached table data
    pub fn invalidate(&self, table_id: &TableId) {
        self.table_cache.invalidate(table_id);
        let _ = self.deregister_from_datafusion(table_id);
    }

    /// Invalidate all versions of a table (for DROP TABLE)
    pub fn invalidate_all_versions(&self, table_id: &TableId) {
        self.table_cache.invalidate_all_versions(table_id);
        let _ = self.deregister_from_datafusion(table_id);
    }

    // ===== Versioned Cache Methods (Phase 16) =====

    /// Get cached table data for a specific version
    ///
    /// Used when reading Parquet files written with older schemas.
    pub fn get_version(&self, version_id: &TableVersionId) -> Option<Arc<CachedTableData>> {
        self.table_cache.get_version(version_id)
    }

    /// Insert a specific version into the cache
    pub fn insert_version(&self, version_id: TableVersionId, data: Arc<CachedTableData>) {
        self.table_cache.insert_version(version_id, data);
    }

    /// Resolve storage path with dynamic placeholders substituted
    pub fn get_storage_path(
        &self,
        table_id: &TableId,
        user_id: Option<&UserId>,
        shard: Option<u32>,
    ) -> Result<String, KalamDbError> {
        let data = self
            .get(table_id)
            .ok_or_else(|| KalamDbError::TableNotFound(format!("Table not found: {}", table_id)))?;

        PathResolver::get_storage_path(&data, user_id, shard)
    }

    /// Get cache hit rate (for metrics)
    pub fn hit_rate(&self) -> f64 {
        self.table_cache.hit_rate()
    }

    /// Get cache statistics
    pub fn stats(&self) -> (usize, u64, u64, f64) {
        self.table_cache.stats()
    }

    /// Clear all cached data
    pub fn clear(&self) {
        self.table_cache.clear();
    }

    /// Get number of cached entries
    pub fn len(&self) -> usize {
        self.table_cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.table_cache.is_empty()
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
        log::info!(
            "[SchemaRegistry] Inserting provider for table {}",
            table_id
        );

        // Store in CachedTableData
        if let Some(cached) = self.get(&table_id) {
            cached.set_provider(provider.clone());
        } else {
            // Table not in cache - try to load from persistence first
            if let Some(cached) = SchemaPersistence::get_table_definition(&self.table_cache, &table_id)?
                .and_then(|_| self.get(&table_id))
            {
                cached.set_provider(provider.clone());
            } else {
                return Err(KalamDbError::TableNotFound(format!(
                    "Cannot insert provider: table {} not in cache",
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
            log::trace!(
                "[SchemaRegistry] Retrieved provider for table {}",
                table_id
            );
        } else {
            log::warn!(
                "[SchemaRegistry] Provider NOT FOUND for table {}",
                table_id
            );
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
            let schema = catalog
                .schema(table_id.namespace_id().as_str())
                .unwrap_or_else(|| {
                    // Create namespace schema if it doesn't exist
                    let new_schema =
                        Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
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
                log::info!(
                    "[SchemaRegistry] Table {} already registered in DataFusion; deregistering before re-registration",
                    table_id
                );
                match schema.deregister_table(table_name) {
                    Ok(Some(_old_provider)) => {
                        log::debug!(
                            "[SchemaRegistry] Successfully deregistered old provider for {}",
                            table_id
                        );
                    }
                    Ok(None) => {
                        // Table existed but deregister returned None - shouldn't happen but handle it
                        log::warn!(
                            "[SchemaRegistry] table_exist returned true but deregister_table returned None for {}",
                            table_id
                        );
                    }
                    Err(e) => {
                        return Err(KalamDbError::InvalidOperation(format!(
                            "Failed to deregister existing table {} from DataFusion: {}",
                            table_id, e
                        )));
                    }
                }
            }
            
            // Now register the new provider - table should not exist at this point
            log::debug!("[SchemaRegistry] Registering table {} (schema cols: {})", 
                table_id, 
                provider.schema().fields().len()
            );
            
            schema.register_table(table_name.to_string(), provider)
                .map_err(|e| {
                    KalamDbError::InvalidOperation(format!(
                        "Failed to register table {} with DataFusion: {}",
                        table_id, e
                    ))
                })?;

            log::info!(
                "[SchemaRegistry] Registered table {} with DataFusion catalog",
                table_id
            );
        }

        Ok(())
    }

    /// Deregister a table from DataFusion's catalog
    fn deregister_from_datafusion(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        if let Some(base_session) = self.base_session_context.get() {
            let catalog_name = base_session
                .state()
                .config()
                .options()
                .catalog
                .default_catalog
                .clone();

            let catalog = base_session.catalog(&catalog_name).ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Catalog '{}' not found", catalog_name))
            })?;

            // Get namespace schema
            if let Some(schema) = catalog.schema(table_id.namespace_id().as_str()) {
                // Deregister table from DataFusion
                schema
                    .deregister_table(table_id.table_name().as_str())
                    .map_err(|e| {
                        KalamDbError::InvalidOperation(format!(
                            "Failed to deregister table from DataFusion: {}",
                            e
                        ))
                    })?;

                log::debug!(
                    "Unregistered table {} from DataFusion catalog",
                    table_id
                );
            }
        }

        Ok(())
    }

    // ===== Persistence Methods (Phase 5: SchemaRegistry Consolidation) =====

    /// Get table definition from persistence layer (read-through pattern)
    pub fn get_table_definition(
        &self,
        table_id: &TableId,
    ) -> Result<Option<Arc<TableDefinition>>, KalamDbError> {
        SchemaPersistence::get_table_definition(&self.table_cache, table_id)
    }

    /// Store table definition to persistence layer (write-through pattern)
    pub fn put_table_definition(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
    ) -> Result<(), KalamDbError> {
        SchemaPersistence::put_table_definition(&self.table_cache, table_id, table_def)
    }

    /// Delete table definition from persistence layer (delete-through pattern)
    pub fn delete_table_definition(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        SchemaPersistence::delete_table_definition(&self.table_cache, table_id)
    }

    /// Scan all table definitions from persistence layer
    pub fn scan_all_table_definitions(&self) -> Result<Vec<TableDefinition>, KalamDbError> {
        SchemaPersistence::scan_all_table_definitions()
    }

    /// Check if table exists in persistence layer
    pub fn table_exists(&self, table_id: &TableId) -> Result<bool, KalamDbError> {
        SchemaPersistence::table_exists(&self.table_cache, table_id)
    }

    /// Get table definition if it exists (optimized single-call pattern)
    ///
    /// Combines table existence check + definition fetch in one operation.
    /// Use this instead of calling `table_exists()` followed by `get_table_definition()`.
    ///
    /// # Performance
    /// - Cache hit: Returns immediately (no duplicate lookups)
    /// - Cache miss: Single persistence query + cache population
    /// - Prevents double fetch: table_exists() then get_table_definition()
    ///
    /// # Example
    /// ```no_run
    /// // ❌ OLD: Two lookups (inefficient)
    /// if schema_registry.table_exists(&table_id)? {
    ///     let def = schema_registry.get_table_definition(&table_id)?;
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
        SchemaPersistence::get_table_if_exists(&self.table_cache, table_id)
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
        if SchemaPersistence::get_table_definition(&self.table_cache, table_id)?.is_some() {
            // Cache is now populated - retrieve it
            if let Some(cached) = self.get(table_id) {
                return cached.arrow_schema();
            }
        }

        Err(KalamDbError::TableNotFound(format!(
            "Table not found: {}",
            table_id
        )))
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
        let version_id = TableVersionId::versioned(table_id.clone(), schema_version);

        // Fast path: check version cache
        if let Some(cached) = self.get_version(&version_id) {
            return cached.arrow_schema();
        }

        // Slow path: load from versioned tables store and cache
        let table_def = crate::app_context::AppContext::get()
            .system_tables()
            .tables()
            .get_version(table_id, schema_version)
            .map_err(|e| KalamDbError::Other(format!("Failed to retrieve schema version {}: {}", schema_version, e)))?
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

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new(10000) // Default max size: 10,000 tables
    }
}
