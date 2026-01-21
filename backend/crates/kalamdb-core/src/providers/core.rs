use crate::app_context::AppContext;
use crate::schema_registry::TableType;
use kalamdb_commons::TableId;
use kalamdb_filestore::StorageRegistry;
use std::sync::Arc;

/// Shared core state for all table providers
///
/// **Memory Optimization**: All provider types share this core structure,
/// reducing per-table memory footprint from 3× allocation to 1× allocation.
///
/// **Phase 12 Refactoring**: Uses kalamdb-registry services directly
///
/// **Services**:
/// - `app_context`: Application context for global services (required for trait methods)
/// - `schema_registry`: Table schema management and caching (from kalamdb-registry)
/// - `system_columns`: SeqId generation, _deleted flag handling (from kalamdb-registry)
/// - `live_query_manager`: WebSocket notifications (optional, from kalamdb-core)
/// - `storage_registry`: Storage path resolution (optional, from kalamdb-core)
pub struct TableProviderCore {
    /// Application context for global services (kept for BaseTableProvider trait)
    pub app_context: Arc<AppContext>,

    /// Table identity shared by provider implementations
    table_id: Arc<TableId>,

    /// Logical table type used for routing/storage decisions
    table_type: TableType,

    /// Schema registry for table metadata and Arrow schema caching
    pub schema_registry: Arc<crate::schema_registry::SchemaRegistry>,

    /// System columns service for _seq and _deleted management
    pub system_columns: Arc<crate::system_columns::SystemColumnsService>,

    /// Storage registry for resolving full storage paths (optional)
    pub storage_registry: Option<Arc<StorageRegistry>>,
}

impl TableProviderCore {
    /// Create new core with required services from AppContext
    pub fn new(app_context: Arc<AppContext>, table_id: TableId, table_type: TableType) -> Self {
        Self {
            table_id: Arc::new(table_id),
            table_type,
            schema_registry: app_context.schema_registry(),
            system_columns: app_context.system_columns_service(),
            storage_registry: None,
            app_context,
        }
    }

    /// Backwards-compatible helper to build from borrowed AppContext
    pub fn from_app_context(
        app_context: &Arc<AppContext>,
        table_id: TableId,
        table_type: TableType,
    ) -> Self {
        Self::new(Arc::clone(app_context), table_id, table_type)
    }

    /// Add StorageRegistry to core
    pub fn with_storage_registry(mut self, registry: Arc<StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    /// TableId accessor (shared across providers)
    pub fn table_id(&self) -> &TableId {
        self.table_id.as_ref()
    }

    /// Best-effort primary key column_id for this table.
    /// Returns 0 if the schema is missing or no primary key is defined.
    pub fn primary_key_column_id(&self) -> u64 {
        self.schema_registry
            .get_table_if_exists(self.table_id())
            .ok()
            .flatten()
            .and_then(|def| def.columns.iter().find(|c| c.is_primary_key).map(|c| c.column_id))
            .unwrap_or(0)
    }

    /// Cloneable TableId handle (avoids leaking Arc internals to callers)
    pub fn table_id_arc(&self) -> Arc<TableId> {
        self.table_id.clone()
    }

    /// TableType accessor
    pub fn table_type(&self) -> TableType {
        self.table_type
    }
}
