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

    /// Delete a row by searching for matching ID field value
    fn delete_by_id_field(&self, user_id: &UserId, id_value: &str) -> Result<(), KalamDbError> {
        let key = self
            .find_row_key_by_id_field(user_id, id_value)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row with {}={} not found",
                    self.primary_key_field_name(),
                    id_value
                ))
            })?;
        self.delete(user_id, &key)
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
