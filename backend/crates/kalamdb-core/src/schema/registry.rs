//! SchemaRegistry: Facade over SchemaCache for efficient table/schema access
// Implements T200 (US8) for Phase 5: AppContext + SchemaRegistry + Stateless Executor

use std::sync::Arc;
use dashmap::DashMap;
use arrow::datatypes::SchemaRef;

use crate::catalog::SchemaCache;
use crate::tables::base_table_provider::UserTableShared;
use crate::catalog::CachedTableData;
use kalamdb_commons::schemas::TableDefinition;
use kalamdb_commons::models::TableId;

/// SchemaRegistry provides a read-through API over SchemaCache for table metadata, definitions, and Arrow schemas.
pub struct SchemaRegistry {
    cache: Arc<SchemaCache>,
    // Memoized Arrow schemas: TableId -> Arc<SchemaRef>
    arrow_schemas: DashMap<TableId, Arc<SchemaRef>>,
}

impl SchemaRegistry {
    pub fn new(cache: Arc<SchemaCache>) -> Self {
        Self {
            cache,
            arrow_schemas: DashMap::new(),
        }
    }

    /// Get full CachedTableData for a table
    pub fn get_table_data(&self, table_id: &TableId) -> Option<Arc<CachedTableData>> {
        self.cache.get(table_id)
    }

    /// Get TableDefinition for a table
    pub fn get_table_definition(&self, table_id: &TableId) -> Option<Arc<TableDefinition>> {
        self.cache.get(table_id).map(|data| data.schema.clone())
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
