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
            "{}:{}:{}",
            match <Self as BaseTableProvider<K, V>>::provider_table_type(self) {
                TableType::User => "user_table",
                TableType::Shared => "shared_table",
                TableType::Stream => "stream_table",
                _ => "table",
            },
            self.namespace_id().as_str(),
            self.table_name().as_str()
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
        columns.iter().any(|c| c.name == "_deleted")
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
    pk_value: &str,
) -> Result<bool, KalamDbError> {
    use crate::manifest::ManifestAccessPlanner;
    use kalamdb_commons::types::Manifest;
    use std::path::PathBuf;

    let namespace = table_id.namespace_id();
    let table = table_id.table_name();
    let scope_label = user_id
        .map(|uid| format!("user={}", uid.as_str()))
        .unwrap_or_else(|| format!("scope={}", table_type.as_str()));

    // 1. Get storage path and check if it exists
    let storage_path = core
        .app_context
        .schema_registry()
        .get_storage_path(table_id, user_id, None)?;
    let storage_dir = PathBuf::from(&storage_path);

    if !storage_dir.exists() {
        log::trace!(
            "[pk_exists_in_cold] No storage dir for {}.{} {} - PK not in cold",
            namespace.as_str(),
            table.as_str(),
            scope_label
        );
        return Ok(false);
    }

    // 2. Load manifest from cache
    let manifest_cache_service = core.app_context.manifest_cache_service();
    let cache_result = manifest_cache_service.get_or_load(table_id, user_id);

    let manifest: Option<Manifest> = match &cache_result {
        Ok(Some(entry)) => match serde_json::from_str::<Manifest>(&entry.manifest_json) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!(
                    "[pk_exists_in_cold] Failed to parse manifest for {}.{} {}: {}",
                    namespace.as_str(),
                    table.as_str(),
                    scope_label,
                    e
                );
                None
            }
        },
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

    // 3. Use manifest to prune segments
    let planner = ManifestAccessPlanner::new();
    let files_to_scan: Vec<PathBuf> = if let Some(ref m) = manifest {
        let pruned_paths = planner.plan_by_pk_value(m, pk_column, pk_value);
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
            .into_iter()
            .map(|p| storage_dir.join(p))
            .collect()
    } else {
        // No manifest - scan all Parquet files (fallback)
        let entries = std::fs::read_dir(&storage_dir).map_err(KalamDbError::Io)?;
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("parquet"))
            .collect()
    };

    if files_to_scan.is_empty() {
        return Ok(false);
    }

    // 4. Scan pruned Parquet files and check for PK
    // We need to apply version resolution (latest _seq wins, _deleted=false)
    for parquet_file in files_to_scan {
        if pk_exists_in_parquet_file(&parquet_file, pk_column, pk_value)? {
            log::trace!(
                "[pk_exists_in_cold] Found PK {} in {:?} for {}.{} {}",
                pk_value,
                parquet_file,
                namespace.as_str(),
                table.as_str(),
                scope_label
            );
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check if a PK value exists in a single Parquet file (with MVCC version resolution).
///
/// Reads the file and checks if any non-deleted row has the matching PK value.
fn pk_exists_in_parquet_file(
    parquet_file: &std::path::Path,
    pk_column: &str,
    pk_value: &str,
) -> Result<bool, KalamDbError> {
    use datafusion::arrow::array::{
        Array, BooleanArray, Int64Array, UInt64Array,
    };
    use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::collections::HashMap;

    let file = std::fs::File::open(parquet_file).map_err(KalamDbError::Io)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .into_kalamdb_error("Failed to create Parquet reader")?;
    let reader = builder
        .build()
        .into_kalamdb_error("Failed to build Parquet reader")?;

    // Track latest version per PK value: pk_value -> (max_seq, is_deleted)
    let mut versions: HashMap<String, (i64, bool)> = HashMap::new();

    for batch_result in reader {
        let batch = batch_result
            .into_kalamdb_error("Failed to read Parquet batch")?;

        // Find column indices
        let pk_idx = batch.schema().index_of(pk_column).ok();
        let seq_idx = batch.schema().index_of("_seq").ok();
        let deleted_idx = batch.schema().index_of("_deleted").ok();

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
pub fn ensure_unique_pk_value<P, K, V>(
    provider: &P,
    scope: Option<&UserId>,
    row_data: &Row,
) -> Result<(), KalamDbError>
where
    P: BaseTableProvider<K, V>,
    K: StorageKey,
{
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
            "⚠️  [UNFILTERED SCAN] table={}.{} type={} | No filter or limit provided - scanning ALL rows. \
             This may cause performance issues for large tables.",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            table_type.as_str()
        );
    }
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
