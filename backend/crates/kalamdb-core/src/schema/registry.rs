//! SchemaRegistry: Unified schema management (cache + persistence)
// Implements T200 (US8) for Phase 5: AppContext + SchemaRegistry + Stateless Executor
// Phase 5 Enhancement: Consolidates TableSchemaStore for single source of truth

use std::sync::Arc;
use dashmap::DashMap;
use arrow::datatypes::SchemaRef;

use crate::schema::SchemaCache;
use crate::tables::base_table_provider::UserTableShared;
use crate::tables::system::schemas::TableSchemaStore;
use crate::schema::CachedTableData;
use crate::error::KalamDbError;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::models::{TableId, NamespaceId};
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

    /// Scan all tables in a namespace (delegates to store)
    /// T370: Added for Phase 10.1 - StorageAdapter → SchemaRegistry Migration
    pub fn scan_namespace(&self, namespace_id: &NamespaceId) -> Result<Vec<(TableId, Arc<TableDefinition>)>, KalamDbError> {
        Ok(self.store.scan_namespace(namespace_id)?
            .into_iter()
            .map(|(id, def)| (id, Arc::new(def)))
            .collect())
    }
    
    /// Fast existence check (cache-first, fallback to store)
    /// T371: Added for Phase 10.1 - StorageAdapter → SchemaRegistry Migration
    pub fn table_exists(&self, table_id: &TableId) -> Result<bool, KalamDbError> {
        // Fast path: check cache first
        if self.cache.get(table_id).is_some() {
            return Ok(true);
        }
        
        // Cache miss: check store
        match self.store.get(table_id)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
    
    /// Get table metadata without full definition (lightweight)
    /// T372: Added for Phase 10.1 - StorageAdapter → SchemaRegistry Migration
    pub fn get_table_metadata(&self, table_id: &TableId) -> Result<Option<TableMetadata>, KalamDbError> {
        // Try cache first (most common case)
        if let Some(cached) = self.cache.get(table_id) {
            return Ok(Some(TableMetadata {
                table_id: table_id.clone(),
                table_type: cached.table_type,
                created_at: cached.created_at,
                storage_id: cached.storage_id.clone(),
            }));
        }
        
        // Cache miss: get from store and extract metadata
        // Note: storage_id is not available when reading from store directly (only in cache)
        match self.store.get(table_id)? {
            Some(def) => Ok(Some(TableMetadata {
                table_id: table_id.clone(),
                table_type: def.table_type,
                created_at: def.created_at,
                storage_id: None, // Not available from store, use get_table_definition() if needed
            })),
            None => Ok(None),
        }
    }
}

/// Lightweight table metadata (without full column definitions)
/// T372: Added for Phase 10.1
#[derive(Debug, Clone)]
pub struct TableMetadata {
    pub table_id: TableId,
    pub table_type: kalamdb_commons::schemas::TableType,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub storage_id: Option<kalamdb_commons::models::StorageId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::init_test_app_context;
    use crate::app_context::AppContext;
    use kalamdb_commons::{datatypes::KalamDataType, schemas::{ColumnDefinition, TableType}};

    fn create_test_registry() -> Arc<SchemaRegistry> {
        init_test_app_context();
        let ctx = AppContext::get();
        ctx.schema_registry().clone()
    }

    fn create_test_table_definition(namespace_id: &NamespaceId, table_name: &kalamdb_commons::models::TableName) -> TableDefinition {
        use kalamdb_commons::schemas::{TableOptions, ColumnDefault};
        
        TableDefinition::new(
            namespace_id.clone(),
            table_name.clone(),
            TableType::User,
            vec![
                ColumnDefinition::new(
                    "id".to_string(),
                    0,
                    KalamDataType::BigInt,
                    false,
                    true,
                    false,
                    ColumnDefault::None,
                    None,
                ),
                ColumnDefinition::new(
                    "name".to_string(),
                    1,
                    KalamDataType::Text,
                    true,
                    false,
                    false,
                    ColumnDefault::None,
                    None,
                ),
            ],
            TableOptions::user(),
            Some("Test table".to_string()),
        ).unwrap()
    }

