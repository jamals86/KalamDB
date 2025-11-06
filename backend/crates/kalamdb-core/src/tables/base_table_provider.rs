//! Base table provider interfaces and shared core
//!
//! Phase 10 - 3B: Common Provider Architecture & Memory Optimization
//! - Define a minimal trait to expose common metadata used by cache/introspection
//! - Provide a small core struct to avoid repeating fields across providers
//!
//! Phase 3C: UserTableProvider Handler Consolidation
//! - UserTableShared: Singleton shared state for all users accessing the same table
//! - Eliminates redundant handler/defaults allocations (30K Arc + 10K HashMap for 1000 users × 10 tables)

use crate::schema_registry::{SchemaRegistry, TableType};
use crate::live_query::manager::LiveQueryManager;
use crate::tables::user_tables::{UserTableDeleteHandler, UserTableInsertHandler, UserTableUpdateHandler};
use crate::tables::UserTableStore;
use datafusion::arrow::datatypes::SchemaRef;
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, TableName};
use kalamdb_commons::schemas::ColumnDefault;
use std::collections::HashMap;
use std::sync::Arc;

/// Common surface for table providers (outside DataFusion's TableProvider)
/// Used for cache/provider coordination and shared helpers.
pub trait BaseTableProvider: Send + Sync {
    /// Table identifier (namespace + table name)
    fn table_id(&self) -> &TableId;

    /// Arrow schema for the table
    fn schema_ref(&self) -> SchemaRef;

    /// Logical table type
    fn table_type(&self) -> TableType;
}

/// Shared core for providers to reduce field duplication
pub struct TableProviderCore {
    pub table_id: Arc<TableId>,
    pub table_type: TableType,
    pub schema: SchemaRef,
    pub created_at_ms: u64,
    pub storage_id: Option<StorageId>,
    pub unified_cache: Arc<SchemaRegistry>,
}

impl TableProviderCore {
    pub fn new(
        table_id: Arc<TableId>,
        table_type: TableType,
        schema: SchemaRef,
        storage_id: Option<StorageId>,
        unified_cache: Arc<SchemaRegistry>,
    ) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let created_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Self {
            table_id,
            table_type,
            schema,
            created_at_ms,
            storage_id,
            unified_cache,
        }
    }

    pub fn table_id(&self) -> &TableId {
        &self.table_id
    }

    pub fn schema_ref(&self) -> SchemaRef {
        self.schema.clone()
    }

    pub fn namespace(&self) -> &NamespaceId {
        self.table_id.namespace_id()
    }

    pub fn table_name(&self) -> &TableName {
        self.table_id.table_name()
    }

    pub fn table_type(&self) -> TableType {
        self.table_type
    }

    pub fn storage_id(&self) -> Option<&StorageId> {
        self.storage_id.as_ref()
    }

    pub fn cache(&self) -> &SchemaRegistry {
        &self.unified_cache
    }
}

/// Shared state for all UserTableProvider instances accessing the same table
///
/// **Problem**: Before Phase 3C, every UserTableProvider instance allocated:
/// - 3 Arc<Handler> instances (insert/update/delete)
/// - 1 HashMap<ColumnDefault> + schema scan
/// - For 1000 users × 10 tables = 30,000 Arc + 10,000 HashMap allocations
///
/// **Solution**: Create UserTableShared once per table, cache in SchemaCache, Arc::clone for each request
///
/// **Memory Savings**: 6 fields per instance → 1 Arc<UserTableShared> (83% reduction)
pub struct UserTableShared {
    /// Core provider fields (table_id, schema, cache)
    core: TableProviderCore,

    /// UserTableStore for DML operations (shared across all users)
    store: Arc<UserTableStore>,

    /// INSERT handler (created once, shared)
    insert_handler: Arc<UserTableInsertHandler>,

    /// UPDATE handler (created once, shared)
    update_handler: Arc<UserTableUpdateHandler>,

    /// DELETE handler (created once, shared)
    delete_handler: Arc<UserTableDeleteHandler>,

    /// Column default definitions derived from schema (shared, Arc-wrapped to avoid cloning HashMap)
    column_defaults: Arc<HashMap<String, ColumnDefault>>,

