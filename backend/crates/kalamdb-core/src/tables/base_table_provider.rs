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
use crate::app_context::AppContext;
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
///
/// **Phase 10, US1, FR-005**: Removed `schema` field - now uses memoized Arrow schemas via SchemaRegistry
pub struct TableProviderCore {
    pub table_id: Arc<TableId>,
    pub table_type: TableType,
    pub created_at_ms: u64,
    pub storage_id: Option<StorageId>,
    pub unified_cache: Arc<SchemaRegistry>,
}

impl TableProviderCore {
    /// Create new TableProviderCore (Phase 10, US1, FR-005: no longer requires schema parameter)
    pub fn new(
        table_id: Arc<TableId>,
        table_type: TableType,
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
            created_at_ms,
            storage_id,
            unified_cache,
        }
    }

    pub fn table_id(&self) -> &TableId {
        &self.table_id
    }

    /// Get memoized Arrow schema for this table (Phase 10, US1, FR-005)
    ///
    /// Delegates to SchemaRegistry.get_arrow_schema() which implements double-check locking
    /// for 50-100× performance improvement over repeated schema computation.
    ///
    /// **Performance**: ~1.5μs cached, ~75μs first access (50-100× speedup)
    ///
    /// # Returns
    /// Arc-wrapped Arrow Schema from cache
    ///
    /// # Errors
    /// - `KalamDbError::TableNotFound` if table not in cache
    /// - `KalamDbError::SchemaError` if Arrow conversion fails
    pub fn arrow_schema(&self) -> Result<Arc<datafusion::arrow::datatypes::Schema>, crate::error::KalamDbError> {
        self.unified_cache.get_arrow_schema(&self.table_id)
    }

    /// Get Arrow schema as SchemaRef (legacy compatibility)
    ///
    /// **Deprecated**: Prefer `arrow_schema()` for memoized access
    pub fn schema_ref(&self) -> SchemaRef {
        // Fall back to arrow_schema() with error handling
        self.arrow_schema()
            .expect("Failed to get Arrow schema from cache")
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

    /// Application context for SystemColumnsService and other dependencies
    app_context: Arc<AppContext>,

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
    /// * `unified_cache` - SchemaRegistry for table metadata
    /// * `schema` - Arrow schema for the table
    /// * `store` - UserTableStore for DML operations
    /// * `app_context` - Application context for SystemColumnsService and other dependencies
    pub fn new(
        table_id: Arc<TableId>,
        unified_cache: Arc<SchemaRegistry>,
        schema: SchemaRef,
        store: Arc<UserTableStore>,
        app_context: Arc<AppContext>,
    ) -> Arc<Self> {
        let insert_handler = Arc::new(UserTableInsertHandler::new(app_context.clone(), store.clone()));
        let update_handler = Arc::new(UserTableUpdateHandler::new(store.clone()));
        let delete_handler = Arc::new(UserTableDeleteHandler::new(store.clone()));
        let column_defaults = Arc::new(Self::derive_column_defaults(&schema));

        let core = TableProviderCore::new(
            table_id,
            TableType::User,
            None, // storage_id - will be fetched from cache when needed
            unified_cache,
        );

        Arc::new(Self {
            core,
            app_context,
            store,
            insert_handler,
            update_handler,
            delete_handler,
            column_defaults,
            live_query_manager: None,
            storage_registry: None,
        })
    }

    /// Attach a LiveQueryManager after construction (used during table creation & bootstrap)
    ///
    /// This avoids needing a different constructor signature while allowing late binding
    /// when AppContext.live_query_manager() is available.
    pub fn attach_live_query_manager(&mut self, manager: Arc<LiveQueryManager>) {
        let store = self.store.clone();
        let app_context = self.app_context.clone();
        self.insert_handler = Arc::new(
            UserTableInsertHandler::new(app_context.clone(), store.clone()).with_live_query_manager(Arc::clone(&manager)),
        );
        self.update_handler = Arc::new(
            UserTableUpdateHandler::new(store.clone()).with_live_query_manager(Arc::clone(&manager)),
        );
        self.delete_handler = Arc::new(
            UserTableDeleteHandler::new(store.clone()).with_live_query_manager(Arc::clone(&manager)),
        );
        self.live_query_manager = Some(manager);
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
        let app_context = self.app_context.clone();
        self.insert_handler = Arc::new(
            UserTableInsertHandler::new(app_context.clone(), store.clone())
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

// ============================================================================
// Phase 3 (US1): Unified Scan Helpers - Version Resolution + Deletion Filter
// ============================================================================

/// Unified scan pipeline for tables with version resolution and deletion filtering
///
/// This helper consolidates the common scan logic used by both UserTableProvider and SharedTableProvider:
/// 1. Combine RocksDB (fast storage) + Parquet (long-term storage) batches
/// 2. Apply version resolution (select MAX(_updated) per row_id with tie-breaker)
/// 3. Filter out soft-deleted records (_deleted = false)
/// 4. Apply limit (post-resolution)
///
/// # Arguments
/// * `fast_batch` - RecordBatch from RocksDB (FastStorage, priority=2)
/// * `long_batch` - RecordBatch from Parquet files (LongStorage, priority=1)
/// * `schema` - Full schema including system columns (_updated, _deleted)
/// * `limit` - Optional row limit to apply after filtering
///
/// # Returns
/// Final RecordBatch with latest versions of non-deleted records
///
/// # Performance
/// - Version resolution: O(n) with HashMap grouping + Arrow take() kernel
/// - Deletion filter: O(n) with Arrow compute kernels (vectorized operations)
pub async fn scan_with_version_resolution_and_filter(
    fast_batch: datafusion::arrow::record_batch::RecordBatch,
    long_batch: datafusion::arrow::record_batch::RecordBatch,
    schema: Arc<datafusion::arrow::datatypes::Schema>,
    limit: Option<usize>,
) -> Result<datafusion::arrow::record_batch::RecordBatch, datafusion::error::DataFusionError> {
    use crate::tables::version_resolution::resolve_latest_version;
    use datafusion::arrow::array::BooleanArray;
    use datafusion::arrow::compute::filter_record_batch;

    // STEP 1: Version Resolution (select MAX(_updated) per row_id)
    log::debug!(
        "Version resolution: fast={} rows, long={} rows",
        fast_batch.num_rows(),
        long_batch.num_rows()
    );
    
    let resolved_batch = resolve_latest_version(fast_batch, long_batch, schema.clone())
        .await
        .map_err(|e| datafusion::error::DataFusionError::Execution(format!("Version resolution failed: {}", e)))?;
    
    log::debug!("After version resolution: {} rows", resolved_batch.num_rows());

    // STEP 2: Apply deletion filter (_deleted = false)
    let deleted_col_idx = schema.fields().iter()
        .position(|f| f.name() == "_deleted")
        .ok_or_else(|| datafusion::error::DataFusionError::Execution("Missing _deleted column".to_string()))?;
    
    let deleted_array = resolved_batch.column(deleted_col_idx)
        .as_any()
        .downcast_ref::<BooleanArray>()
        .ok_or_else(|| datafusion::error::DataFusionError::Execution("_deleted column is not BooleanArray".to_string()))?;
    
    // Create filter: NOT _deleted (invert boolean array)
    let keep_mask = datafusion::arrow::compute::not(deleted_array)
        .map_err(|e| datafusion::error::DataFusionError::Execution(format!("Failed to invert _deleted mask: {}", e)))?;
    
    let filtered_batch = filter_record_batch(&resolved_batch, &keep_mask)
        .map_err(|e| datafusion::error::DataFusionError::Execution(format!("Failed to filter deleted rows: {}", e)))?;
    
    log::debug!("After deletion filter: {} rows", filtered_batch.num_rows());

    // STEP 3: Apply limit (post-resolution limit)
    let final_batch = if let Some(limit_value) = limit {
        let batch_limit = std::cmp::min(limit_value, filtered_batch.num_rows());
        filtered_batch.slice(0, batch_limit)
    } else {
        filtered_batch
    };

    log::debug!("Final batch: {} rows", final_batch.num_rows());
    Ok(final_batch)
}

/// Create empty RecordBatch with system columns (_updated, _deleted)
///
/// Helper to create schema with system columns added to base schema.
/// Used by both UserTableProvider and SharedTableProvider.
///
/// # Arguments
/// * `base_schema` - Base table schema (user-defined columns only)
///
/// # Returns
/// SchemaRef with _updated (Utf8) and _deleted (Boolean) columns appended
pub fn schema_with_system_columns(
    base_schema: &SchemaRef,
) -> SchemaRef {
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    
    let mut fields = base_schema.fields().to_vec();
    
    // Add system columns: _updated (String RFC3339) and _deleted (Boolean)
    fields.push(Arc::new(Field::new(
        "_updated",
        DataType::Utf8, // RFC3339 string for version resolution compatibility
        false, // NOT NULL
    )));
    fields.push(Arc::new(Field::new(
        "_deleted",
        DataType::Boolean,
        false, // NOT NULL
    )));
    
    Arc::new(Schema::new(fields))
}

// ============================================================================
// Parquet Scanning Helpers (Phase 3, US1: Eliminate Duplication)
// ============================================================================

/// Create empty RecordBatch with correct schema
///
/// Generic helper used by both UserTableProvider and SharedTableProvider
/// when no data is found (empty RocksDB scan or no Parquet files).
///
/// # Arguments
/// * `schema` - Full schema including system columns
///
/// # Returns
/// Empty RecordBatch with 0 rows but correct schema
pub fn create_empty_batch(
    schema: &SchemaRef,
) -> Result<datafusion::arrow::record_batch::RecordBatch, datafusion::error::DataFusionError> {
    use datafusion::arrow::array::new_null_array;
    
    let arrays: Vec<datafusion::arrow::array::ArrayRef> = schema
        .fields()
        .iter()
        .map(|f| new_null_array(f.data_type(), 0))
        .collect();
    
    datafusion::arrow::record_batch::RecordBatch::try_new(schema.clone(), arrays)
        .map_err(|e| datafusion::error::DataFusionError::Execution(format!("Empty batch creation failed: {}", e)))
}

/// Scan Parquet files from storage directory and return concatenated RecordBatch
///
/// Generic Parquet scanning logic shared by UserTableProvider and SharedTableProvider.
/// Handles:
/// - Directory scanning for .parquet files
/// - Parquet file reading with ParquetRecordBatchReaderBuilder
/// - Batch concatenation
/// - Empty results
///
/// # Arguments
/// * `storage_path` - Full path to storage directory (already resolved with user_id if applicable)
/// * `schema` - Full schema including system columns
/// * `table_identifier` - Human-readable table identifier for logging (e.g., "app.users")
///
/// # Returns
/// Concatenated RecordBatch from all Parquet files, or empty batch if no files found
///
/// # RLS Note
/// Caller is responsible for ensuring storage_path includes proper user_id isolation.
/// This function does NOT enforce RLS - it scans whatever directory is provided.
pub async fn scan_parquet_files_as_batch(
    storage_path: &str,
    schema: &SchemaRef,
    table_identifier: &str,
) -> Result<datafusion::arrow::record_batch::RecordBatch, datafusion::error::DataFusionError> {
    use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::fs;
    use std::path::Path;

    if storage_path.is_empty() {
        log::debug!(
            "Storage path is empty for table {}, returning empty batch",
            table_identifier
        );
        return create_empty_batch(schema);
    }

    let storage_dir = Path::new(storage_path);
    log::debug!(
        "Scanning Parquet files in: {} (exists: {})",
        storage_path,
        storage_dir.exists()
    );

    if !storage_dir.exists() {
        log::debug!("Storage directory does not exist, returning empty result");
        return create_empty_batch(schema);
    }

    let parquet_files: Vec<_> = fs::read_dir(storage_dir)
        .map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!("Failed to read storage directory: {}", e))
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("parquet"))
        .map(|entry| entry.path())
        .collect();

    log::debug!("Found {} Parquet files for {}", parquet_files.len(), table_identifier);

    if parquet_files.is_empty() {
        return create_empty_batch(schema);
    }

    let mut all_batches = Vec::new();

    for parquet_file in parquet_files {
        log::debug!("Reading Parquet file: {:?}", parquet_file);

        let file = fs::File::open(&parquet_file).map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!(
                "Failed to open Parquet file {:?}: {}",
                parquet_file, e
            ))
        })?;

        let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!(
                "Failed to create Parquet reader for {:?}: {}",
                parquet_file, e
            ))
        })?;

        let reader = builder.build().map_err(|e| {
            datafusion::error::DataFusionError::Execution(format!(
                "Failed to build Parquet reader for {:?}: {}",
                parquet_file, e
            ))
        })?;

        for batch_result in reader {
            let batch = batch_result.map_err(|e| {
                datafusion::error::DataFusionError::Execution(format!(
                    "Failed to read batch from {:?}: {}",
                    parquet_file, e
                ))
            })?;

            all_batches.push(batch);
        }
    }

    log::debug!("Total batches from Parquet files: {}", all_batches.len());

    // Concatenate all batches
    if all_batches.is_empty() {
        create_empty_batch(schema)
    } else {
        datafusion::arrow::compute::concat_batches(schema, &all_batches)
            .map_err(|e| datafusion::error::DataFusionError::Execution(format!("Failed to concatenate batches: {}", e)))
    }
}
