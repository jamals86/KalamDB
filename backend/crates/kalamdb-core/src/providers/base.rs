//! Base trait for table providers with unified DML operations
//!
//! This module provides:
//! - BaseTableProvider<K, V> trait for generic table operations
//! - TableProviderCore shared structure for common services
//!
//! **Design Rationale**:
//! - Eliminates ~1200 lines of duplicate code across User/Shared/Stream providers
//! - Generic over storage key (K) and value (V) types
//! - No separate handlers - DML logic implemented directly in providers
//! - Shared core reduces memory overhead (Arc<TableProviderCore> vs per-provider fields)

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::providers::unified_dml;
use crate::schema_registry::TableType;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::constants::SystemColumnNames;
use datafusion::catalog::Session;
use datafusion::datasource::memory::MemTable;
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::{ExecutionPlan, Statistics};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::ids::SeqId;
use kalamdb_commons::models::{NamespaceId, Row, TableName, UserId};
use kalamdb_commons::{StorageKey, TableId};
use std::sync::Arc;

// Re-export types moved to submodules
pub use crate::providers::core::TableProviderCore;
pub use crate::providers::helpers::{
    extract_seq_bounds_from_filter, resolve_user_scope, system_user_id,
};
pub(crate) use crate::providers::parquet::scan_parquet_files_as_batch;
pub use crate::providers::scan_row::{inject_system_columns, rows_to_arrow_batch, ScanRow};

/// Unified trait for all table providers with generic storage abstraction
///
/// **Key Design Decisions**:
/// - Generic K: StorageKey (UserTableRowId, SharedTableRowId, StreamTableRowId)
/// - Generic V: Row type (UserTableRow, SharedTableRow, StreamTableRow)
/// - Extends DataFusion::TableProvider (same struct serves both custom DML + SQL)
/// - No separate handlers - all DML logic in provider implementations
/// - Stateless providers - user_id passed per-operation, not stored per-user
///
/// **Architecture**:
/// ```text
/// ExecutionContext → SessionState.extensions (SessionUserContext)
///                 ↓
/// Provider.scan_rows(state) → extract_user_context(state)
///                           ↓
/// Provider.scan_with_version_resolution_to_kvs(user_id, filter)
/// ```
#[async_trait]
pub trait BaseTableProvider<K: StorageKey, V>: Send + Sync + TableProvider {
    // ===========================
    // Core Metadata (read-only)
    // ===========================

    /// Table identifier (namespace + table name)
    fn table_id(&self) -> &TableId;

    /// Memoized Arrow schema (Phase 10 optimization: 50-100× faster than recomputation)
    fn schema_ref(&self) -> SchemaRef;

    /// Logical table type (User, Shared, Stream)
    ///
    /// Named differently from DataFusion's TableProvider::table_type to avoid ambiguity.
    fn provider_table_type(&self) -> TableType;

    /// Get namespace ID from table_id (default implementation)
    fn namespace_id(&self) -> &NamespaceId {
        self.table_id().namespace_id()
    }

    /// Get table name from table_id (default implementation)
    fn table_name(&self) -> &TableName {
        self.table_id().table_name()
    }

    /// Get RocksDB column family name (default implementation)
    fn column_family_name(&self) -> String {
        format!(
            "{}:{}",
            match <Self as BaseTableProvider<K, V>>::provider_table_type(self) {
                TableType::User => "user_table",
                TableType::Shared => "shared_table",
                TableType::Stream => "stream_table",
                _ => "table",
            },
            self.table_id() // TableId Display: "namespace:table"
        )
    }

    // ===========================
    // Storage Access
    // ===========================

    /// Access to AppContext for SystemColumnsService, SnowflakeGenerator, etc.
    fn app_context(&self) -> &Arc<AppContext>;

    /// Primary key field name from schema definition (e.g., "id", "email")
    fn primary_key_field_name(&self) -> &str;

    // ===========================
    // DML Operations (Synchronous - No Handlers)
    // ===========================

    /// Insert a single row (auto-generates system columns: _seq, _deleted)
    ///
    /// **Implementation**: Calls unified_dml helpers directly
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS (User/Stream use it, Shared ignores it)
    /// * `row_data` - Row containing user-defined columns
    ///
    /// # Returns
    /// Generated storage key (UserTableRowId, SharedTableRowId, or StreamTableRowId)
    ///
    /// # Architecture Note
    /// Providers are stateless. The user_id is passed per-operation by the SQL executor
    /// from ExecutionContext, enabling:
    /// - AS USER impersonation (executor passes subject_user_id)
    /// - Per-request user scoping without per-user provider instances
    /// - Clean separation: executor handles auth/context, provider handles storage
    fn insert(&self, user_id: &UserId, row_data: Row) -> Result<K, KalamDbError>;

