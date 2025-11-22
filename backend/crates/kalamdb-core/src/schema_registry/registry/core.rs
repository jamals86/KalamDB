//! Core implementation of SchemaRegistry

use crate::error::KalamDbError;
use crate::schema_registry::cached_table_data::CachedTableData;
use crate::schema_registry::path_resolver::PathResolver;
use crate::schema_registry::persistence::SchemaPersistence;
use crate::schema_registry::provider_registry::ProviderRegistry;
use crate::schema_registry::table_cache::TableCache;
use datafusion::datasource::TableProvider;
use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{StorageId, TableId, UserId};
use kalamdb_commons::schemas::TableType;
use std::sync::Arc;

/// Unified schema cache for table metadata and schemas
///
/// Replaces dual-cache architecture with single DashMap for all table data.
///
/// **Performance Optimization**: LRU timestamps are stored separately to avoid
/// cloning large CachedTableData structs on every access.
#[derive(Debug)]
pub struct SchemaRegistry {
    /// Cache for table data
    table_cache: TableCache,

    /// Registry for DataFusion providers
    provider_registry: ProviderRegistry,
}

impl SchemaRegistry {
    /// Create a new schema cache with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            table_cache: TableCache::new(max_size),
            provider_registry: ProviderRegistry::new(),
        }
    }

    /// Set the DataFusion base session context for table registration
    pub fn set_base_session_context(&self, session: Arc<datafusion::prelude::SessionContext>) {
        self.provider_registry.set_base_session_context(session);
    }

    /// Get cached table data by TableId
    pub fn get(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        self.table_cache.get(table_id)
    }

    /// Insert or update cached table data
    pub fn insert(&self, table_id: TableId, data: Arc<CachedTableData>) {
        if let Some(evicted) = self.table_cache.insert(table_id, data) {
            // Also remove the provider for the evicted table
            let _ = self.provider_registry.remove_provider(&evicted);
        }
    }

    /// Invalidate (remove) cached table data
    pub fn invalidate(&self, table_id: &TableId) {
        self.table_cache.invalidate(table_id);
        let _ = self.provider_registry.remove_provider(table_id);
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
        self.provider_registry.clear();
    }

    /// Get number of cached entries
    pub fn len(&self) -> usize {
        self.table_cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.table_cache.is_empty()
    }

    /// Insert a DataFusion provider into the cache for a table
    pub fn insert_provider(
        &self,
        table_id: TableId,
        provider: Arc<dyn TableProvider + Send + Sync>,
    ) -> Result<(), KalamDbError> {
        self.provider_registry.insert_provider(table_id, provider)
    }

    /// Remove a cached DataFusion provider for a table and unregister from DataFusion
    pub fn remove_provider(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        self.provider_registry.remove_provider(table_id)
    }

    /// Get a cached DataFusion provider for a table
    pub fn get_provider(&self, table_id: &TableId) -> Option<Arc<dyn TableProvider + Send + Sync>> {
        self.provider_registry.get_provider(table_id)
    }

    /// Resolve partial storage path template for a table
    pub fn resolve_storage_path_template(
        &self,
        table_id: &TableId,
        table_type: TableType,
        storage_id: &StorageId,
    ) -> Result<String, KalamDbError> {
        PathResolver::resolve_storage_path_template(table_id, table_type, storage_id)
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

    /// Get Arrow schema for a table (Phase 10: Arrow Schema Memoization)
    pub fn get_arrow_schema(
        &self,
        table_id: &TableId,
    ) -> Result<Arc<arrow::datatypes::Schema>, KalamDbError> {
        let (schema, evicted) = SchemaPersistence::get_arrow_schema(&self.table_cache, table_id)?;

        if let Some(evicted_id) = evicted {
            let _ = self.provider_registry.remove_provider(&evicted_id);
        }

        Ok(schema)
    }

    /// Get Bloom filter column names for a table (PRIMARY KEY columns + _seq)
    pub fn get_bloom_filter_columns(
        &self,
        table_id: &TableId,
    ) -> Result<Vec<String>, KalamDbError> {
        let table_def = self
            .get_table_definition(table_id)?
            .ok_or_else(|| KalamDbError::TableNotFound(format!("Table not found: {}", table_id)))?;

        let mut columns = vec![];

        // Add PRIMARY KEY columns (FR-055: indexed columns)
        for col in table_def.columns.iter().filter(|c| c.is_primary_key) {
            columns.push(col.column_name.clone());
        }

        // Add _seq system column (FR-054: default Bloom filter columns)
        columns.push("_seq".to_string());

        Ok(columns)
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new(10000) // Default max size: 10,000 tables
    }
}