    /// T374: Test scan_namespace() returns all tables in a namespace
    #[tokio::test]
    async fn test_scan_namespace() {
        use kalamdb_commons::models::TableName;
        
        let registry = create_test_registry();
        let namespace_id = NamespaceId::new("mydb");
        
        // Insert 3 tables
        let table1 = TableId::new(namespace_id.clone(), TableName::new("table1"));
        let table2 = TableId::new(namespace_id.clone(), TableName::new("table2"));
        let table3 = TableId::new(namespace_id.clone(), TableName::new("table3"));
        
        let def1 = create_test_table_definition(&namespace_id, &TableName::new("table1"));
        let def2 = create_test_table_definition(&namespace_id, &TableName::new("table2"));
        let def3 = create_test_table_definition(&namespace_id, &TableName::new("table3"));
        
        registry.put_table_definition(&table1, &def1).unwrap();
        registry.put_table_definition(&table2, &def2).unwrap();
        registry.put_table_definition(&table3, &def3).unwrap();
        
        // Scan namespace
        let tables = registry.scan_namespace(&namespace_id).unwrap();
        assert_eq!(tables.len(), 3, "Should find 3 tables in namespace");
        
        // Verify table IDs are present
        let table_ids: Vec<_> = tables.iter().map(|(id, _)| id.clone()).collect();
        assert!(table_ids.contains(&table1));
        assert!(table_ids.contains(&table2));
        assert!(table_ids.contains(&table3));
    }

    /// T375: Test table_exists() cache hit path
    #[tokio::test]
    async fn test_table_exists_cache_hit() {
        use kalamdb_commons::models::TableName;
        
        let registry = create_test_registry();
        let namespace_id = NamespaceId::new("mydb");
        let table_name = TableName::new("users");
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        
        // Insert table (persists + should be cached on next get)
        let definition = create_test_table_definition(&namespace_id, &table_name);
        registry.put_table_definition(&table_id, &definition).unwrap();
        
        // Prime the cache by reading
        let _ = registry.get_table_definition(&table_id);
        
        // Check existence (should hit cache)
        assert!(registry.table_exists(&table_id).unwrap(), "Table should exist");
    }

    /// T376: Test table_exists() with cache miss (fallback to store)
    #[tokio::test]
    async fn test_table_exists_cache_miss() {
        use kalamdb_commons::models::TableName;
        
        let registry = create_test_registry();
        let namespace_id = NamespaceId::new("mydb");
        let table_name = TableName::new("users");
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        
        // Insert table
        let definition = create_test_table_definition(&namespace_id, &table_name);
        registry.put_table_definition(&table_id, &definition).unwrap();
        
        // Check existence without priming cache (should fallback to store)
        assert!(registry.table_exists(&table_id).unwrap(), "Table should exist in store");
        
        // Check non-existent table
        let nonexistent = TableId::new(namespace_id.clone(), TableName::new("nonexistent"));
        assert!(!registry.table_exists(&nonexistent).unwrap(), "Non-existent table should return false");
    }

    /// T377: Test get_table_metadata() for lightweight lookups
    #[tokio::test]
    async fn test_get_table_metadata_lightweight() {
        use kalamdb_commons::models::TableName;
        
        let registry = create_test_registry();
        let namespace_id = NamespaceId::new("mydb");
        let table_name = TableName::new("users");
        let table_id = TableId::new(namespace_id.clone(), table_name.clone());
        
        // Insert table with full definition
        let definition = create_test_table_definition(&namespace_id, &table_name);
        let created_at = definition.created_at;
        registry.put_table_definition(&table_id, &definition).unwrap();
        
        // Get metadata only (no full column definitions)
        let metadata = registry.get_table_metadata(&table_id).unwrap().expect("Metadata should exist");
        
        assert_eq!(metadata.table_id, table_id);
        assert_eq!(metadata.table_type, TableType::User);
        // storage_id is available from cache (SchemaCache has it), so this should return Some
        assert!(metadata.storage_id.is_some(), "storage_id should be present from cache");
        // timestamps should be close (within 1 second)
        assert!((metadata.created_at - created_at).num_seconds().abs() < 1);
    }
}
