//! SchemaRegistry: Unified schema management (cache + persistence)
// Implements T200 (US8) for Phase 5: AppContext + SchemaRegistry + Stateless Executor
// Phase 5 Enhancement: Consolidates TableSchemaStore for single source of truth

use std::sync::Arc;
use dashmap::DashMap;
use arrow::datatypes::SchemaRef;

use crate::catalog::SchemaCache;
use crate::tables::base_table_provider::UserTableShared;
use crate::tables::system::schemas::TableSchemaStore;
use crate::catalog::CachedTableData;
use crate::error::KalamDbError;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::models::TableId;
use kalamdb_store::entity_store::EntityStore;

/// SchemaRegistry provides unified schema management:
/// - In-memory cache (hot path) via SchemaCache
/// - Persistent storage (cold path) via TableSchemaStore
/// - Memoized Arrow schemas for zero-allocation repeated access
/// 
/// **Consolidation**: Eliminates duplicate logic between SchemaRegistry (cache) 
/// and TableSchemaStore (persistence) by making SchemaRegistry the single API 
/// for all schema operations.
pub struct SchemaRegistry {
    cache: Arc<SchemaCache>,
    store: Arc<TableSchemaStore>,
    // Memoized Arrow schemas: TableId -> Arc<SchemaRef>
    arrow_schemas: DashMap<TableId, Arc<SchemaRef>>,
}

impl SchemaRegistry {
    pub fn new(cache: Arc<SchemaCache>, store: Arc<TableSchemaStore>) -> Self {
        Self {
            cache,
            store,
            arrow_schemas: DashMap::new(),
        }
    }

    /// Get full CachedTableData for a table
    pub fn get_table_data(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        self.cache.get(table_id)
    }

    /// Get TableDefinition for a table (read-through: cache → store fallback)
    pub fn get_table_definition(&self, table_id: &TableId) -> Option<Arc<TableDefinition>> {
        // Fast path: check cache first
        if let Some(data) = self.cache.get(table_id) {
            return Some(data.schema.clone());
        }
        
        // Cache miss: read from persistent store
        match self.store.get(table_id) {
            Ok(Some(def)) => Some(Arc::new(def)),
            _ => None,
        }
    }
    
    /// Store a table definition (write-through: persist + invalidate cache)
    pub fn put_table_definition(&self, table_id: &TableId, definition: &TableDefinition) -> Result<(), KalamDbError> {
        // Persist to RocksDB
        self.store.put(table_id, definition)?;
        
        // Invalidate cache to force reload on next access
        self.invalidate(table_id);
        
        Ok(())
    }
    
    /// Delete a table definition (write-through: remove from store + cache)
    pub fn delete_table_definition(&self, table_id: &TableId) -> Result<(), KalamDbError> {
        // Delete from RocksDB
        self.store.delete(table_id)?;
        
        // Invalidate cache
        self.invalidate(table_id);
        
        Ok(())
    }

    /// Get Arrow SchemaRef for a table (memoized)
    pub fn get_arrow_schema(&self, table_id: &TableId) -> Option<Arc<SchemaRef>> {
        if let Some(schema) = self.arrow_schemas.get(table_id) {
            return Some(schema.value().clone());
        }
        let def = self.get_table_definition(table_id)?;
        let arrow = Arc::new(def.to_arrow_schema().ok()?);
        self.arrow_schemas.insert(table_id.clone(), arrow.clone());
        Some(arrow)
    }

    /// Get UserTableShared for a user table (if present)
    pub fn get_user_table_shared(&self, table_id: &TableId) -> Option<Arc<UserTableShared>> {
        self.cache.get_user_table_shared(table_id)
    }

    /// Invalidate all derived artifacts for a table (Arrow schema, UserTableShared, etc.)
    pub fn invalidate(&self, table_id: &TableId) {
        self.arrow_schemas.remove(table_id);
        self.cache.invalidate(table_id);
    }
}