    /// Insert multiple rows in a batch (optimized for bulk operations)
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `rows` - Vector of Row objects
    ///
    /// # Default Implementation
    /// Iterates over rows and calls insert() for each. Providers may override
    /// with batch-optimized implementation.
    fn insert_batch(&self, user_id: &UserId, rows: Vec<Row>) -> Result<Vec<K>, KalamDbError> {
        use crate::providers::arrow_json_conversion::coerce_rows;

        // Coerce rows to match schema types (e.g. String -> Timestamp)
        // This ensures real-time events match the storage format
        let coerced_rows = coerce_rows(rows, &self.schema_ref()).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Schema coercion failed: {}", e))
        })?;

        coerced_rows
            .into_iter()
            .map(|row| self.insert(user_id, row))
            .collect()
    }

    /// Update a row by key (appends new version with incremented _seq)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `key` - Storage key identifying the row
    /// * `updates` - Row object with column updates
    ///
    /// # Returns
    /// New storage key (new SeqId for versioning)
    fn update(&self, user_id: &UserId, key: &K, updates: Row) -> Result<K, KalamDbError>;

    /// Delete a row by key (appends tombstone with _deleted=true)
    ///
    /// **Implementation**: Uses version_resolution helpers + unified_dml
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `key` - Storage key identifying the row
    fn delete(&self, user_id: &UserId, key: &K) -> Result<(), KalamDbError>;

    /// Update multiple rows in a batch (default implementation)
    fn update_batch(
        &self,
        user_id: &UserId,
        updates: Vec<(K, Row)>,
    ) -> Result<Vec<K>, KalamDbError> {
        updates
            .into_iter()
            .map(|(key, update)| self.update(user_id, &key, update))
            .collect()
    }

    /// Delete multiple rows in a batch (default implementation)
    fn delete_batch(&self, user_id: &UserId, keys: Vec<K>) -> Result<Vec<()>, KalamDbError> {
        keys.into_iter()
            .map(|key| self.delete(user_id, &key))
            .collect()
    }

    // ===========================
    // Convenience Methods (with default implementations)
    // ===========================

    /// Find row key by ID field value
    ///
    /// Scans rows with version resolution and returns the key of the first row
    /// where `fields.id == id_value`. The returned key K already contains user_id
    /// for user/stream tables (embedded in UserTableRowId/StreamTableRowId).
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS scoping
    /// * `id_value` - Value to search for in the ID field
    ///
    /// # Performance
    /// - User tables: Override uses PK index for O(1) lookup
    /// - Shared tables: Override uses PK index for O(1) lookup
    /// - Stream tables: Uses default implementation (full scan)
    ///
    /// # Note
    /// Providers with PK indexes should override this method for efficient lookups.
    fn find_row_key_by_id_field(
        &self,
        user_id: &UserId,
        id_value: &str,
    ) -> Result<Option<K>, KalamDbError> {
        // Default implementation: full table scan with version resolution
        // Providers with PK indexes override this for O(1) lookups
        let rows = self.scan_with_version_resolution_to_kvs(user_id, None, None, None, false)?;

        log::trace!(
            "[find_row_key_by_id_field] Scanning {} rows for pk='{}', value='{}', user='{}'",
            rows.len(),
            self.primary_key_field_name(),
            id_value,
            user_id.as_str()
        );

        for (key, row) in rows {
            let fields = Self::extract_row(&row);
            if let Some(pk_val) = fields.get(self.primary_key_field_name()) {
                if scalar_value_matches_id(pk_val, id_value) {
                    return Ok(Some(key));
                }
            }
        }

        Ok(None)
    }

    /// Update a row by primary key value directly (no key lookup needed)
    ///
    /// This is more efficient than `update()` because it doesn't need to load
    /// the prior row just to extract the PK value - we already have it.
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `pk_value` - Primary key value (e.g., "user123")
    /// * `updates` - Row object with column updates
    ///
    /// # Returns
    /// New storage key (new SeqId for versioning)
    fn update_by_pk_value(
        &self,
        user_id: &UserId,
        pk_value: &str,
        updates: Row,
    ) -> Result<K, KalamDbError>;

    /// Update a row by searching for matching ID field value
    fn update_by_id_field(
        &self,
        user_id: &UserId,
        id_value: &str,
        updates: Row,
    ) -> Result<K, KalamDbError> {
        // Directly update by PK value - no need to find key first, then load row to extract PK
        self.update_by_pk_value(user_id, id_value, updates)
    }

    /// Delete a row by searching for matching ID field value.
    ///
    /// Returns `true` if a row was deleted, `false` if the row did not exist.
    fn delete_by_id_field(
        &self,
        user_id: &UserId,
        id_value: &str,
    ) -> Result<bool, KalamDbError> {
        if let Some(key) = self.find_row_key_by_id_field(user_id, id_value)? {
            self.delete(user_id, &key)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ===========================
    // DataFusion TableProvider Default Implementations
    // ===========================

    /// Default implementation for supports_filters_pushdown
    fn base_supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        // We support Inexact pushdown for all filters because:
        // 1. We use them for partition pruning (Parquet)
        // 2. We use them for prefix scan / range scan (RocksDB)
        // But we still need DataFusion to apply the filter afterwards to be safe/exact.
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }

    /// Default implementation for statistics
    fn base_statistics(&self) -> Option<Statistics> {
        // TODO: Implement row count estimation from Manifest + RocksDB stats
        None
    }

    /// Default implementation for scan
    async fn base_scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // Combine filters (AND) for pruning and pass to scan_rows
        let combined_filter: Option<Expr> = if filters.is_empty() {
            None
        } else {
            let first = filters[0].clone();
            Some(
                filters[1..]
                    .iter()
                    .cloned()
                    .fold(first, |acc, e| acc.and(e)),
            )
        };

        // Optimization: Pass projection to scan_rows ONLY if filters is empty.
        // If filters exist, we need all columns involved in the filter.
        // Since we return Inexact for pushdown, DataFusion adds a FilterExec after the Scan.
        // So the Scan must return columns needed for the filter.
        // Safest approach: If filters are present, fetch all columns.
        let effective_projection = if filters.is_empty() { projection } else { None };

        let batch = self
            .scan_rows(state, effective_projection, combined_filter.as_ref(), limit)
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(batch.schema(), vec![vec![batch]])?;

        // If filters are empty, batch is already projected, so we scan all columns of MemTable.
        // If filters are present, batch has all columns, so we apply projection in MemTable scan.
        let final_projection = if filters.is_empty() { None } else { projection };

        mem.scan(state, final_projection, filters, limit).await
    }

    // ===========================
    // Scan Operations (with version resolution)
    // ===========================

    /// Scan rows with optional filter (merges hot + cold storage with version resolution)
    ///
    /// **Called by DataFusion during query execution via TableProvider::scan()**
    ///
    /// The `state` parameter contains SessionUserContext in extensions,
    /// which providers extract to apply RLS filtering.
    ///
    /// **User/Shared Tables**:
    /// 1. Extract user_id from SessionState.config().options().extensions
    /// 2. Scan RocksDB (hot storage)
    /// 3. Scan Parquet files (cold storage)
    /// 4. Apply version resolution (MAX(_seq) per primary key) via DataFusion
    /// 5. Filter _deleted = false
    /// 6. Apply user filter expression
    /// 7. For User tables: Apply RLS (user_id = subject)
    ///
    /// **Stream Tables**:
    /// 1. Extract user_id from SessionState
    /// 2. Scan ONLY RocksDB (hot storage, no Parquet)
    /// 3. Apply TTL filtering
    /// 4. Filter _deleted = false (if applicable)
    /// 5. Apply user filter expression
    /// 6. Apply RLS (user_id = subject)
    ///
    /// # Arguments
    /// * `state` - DataFusion SessionState (contains SessionUserContext)
    /// * `projection` - Optional column projection
    /// * `filter` - Optional DataFusion expression for filtering
    /// * `limit` - Optional limit on number of rows
    ///
    /// # Returns
    /// RecordBatch with resolved, filtered rows
    ///
    /// # Note
    /// Called by DataFusion's TableProvider::scan(). For direct DML operations,
    /// use scan_with_version_resolution_to_kvs().
    fn scan_rows(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filter: Option<&Expr>,
        limit: Option<usize>,
    ) -> Result<RecordBatch, KalamDbError>;

    /// Scan with version resolution returning key-value pairs (for internal DML use)
    ///
    /// Used by UPDATE/DELETE to find current version before appending new version.
    /// Unlike scan_rows(), this is called directly by DML operations with user_id
    /// passed explicitly.
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS scoping
    /// * `filter` - Optional DataFusion expression for filtering
    /// * `since_seq` - Optional sequence number to start scanning from (optimization)
    /// * `limit` - Optional limit on number of rows
    /// * `keep_deleted` - Whether to include soft-deleted rows (tombstones) in the result
    fn scan_with_version_resolution_to_kvs(
        &self,
        user_id: &UserId,
        filter: Option<&Expr>,
        since_seq: Option<SeqId>,
        limit: Option<usize>,
        keep_deleted: bool,
    ) -> Result<Vec<(K, V)>, KalamDbError>;

    /// Extract row fields from provider-specific value type
    ///
    /// Each provider implements this to access the internal `Row` stored on their row type.
    fn extract_row(row: &V) -> &Row;
}