    /// LiveQueryManager for WebSocket notifications (optional, shared when set)
    live_query_manager: Option<Arc<LiveQueryManager>>,

    /// Storage registry for resolving full storage paths (optional, shared)
    storage_registry: Option<Arc<crate::storage::StorageRegistry>>,
}

impl UserTableShared {
    /// Create a new shared state for a user table
    ///
    /// # Arguments
    /// * `table_id` - Arc<TableId> created once at registration (zero-allocation cache lookups)
    /// * `unified_cache` - Reference to unified SchemaCache for metadata lookups
    /// * `schema` - Arrow schema for the table
    /// * `store` - UserTableStore for DML operations
    pub fn new(
        table_id: Arc<TableId>,
        unified_cache: Arc<SchemaRegistry>,
        schema: SchemaRef,
        store: Arc<UserTableStore>,
    ) -> Arc<Self> {
        let insert_handler = Arc::new(UserTableInsertHandler::new(store.clone()));
        let update_handler = Arc::new(UserTableUpdateHandler::new(store.clone()));
        let delete_handler = Arc::new(UserTableDeleteHandler::new(store.clone()));
        let column_defaults = Arc::new(Self::derive_column_defaults(&schema));

        let core = TableProviderCore::new(
            table_id,
            TableType::User,
            schema,
            None, // storage_id - will be fetched from cache when needed
            unified_cache,
        );

        Arc::new(Self {
            core,
            store,
            insert_handler,
            update_handler,
            delete_handler,
            column_defaults,
            live_query_manager: None,
            storage_registry: None,
        })
    }

    /// Build default column map for INSERT operations.
    ///
    /// Currently injects SNOWFLAKE_ID default for auto-generated `id` columns.
    fn derive_column_defaults(schema: &SchemaRef) -> HashMap<String, ColumnDefault> {
        let mut defaults = HashMap::new();
        if schema.field_with_name("id").is_ok() {
            defaults.insert(
                "id".to_string(),
                ColumnDefault::function("SNOWFLAKE_ID", vec![]),
            );
        }
        defaults
    }

    /// Configure LiveQueryManager for WebSocket notifications (builder pattern)
    pub fn with_live_query_manager(mut self, manager: Arc<LiveQueryManager>) -> Self {
        // Wire through to all handlers
        let store = self.store.clone();
        self.insert_handler = Arc::new(
            UserTableInsertHandler::new(store.clone())
                .with_live_query_manager(Arc::clone(&manager)),
        );
        self.update_handler = Arc::new(
            UserTableUpdateHandler::new(store.clone())
                .with_live_query_manager(Arc::clone(&manager)),
        );
        self.delete_handler = Arc::new(
            UserTableDeleteHandler::new(store.clone())
                .with_live_query_manager(Arc::clone(&manager)),
        );

        self.live_query_manager = Some(manager);
        self
    }

    /// Set the StorageRegistry for resolving full storage paths (builder pattern)
    pub fn with_storage_registry(mut self, registry: Arc<crate::storage::StorageRegistry>) -> Self {
        self.storage_registry = Some(registry);
        self
    }

    // Accessor methods for shared state
    pub fn core(&self) -> &TableProviderCore {
        &self.core
    }

    pub fn store(&self) -> &Arc<UserTableStore> {
        &self.store
    }

    pub fn insert_handler(&self) -> &Arc<UserTableInsertHandler> {
        &self.insert_handler
    }

    pub fn update_handler(&self) -> &Arc<UserTableUpdateHandler> {
        &self.update_handler
    }

    pub fn delete_handler(&self) -> &Arc<UserTableDeleteHandler> {
        &self.delete_handler
    }

    pub fn column_defaults(&self) -> &Arc<HashMap<String, ColumnDefault>> {
        &self.column_defaults
    }

    pub fn live_query_manager(&self) -> Option<&Arc<LiveQueryManager>> {
        self.live_query_manager.as_ref()
    }

    pub fn storage_registry(&self) -> Option<&Arc<crate::storage::StorageRegistry>> {
        self.storage_registry.as_ref()
    }
}