/// Check if a ScalarValue matches a target string value
///
/// Supports string and numeric comparisons for primary key lookups.
fn scalar_value_matches_id(value: &ScalarValue, target: &str) -> bool {
    match value {
        ScalarValue::Utf8(Some(s)) | ScalarValue::LargeUtf8(Some(s)) => s == target,
        ScalarValue::Int64(Some(n)) => {
            n.to_string() == target || target.parse::<i64>().map(|t| *n == t).unwrap_or(false)
        }
        ScalarValue::Int32(Some(n)) => {
            n.to_string() == target || target.parse::<i32>().map(|t| *n == t).unwrap_or(false)
        }
        ScalarValue::Int16(Some(n)) => {
            n.to_string() == target || target.parse::<i16>().map(|t| *n == t).unwrap_or(false)
        }
        ScalarValue::UInt64(Some(n)) => {
            n.to_string() == target || target.parse::<u64>().map(|t| *n == t).unwrap_or(false)
        }
        ScalarValue::UInt32(Some(n)) => {
            n.to_string() == target || target.parse::<u32>().map(|t| *n == t).unwrap_or(false)
        }
        ScalarValue::Boolean(Some(b)) => b.to_string() == target,
        _ => false,
    }
}

/// Check if a filter expression references the _deleted column
pub fn filter_uses_deleted_column(filter: &Expr) -> bool {
    use datafusion::logical_expr::utils::expr_to_columns;
    use std::collections::HashSet;
    let mut columns = HashSet::new();
    if expr_to_columns(filter, &mut columns).is_ok() {
        columns.iter().any(|c| c.name == SystemColumnNames::DELETED)
    } else {
        false
    }
}

/// Locate the latest non-deleted row matching the provided primary-key value
///
/// **DEPRECATED**: Use `pk_exists_in_cold` for existence checks (returns bool, faster).
/// This function is kept for UPDATE/DELETE operations that need the actual row data.
pub fn find_row_by_pk<P, K, V>(
    provider: &P,
    scope: Option<&UserId>,
    pk_value: &str,
) -> Result<Option<(K, V)>, KalamDbError>
where
    P: BaseTableProvider<K, V>,
    K: StorageKey,
{
    let user_scope = resolve_user_scope(scope);
    let resolved =
        provider.scan_with_version_resolution_to_kvs(user_scope, None, None, None, false)?;
    let pk_name = provider.primary_key_field_name();

    for (key, row) in resolved.into_iter() {
        let fields = P::extract_row(&row);
        if let Some(val) = fields.get(pk_name) {
            if scalar_value_matches_id(val, pk_value) {
                return Ok(Some((key, row)));
            }
        }
    }

    Ok(None)
}

/// Check if a PK value exists in cold storage (Parquet files) using manifest-based pruning.
///
/// **Optimized for PK existence checks during INSERT**:
/// 1. Load manifest from cache (no disk I/O if cached)
/// 2. Use column_stats min/max to prune segments that definitely don't contain the PK
/// 3. Only scan relevant Parquet files (if any)
/// 4. Scan with version resolution to handle MVCC (latest non-deleted wins)
///
/// This is much faster than `find_row_by_pk` which scans ALL cold storage rows.
///
/// # Arguments
/// * `core` - TableProviderCore for app_context access
/// * `table_id` - Table identifier
/// * `table_type` - TableType (User, Shared, Stream)
/// * `user_id` - Optional user ID for scoping (User tables)
/// * `pk_column` - Name of the primary key column
/// * `pk_column_id` - Column ID of the primary key column (for manifest column_stats lookup)
/// * `pk_value` - The PK value to check for
///
/// # Returns
/// * `Ok(true)` - PK exists in cold storage (non-deleted)
/// * `Ok(false)` - PK does not exist in cold storage
pub fn pk_exists_in_cold(
    core: &TableProviderCore,
    table_id: &TableId,
    table_type: TableType,
    user_id: Option<&UserId>,
    pk_column: &str,
    pk_column_id: u64,
    pk_value: &str,
) -> Result<bool, KalamDbError> {
    use crate::manifest::ManifestAccessPlanner;
    use kalamdb_commons::types::Manifest;

    let namespace = table_id.namespace_id();
    let table = table_id.table_name();
    let scope_label = user_id
        .map(|uid| format!("user={}", uid.as_str()))
        .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

    // 1. Get CachedTableData for storage access
    let cached = match core.app_context.schema_registry().get(table_id) {
        Some(c) => c,
        None => {
            log::trace!(
                "[pk_exists_in_cold] No cached table data for {}.{} {} - PK not in cold",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(false);
        }
    };

    // 2. Get Storage from registry (cached lookup) and ObjectStore
    let storage_id = cached.storage_id.clone().unwrap_or_else(kalamdb_commons::models::StorageId::local);
    let storage = match core.app_context.storage_registry().get_storage(&storage_id) {
        Ok(Some(s)) => s,
        Ok(None) | Err(_) => {
            log::trace!(
                "[pk_exists_in_cold] Storage {} not found for {}.{} {} - PK not in cold",
                storage_id.as_str(),
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(false);
        }
    };

    let object_store = cached
        .object_store()
        .into_kalamdb_error("Failed to get object store")?;

    // 3. Get storage path (relative to storage base)
    let storage_path = crate::schema_registry::PathResolver::get_storage_path(&cached, user_id, None)?;

    // Check if any files exist at this path using object_store
    let files = kalamdb_filestore::list_files_sync(
        std::sync::Arc::clone(&object_store),
        &storage,
        &storage_path,
    );

    let all_files = match files {
        Ok(f) => f,
        Err(_) => {
            log::trace!(
                "[pk_exists_in_cold] No storage dir for {}.{} {} - PK not in cold",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(false);
        }
    };

    if all_files.is_empty() {
        log::trace!(
            "[pk_exists_in_cold] No files in storage for {}.{} {} - PK not in cold",
            namespace.as_str(),
            table.as_str(),
            scope_label
        );
        return Ok(false);
    }

    // 4. Load manifest from cache
    let manifest_service = core.app_context.manifest_service();
    let cache_result = manifest_service.get_or_load(table_id, user_id);

    let manifest: Option<Manifest> = match &cache_result {
        Ok(Some(entry)) => Some(entry.manifest.clone()),
        Ok(None) => {
            log::trace!(
                "[pk_exists_in_cold] No manifest for {}.{} {} - checking all files",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            None
        }
        Err(e) => {
            log::warn!(
                "[pk_exists_in_cold] Manifest cache error for {}.{} {}: {}",
                namespace.as_str(),
                table.as_str(),
                scope_label,
                e
            );
            None
        }
    };

    // 5. Use manifest to prune segments or list all Parquet files
    let planner = ManifestAccessPlanner::new();
    let files_to_scan: Vec<String> = if let Some(ref m) = manifest {
        let pruned_paths = planner.plan_by_pk_value(m, pk_column_id, pk_value);
        if pruned_paths.is_empty() {
            log::trace!(
                "[pk_exists_in_cold] Manifest pruning: PK {} not in any segment range for {}.{} {}",
                pk_value,
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(false); // PK definitely not in cold storage
        }
        log::trace!(
            "[pk_exists_in_cold] Manifest pruning: {} of {} segments may contain PK {} for {}.{} {}",
            pruned_paths.len(),
            m.segments.len(),
            pk_value,
            namespace.as_str(),
            table.as_str(),
            scope_label
        );
        pruned_paths
    } else {
        // No manifest - use all Parquet files from listing
        all_files
            .into_iter()
            .filter(|p| p.ends_with(".parquet"))
            .collect()
    };

    if files_to_scan.is_empty() {
        return Ok(false);
    }

    // 6. Scan pruned Parquet files and check for PK using object_store
    // Manifest paths are just filenames (e.g., "batch-0.parquet"), so prepend storage_path
    for file_name in files_to_scan {
        let parquet_path = format!("{}/{}", storage_path.trim_end_matches('/'), file_name);
        if pk_exists_in_parquet_via_store(
            std::sync::Arc::clone(&object_store),
            &storage,
            &parquet_path,
            pk_column,
            pk_value,
        )? {
            log::trace!(
                "[pk_exists_in_cold] Found PK {} in {} for {}.{} {}",
                pk_value,
                parquet_path,
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(true);
        }
    }

    Ok(false)
}

/// Batch check if any PK values exist in cold storage (Parquet files).
///
/// **OPTIMIZED for batch INSERT**: Checks multiple PK values in a single pass through cold storage.
/// This is O(files) instead of O(files × N) where N is the number of PK values.
///
/// # Arguments
/// * `core` - TableProviderCore for app_context access
/// * `table_id` - Table identifier
/// * `table_type` - TableType (User, Shared, Stream)
/// * `user_id` - Optional user ID for scoping (User tables)
/// * `pk_column` - Name of the primary key column
/// * `pk_column_id` - Column ID of the primary key column (for manifest column_stats lookup)
/// * `pk_values` - The PK values to check for
///
/// # Returns
/// * `Ok(Some(pk))` - First PK that exists in cold storage (non-deleted)
/// * `Ok(None)` - None of the PKs exist in cold storage
pub fn pk_exists_batch_in_cold(
    core: &TableProviderCore,
    table_id: &TableId,
    table_type: TableType,
    user_id: Option<&UserId>,
    pk_column: &str,
    pk_column_id: u64,
    pk_values: &[String],
) -> Result<Option<String>, KalamDbError> {
    use crate::manifest::ManifestAccessPlanner;
    use kalamdb_commons::types::Manifest;
    use std::collections::HashSet;

    if pk_values.is_empty() {
        return Ok(None);
    }

    let namespace = table_id.namespace_id();
    let table = table_id.table_name();
    let scope_label = user_id
        .map(|uid| format!("user={}", uid.as_str()))
        .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

    // 1. Get CachedTableData for storage access
    let cached = match core.app_context.schema_registry().get(table_id) {
        Some(c) => c,
        None => {
            log::trace!(
                "[pk_exists_batch_in_cold] No cached table data for {}.{} {} - PK not in cold",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(None);
        }
    };

    // 2. Get Storage from registry (cached lookup) and ObjectStore
    let storage_id = cached.storage_id.clone().unwrap_or_else(kalamdb_commons::models::StorageId::local);
    let storage = match core.app_context.storage_registry().get_storage(&storage_id) {
        Ok(Some(s)) => s,
        Ok(None) | Err(_) => {
            log::trace!(
                "[pk_exists_batch_in_cold] Storage {} not found for {}.{} {} - PK not in cold",
                storage_id.as_str(),
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(None);
        }
    };

    let object_store = cached
        .object_store()
        .into_kalamdb_error("Failed to get object store")?;

    // 3. Get storage path (relative to storage base)
    let storage_path = crate::schema_registry::PathResolver::get_storage_path(&cached, user_id, None)?;

    // Check if any files exist at this path using object_store
    let files = kalamdb_filestore::list_files_sync(
        std::sync::Arc::clone(&object_store),
        &storage,
        &storage_path,
    );

    let all_files = match files {
        Ok(f) => f,
        Err(_) => {
            log::trace!(
                "[pk_exists_batch_in_cold] No storage dir for {}.{} {} - PK not in cold",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(None);
        }
    };

    if all_files.is_empty() {
        log::trace!(
            "[pk_exists_batch_in_cold] No files in storage for {}.{} {} - PK not in cold",
            namespace.as_str(),
            table.as_str(),
            scope_label
        );
        return Ok(None);
    }

    // 4. Load manifest from cache
    let manifest_service = core.app_context.manifest_service();
    let cache_result = manifest_service.get_or_load(table_id, user_id);

    let manifest: Option<Manifest> = match &cache_result {
        Ok(Some(entry)) => Some(entry.manifest.clone()),
        Ok(None) => {
            log::trace!(
                "[pk_exists_batch_in_cold] No manifest for {}.{} {} - checking all files",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            None
        }
        Err(e) => {
            log::warn!(
                "[pk_exists_batch_in_cold] Manifest cache error for {}.{} {}: {}",
                namespace.as_str(),
                table.as_str(),
                scope_label,
                e
            );
            None
        }
    };

    // 5. Determine files to scan - union of files that may contain any of the PK values
    let planner = ManifestAccessPlanner::new();
    let files_to_scan: Vec<String> = if let Some(ref m) = manifest {
        // Collect all potentially relevant files for any PK value
        let mut relevant_files: HashSet<String> = HashSet::new();
        for pk_value in pk_values {
            let pruned_paths = planner.plan_by_pk_value(m, pk_column_id, pk_value);
            relevant_files.extend(pruned_paths);
        }
        if relevant_files.is_empty() {
            log::trace!(
                "[pk_exists_batch_in_cold] Manifest pruning: no segments may contain any PK for {}.{} {}",
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(None);
        }
        log::trace!(
            "[pk_exists_batch_in_cold] Manifest pruning: {} of {} segments may contain {} PKs for {}.{} {}",
            relevant_files.len(),
            m.segments.len(),
            pk_values.len(),
            namespace.as_str(),
            table.as_str(),
            scope_label
        );
        relevant_files.into_iter().collect()
    } else {
        // No manifest - use all Parquet files from listing
        all_files
            .into_iter()
            .filter(|p| p.ends_with(".parquet"))
            .collect()
    };

    if files_to_scan.is_empty() {
        return Ok(None);
    }

    // 6. Create a HashSet for O(1) PK lookups
    let pk_set: HashSet<&str> = pk_values.iter().map(|s| s.as_str()).collect();

    // 7. Scan Parquet files and check for PKs (batch version)
    for file_name in files_to_scan {
        let parquet_path = format!("{}/{}", storage_path.trim_end_matches('/'), file_name);
        if let Some(found_pk) = pk_exists_batch_in_parquet_via_store(
            std::sync::Arc::clone(&object_store),
            &storage,
            &parquet_path,
            pk_column,
            &pk_set,
        )? {
            log::trace!(
                "[pk_exists_batch_in_cold] Found PK {} in {} for {}.{} {}",
                found_pk,
                parquet_path,
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(Some(found_pk));
        }
    }

    Ok(None)
}

/// Batch check if any PK values exist in a single Parquet file via object_store.
///
/// Returns the first matching PK found (with non-deleted latest version).
fn pk_exists_batch_in_parquet_via_store(
    store: std::sync::Arc<dyn object_store::ObjectStore>,
    storage: &kalamdb_commons::system::Storage,
    parquet_path: &str,
    pk_column: &str,
    pk_values: &std::collections::HashSet<&str>,
) -> Result<Option<String>, KalamDbError> {
    use datafusion::arrow::array::{Array, BooleanArray, Int64Array, UInt64Array};
    use std::collections::HashMap;

    // Read Parquet file via object_store
    let batches = kalamdb_filestore::read_parquet_batches_sync(store, storage, parquet_path)
        .into_kalamdb_error("Failed to read Parquet file")?;

    // Track latest version per PK value: pk_value -> (max_seq, is_deleted)
    let mut versions: HashMap<String, (i64, bool)> = HashMap::new();

    for batch in batches {
        // Find column indices
        let pk_idx = batch.schema().index_of(pk_column).ok();
        let seq_idx = batch.schema().index_of(SystemColumnNames::SEQ).ok();
        let deleted_idx = batch.schema().index_of(SystemColumnNames::DELETED).ok();

        let (Some(pk_i), Some(seq_i)) = (pk_idx, seq_idx) else {
            continue; // Missing required columns
        };

        let pk_col = batch.column(pk_i);
        let seq_col = batch.column(seq_i);
        let deleted_col = deleted_idx.map(|i| batch.column(i));

        for row_idx in 0..batch.num_rows() {
            // Extract PK value as string
            let row_pk = extract_pk_as_string(pk_col.as_ref(), row_idx);
            let Some(row_pk_str) = row_pk else { continue };

            // Only check rows matching target PKs (O(1) lookup in HashSet)
            if !pk_values.contains(row_pk_str.as_str()) {
                continue;
            }

            // Extract _seq
            let seq = if let Some(arr) = seq_col.as_any().downcast_ref::<Int64Array>() {
                arr.value(row_idx)
            } else if let Some(arr) = seq_col.as_any().downcast_ref::<UInt64Array>() {
                arr.value(row_idx) as i64
            } else {
                continue;
            };

            // Extract _deleted
            let deleted = if let Some(del_col) = &deleted_col {
                if let Some(arr) = del_col.as_any().downcast_ref::<BooleanArray>() {
                    arr.value(row_idx)
                } else {
                    false
                }
            } else {
                false
            };

            // Update version tracking
            if let Some((max_seq, _)) = versions.get(&row_pk_str) {
                if seq > *max_seq {
                    versions.insert(row_pk_str, (seq, deleted));
                }
            } else {
                versions.insert(row_pk_str, (seq, deleted));
            }
        }
    }

    // Check if any target PK has a non-deleted latest version
    for (pk, (_, is_deleted)) in versions {
        if !is_deleted && pk_values.contains(pk.as_str()) {
            return Ok(Some(pk));
        }
    }

    Ok(None)
}

/// Check if a PK value exists in a single Parquet file via object_store (with MVCC version resolution).
///
/// Reads the file using object_store and checks if any non-deleted row has the matching PK value.
fn pk_exists_in_parquet_via_store(
    store: std::sync::Arc<dyn object_store::ObjectStore>,
    storage: &kalamdb_commons::system::Storage,
    parquet_path: &str,
    pk_column: &str,
    pk_value: &str,
) -> Result<bool, KalamDbError> {
    use datafusion::arrow::array::{Array, BooleanArray, Int64Array, UInt64Array};
    use std::collections::HashMap;

    // Read Parquet file via object_store
    let batches = kalamdb_filestore::read_parquet_batches_sync(store, storage, parquet_path)
        .into_kalamdb_error("Failed to read Parquet file")?;

    // Track latest version per PK value: pk_value -> (max_seq, is_deleted)
    let mut versions: HashMap<String, (i64, bool)> = HashMap::new();

    for batch in batches {
        // Find column indices
        let pk_idx = batch.schema().index_of(pk_column).ok();
        let seq_idx = batch.schema().index_of(SystemColumnNames::SEQ).ok();
        let deleted_idx = batch.schema().index_of(SystemColumnNames::DELETED).ok();

        let (Some(pk_i), Some(seq_i)) = (pk_idx, seq_idx) else {
            continue; // Missing required columns
        };

        let pk_col = batch.column(pk_i);
        let seq_col = batch.column(seq_i);
        let deleted_col = deleted_idx.map(|i| batch.column(i));

        for row_idx in 0..batch.num_rows() {
            // Extract PK value as string
            let row_pk = extract_pk_as_string(pk_col.as_ref(), row_idx);
            let Some(row_pk_str) = row_pk else { continue };

            // Only check rows matching target PK
            if row_pk_str != pk_value {
                continue;
            }

            // Extract _seq
            let seq = if let Some(arr) = seq_col.as_any().downcast_ref::<Int64Array>() {
                arr.value(row_idx)
            } else if let Some(arr) = seq_col.as_any().downcast_ref::<UInt64Array>() {
                arr.value(row_idx) as i64
            } else {
                continue;
            };

            // Extract _deleted
            let deleted = if let Some(del_col) = &deleted_col {
                if let Some(arr) = del_col.as_any().downcast_ref::<BooleanArray>() {
                    arr.value(row_idx)
                } else {
                    false
                }
            } else {
                false
            };

            // Update version tracking
            if let Some((max_seq, _)) = versions.get(&row_pk_str) {
                if seq > *max_seq {
                    versions.insert(row_pk_str, (seq, deleted));
                }
            } else {
                versions.insert(row_pk_str, (seq, deleted));
            }
        }
    }

    // Check if target PK has a non-deleted latest version
    if let Some((_, is_deleted)) = versions.get(pk_value) {
        Ok(!*is_deleted)
    } else {
        Ok(false)
    }
}

/// Extract PK value as string from an Arrow array at given index.
fn extract_pk_as_string(col: &dyn datafusion::arrow::array::Array, idx: usize) -> Option<String> {
    use datafusion::arrow::array::{Int16Array, Int32Array, Int64Array, StringArray, UInt32Array, UInt64Array};

    if col.is_null(idx) {
        return None;
    }

    if let Some(arr) = col.as_any().downcast_ref::<StringArray>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<Int64Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<Int32Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<Int16Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<UInt64Array>() {
        return Some(arr.value(idx).to_string());
    }
    if let Some(arr) = col.as_any().downcast_ref::<UInt32Array>() {
        return Some(arr.value(idx).to_string());
    }

    None
}

/// Ensure an INSERT payload either auto-generates or provides a unique primary-key value
///
/// This uses find_row_key_by_id_field which providers can override to use PK indexes
/// for O(1) lookup instead of scanning all rows.
///
/// **Optimization**: If the PK column is AUTO_INCREMENT or SNOWFLAKE_ID, this check
/// is skipped since the system guarantees unique values.
pub fn ensure_unique_pk_value<P, K, V>(
    provider: &P,
    scope: Option<&UserId>,
    row_data: &Row,
) -> Result<(), KalamDbError>
where
    P: BaseTableProvider<K, V>,
    K: StorageKey,
{
    let table_id = provider.table_id();
    
    // Get table definition to check if PK is auto-increment
    if let Some(table_def) = provider.app_context().schema_registry().get_table_definition(table_id)? {
        // Fast path: Skip uniqueness check if PK is auto-increment
        if crate::pk::PkExistenceChecker::is_auto_increment_pk(&table_def) {
            log::trace!(
                "[ensure_unique_pk_value] Skipping PK check for {} - PK is auto-increment",
                table_id
            );
            return Ok(());
        }
    }

    let pk_name = provider.primary_key_field_name();
    if let Some(pk_value) = row_data.get(pk_name) {
        if !matches!(pk_value, ScalarValue::Null) {
            let pk_str = unified_dml::extract_user_pk_value(row_data, pk_name)?;
            let user_scope = resolve_user_scope(scope);
            // Use find_row_key_by_id_field which can use PK index (O(1) vs O(n))
            if provider
                .find_row_key_by_id_field(user_scope, &pk_str)?
                .is_some()
            {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Primary key violation: value '{}' already exists in column '{}'",
                    pk_str, pk_name
                )));
            }
        }
    }
    Ok(())
}

/// Log a warning when scanning version resolution without filter or limit.
///
/// This helps identify potential performance issues where full table scans are happening.
/// Called by `scan_with_version_resolution_to_kvs` implementations.
///
/// # Arguments
/// * `table_id` - Table identifier for logging
/// * `filter` - Optional filter expression
/// * `limit` - Optional limit
/// * `table_type` - Type of table (User, Shared, Stream)
pub fn warn_if_unfiltered_scan(
    table_id: &TableId,
    filter: Option<&Expr>,
    limit: Option<usize>,
    table_type: TableType,
) {
    if filter.is_none() && limit.is_none() {
        log::warn!(
            "⚠️  [UNFILTERED SCAN] table={} type={} | No filter or limit provided - scanning ALL rows. \
             This may cause performance issues for large tables.",
            table_id,
            table_type.as_str()
        );
    }
}

/// Validate that an UPDATE operation doesn't change the PK to an existing value
///
/// This is called when an UPDATE includes the PK column in the SET clause.
/// If the new PK value already exists (for a different row), returns an error.
///
/// **Skip conditions**:
/// - PK value is not being changed (new value == old value)
/// - PK column is not in the updates
/// - PK column has AUTO_INCREMENT (not allowed to be updated)
///
/// # Arguments
/// * `provider` - The table provider to check against
/// * `scope` - Optional user ID for scoping (User tables)
/// * `updates` - The Row containing update values
/// * `current_pk_value` - The current PK value of the row being updated
///
/// # Returns
/// * `Ok(())` if the update is valid
/// * `Err(AlreadyExists)` if the new PK value already exists
/// * `Err(InvalidOperation)` if trying to change an auto-increment PK
pub fn validate_pk_update<P, K, V>(
    provider: &P,
    scope: Option<&UserId>,
    updates: &Row,
    current_pk_value: &ScalarValue,
) -> Result<(), KalamDbError>
where
    P: BaseTableProvider<K, V>,
    K: StorageKey,
{
    let table_id = provider.table_id();
    let pk_name = provider.primary_key_field_name();

    // Check if PK is in the update values
    let new_pk_value = match updates.get(pk_name) {
        Some(v) if !matches!(v, ScalarValue::Null) => v,
        _ => return Ok(()), // PK not being updated, nothing to validate
    };

    // Check if the value is actually changing
    if new_pk_value == current_pk_value {
        return Ok(()); // Same value, no change
    }

    // Get table definition to check if PK is auto-increment
    if let Some(table_def) = provider.app_context().schema_registry().get_table_definition(table_id)? {
        if crate::pk::PkExistenceChecker::is_auto_increment_pk(&table_def) {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot modify auto-increment primary key column '{}' in table {}",
                pk_name,
                table_id
            )));
        }
    }

    // Check if the new PK value already exists
    let new_pk_str = unified_dml::extract_user_pk_value(updates, pk_name)?;
    let user_scope = resolve_user_scope(scope);

    if provider
        .find_row_key_by_id_field(user_scope, &new_pk_str)?
        .is_some()
    {
        return Err(KalamDbError::AlreadyExists(format!(
            "Primary key violation: value '{}' already exists in column '{}' (UPDATE would create duplicate)",
            new_pk_str, pk_name
        )));
    }

    log::trace!(
        "[validate_pk_update] PK change validated: {} -> {} for {}",
        current_pk_value,
        new_pk_str,
        table_id
    );

    Ok(())
}

/// Apply limit to a vector of results after version resolution.
///
/// Common helper used by both User and Shared table providers.
pub fn apply_limit<T>(result: &mut Vec<T>, limit: Option<usize>) {
    if let Some(l) = limit {
        if result.len() > l {
            result.truncate(l);
        }
    }
}

/// Calculate scan limit for RocksDB based on user-provided limit.
///
/// We scan more than the limit to account for version resolution and tombstones.
/// Default is 100,000 if no limit is provided.
pub fn calculate_scan_limit(limit: Option<usize>) -> usize {
    limit.map(|l| std::cmp::max(l * 2, 1000)).unwrap_or(100_000)
}
