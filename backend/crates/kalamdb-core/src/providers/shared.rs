//! Shared table provider implementation without RLS
//!
//! This module provides SharedTableProvider implementing BaseTableProvider<SharedTableRowId, SharedTableRow>
//! for cross-user shared tables (no Row-Level Security).
//!
//! **Key Features**:
//! - Direct fields (no wrapper layer)
//! - Shared core via Arc<TableProviderCore>
//! - No handlers - all DML logic inline
//! - NO RLS - ignores user_id parameter (operates on all rows)
//! - SessionState NOT extracted in scan_rows() (scans all rows)
//! - PK Index: Uses SharedTableIndexedStore for efficient O(1) lookups by PK value

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::providers::base::{self, BaseTableProvider, TableProviderCore};
use crate::providers::manifest_helpers::{ensure_manifest_ready, load_row_from_parquet_by_seq};
use crate::schema_registry::TableType;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::ids::SharedTableRowId;
use kalamdb_commons::models::{Row, UserId};
use kalamdb_commons::TableId;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{SharedTableIndexedStore, SharedTablePkIndex, SharedTableRow};
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::providers::version_resolution::{merge_versioned_rows, parquet_batch_to_rows};

/// Shared table provider without RLS
///
/// **Architecture**:
/// - Stateless provider (user context passed but ignored)
/// - Direct fields (no wrapper layer)
/// - Shared core via Arc<TableProviderCore>
/// - NO RLS - user_id parameter ignored in all operations
/// - Uses SharedTableIndexedStore for efficient PK lookups
pub struct SharedTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,

    /// SharedTableIndexedStore for DML operations with PK index
    pub(crate) store: Arc<SharedTableIndexedStore>,

    /// PK index for efficient lookups
    pk_index: SharedTablePkIndex,

    /// Cached primary key field name
    primary_key_field_name: String,

    /// Cached Arrow schema (prevents panics if table is dropped while provider is in use)
    schema: SchemaRef,
}

impl SharedTableProvider {
    /// Create a new shared table provider
    ///
    /// # Arguments
    /// * `core` - Shared core with app_context and optional services
    /// * `table_id` - Table identifier
    /// * `store` - SharedTableIndexedStore for this table
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        store: Arc<SharedTableIndexedStore>,
        primary_key_field_name: String,
    ) -> Self {
        // Cache schema at creation time to avoid "Table not found" panics if table is dropped
        // while provider is still in use by a query plan
        let schema = core
            .app_context
            .schema_registry()
            .get_arrow_schema(core.table_id())
            .expect("Failed to get Arrow schema from registry during provider creation");

        // Create PK index for efficient lookups
        let pk_index = SharedTablePkIndex::new(
            core.table_id(),
            &primary_key_field_name,
        );

        Self {
            core,
            store,
            pk_index,
            primary_key_field_name,
            schema,
        }
    }

    /// Scan Parquet files from cold storage for shared table
    ///
    /// Lists all *.parquet files in the table's storage directory and merges them into a single RecordBatch.
    /// Returns an empty batch if no Parquet files exist.
    ///
    /// **Difference from user tables**: Shared tables have NO user_id partitioning,
    /// so all Parquet files are in the same directory (no subdirectories per user).
    ///
    /// **Phase 4 (US6, T082-T084)**: Integrated with ManifestService for manifest caching.
    /// Logs cache hits/misses and updates last_accessed timestamp. Full query optimization
    /// (batch file pruning based on manifest metadata) implemented in Phase 5 (US2, T119-T123).
    ///
    /// **Manifest-Driven Pruning**: Uses ManifestAccessPlanner to select files based on filter predicates,
    /// enabling row-group level pruning when row_group metadata is available.
    fn scan_parquet_files_as_batch(
        &self,
        filter: Option<&Expr>,
    ) -> Result<RecordBatch, KalamDbError> {
        base::scan_parquet_files_as_batch(
            &self.core,
            self.core.table_id(),
            self.core.table_type(),
            None,
            self.schema_ref(),
            filter,
        )
    }

    /// Find a row by PK value using the PK index for efficient O(1) lookup.
    ///
    /// This method uses the PK index to find all versions of a row with the given PK value,
    /// then returns the latest non-deleted version.
    fn find_by_pk(
        &self,
        pk_value: &ScalarValue,
    ) -> Result<Option<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        // Build index prefix for this PK value
        let prefix = self.pk_index.build_prefix_for_pk(pk_value);

        // Use scan_by_index on the IndexedEntityStore (index 0 is the PK index)
        // scan_by_index returns (key, entity) pairs directly
        let results = self
            .store
            .scan_by_index(0, Some(&prefix), None)
            .into_kalamdb_error("PK index scan failed")?;

        if results.is_empty() {
            return Ok(None);
        }

        if let Some((row_id, row)) = results
            .into_iter()
            .max_by_key(|(row_id, _)| row_id.as_i64())
        {
            if row._deleted {
                Ok(None)
            } else {
                Ok(Some((row_id, row)))
            }
        } else {
            Ok(None)
        }
    }

}

impl BaseTableProvider<SharedTableRowId, SharedTableRow> for SharedTableProvider {
    fn table_id(&self) -> &TableId {
        self.core.table_id()
    }

    fn schema_ref(&self) -> SchemaRef {
        // Return cached schema
        self.schema.clone()
    }

    fn provider_table_type(&self) -> TableType {
        self.core.table_type()
    }

    fn app_context(&self) -> &Arc<AppContext> {
        &self.core.app_context
    }

    fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }

    /// Find row by PK value using the PK index for O(1) lookup.
    ///
    /// OPTIMIZED: Uses `pk_exists_in_hot` for fast hot-path check.
    /// OPTIMIZED: Uses `pk_exists_in_cold` with manifest-based segment pruning for cold storage.
    /// For shared tables, user_id is ignored (no RLS).
    fn find_row_key_by_id_field(
        &self,
        _user_id: &UserId,
        id_value: &str,
    ) -> Result<Option<SharedTableRowId>, KalamDbError> {
        // Use shared helper to parse PK value
        let pk_value = crate::providers::pk_helpers::parse_pk_value(id_value);

        // Fast path: return the latest non-deleted row from hot storage if it exists.
        if let Some((row_id, _row)) = self.find_by_pk(&pk_value)? {
            return Ok(Some(row_id));
        }

        // If hot storage has entries but they're all deleted, allow PK reuse.
        let prefix = self.pk_index.build_prefix_for_pk(&pk_value);
        let hot_has_versions = self
            .store
            .exists_by_index(0, &prefix)
            .into_kalamdb_error("PK index scan failed")?;
        if hot_has_versions {
            return Ok(None);
        }

        // Not found in hot storage - check cold storage using optimized manifest-based lookup
        // This uses column_stats to prune segments that can't contain the PK
        let pk_name = self.primary_key_field_name();
        let exists_in_cold = base::pk_exists_in_cold(
            &self.core,
            self.core.table_id(),
            self.core.table_type(),
            None, // No user scoping for shared tables
            pk_name,
            id_value,
        )?;

        if exists_in_cold {
            log::trace!(
                "[SharedTableProvider] PK {} exists in cold storage",
                id_value
            );
            // Load the actual row_id from cold storage so DML (DELETE/UPDATE) can target it
            if let Some((row_id, _row)) = base::find_row_by_pk(self, None, id_value)? {
                return Ok(Some(row_id));
            }

            return Ok(None);
        }

        Ok(None)
    }

    fn insert(&self, _user_id: &UserId, row_data: Row) -> Result<SharedTableRowId, KalamDbError> {
        ensure_manifest_ready(
            &self.core,
            self.core.table_type(),
            None,
            "SharedTableProvider",
        )?;

        // IGNORE user_id parameter - no RLS for shared tables
        base::ensure_unique_pk_value(self, None, &row_data)?;

        // Generate new SeqId via SystemColumnsService
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;

        // Create SharedTableRow directly
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: false,
            fields: row_data,
        };

        // Key is just the SeqId (SharedTableRowId is type alias for SeqId)
        let row_key = seq_id;

        // Store the entity in RocksDB (hot storage) using insert() to update PK index
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to insert shared table row: {}", e))
        })?;

        log::debug!("Inserted shared table row with _seq {}", seq_id);

        Ok(row_key)
    }

    /// Optimized batch insert using single RocksDB WriteBatch
    ///
    /// **Performance**: This method is significantly faster than calling insert() N times:
    /// - Single mutex acquisition for all SeqId generation
    /// - Single RocksDB WriteBatch for all rows (one disk write vs N)
    /// - Batch PK validation (single scan for large batches, O(1) lookups for small batches)
    ///
    /// # Arguments
    /// * `_user_id` - Ignored for shared tables (no RLS)
    /// * `rows` - Vector of Row objects to insert
    ///
    /// # Returns
    /// Vector of generated SharedTableRowIds (SeqIds)
    fn insert_batch(
        &self,
        _user_id: &UserId,
        rows: Vec<Row>,
    ) -> Result<Vec<SharedTableRowId>, KalamDbError> {
        use crate::error_extensions::KalamDbResultExt;
        use crate::providers::arrow_json_conversion::coerce_rows;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Ensure manifest is ready
        ensure_manifest_ready(
            &self.core,
            self.core.table_type(),
            None,
            "SharedTableProvider",
        )?;

        // Coerce rows to match schema types
        let coerced_rows = coerce_rows(rows, &self.schema_ref()).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Schema coercion failed: {}", e))
        })?;

        let row_count = coerced_rows.len();

        // Batch PK validation: collect all user-provided PK values
        let pk_name = self.primary_key_field_name();
        let mut pk_values_to_check: Vec<String> = Vec::new();
        for row_data in &coerced_rows {
            if let Some(pk_value) = row_data.get(pk_name) {
                if !matches!(pk_value, ScalarValue::Null) {
                    let pk_str = crate::providers::unified_dml::extract_user_pk_value(row_data, pk_name)?;
                    pk_values_to_check.push(pk_str);
                }
            }
        }

        // OPTIMIZED: Check PK existence in hot storage only (cold storage handled below)
        // For small batches (≤2 rows), use direct lookups. For larger batches, use batch scan.
        if !pk_values_to_check.is_empty() {
            if pk_values_to_check.len() <= 2 {
                // Small batch: individual O(1) lookups are efficient
                for pk_str in &pk_values_to_check {
                    let prefix = self.pk_index.build_pk_prefix(pk_str);
                    if self.store.exists_by_index(0, &prefix)
                        .into_kalamdb_error("PK index check failed")? 
                    {
                        return Err(KalamDbError::AlreadyExists(format!(
                            "Primary key violation: value '{}' already exists in column '{}'",
                            pk_str, pk_name
                        )));
                    }
                }
            } else {
                // Larger batch: build all prefixes and use batch scan
                let prefixes: Vec<Vec<u8>> = pk_values_to_check
                    .iter()
                    .map(|pk_str| self.pk_index.build_pk_prefix(pk_str))
                    .collect();

                // Use empty common prefix for shared tables (no user scoping)
                let existing = self.store.exists_batch_by_index(0, &[], &prefixes)
                    .into_kalamdb_error("Batch PK index check failed")?;

                if !existing.is_empty() {
                    // Find which PK matched
                    for pk_str in &pk_values_to_check {
                        let prefix = self.pk_index.build_pk_prefix(pk_str);
                        if existing.contains(&prefix) {
                            return Err(KalamDbError::AlreadyExists(format!(
                                "Primary key violation: value '{}' already exists in column '{}'",
                                pk_str, pk_name
                            )));
                        }
                    }
                    // Fallback if we couldn't match the prefix back (shouldn't happen)
                    return Err(KalamDbError::AlreadyExists(format!(
                        "Primary key violation: a value already exists in column '{}'",
                        pk_name
                    )));
                }
            }

            // OPTIMIZED: Batch cold storage check - O(files) instead of O(files × N)
            // This reads Parquet files ONCE for all PK values instead of N times
            if let Some(found_pk) = base::pk_exists_batch_in_cold(
                &self.core,
                self.core.table_id(),
                self.core.table_type(),
                None, // No user scoping for shared tables
                pk_name,
                &pk_values_to_check,
            )? {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Primary key violation: value '{}' already exists in column '{}'",
                    found_pk, pk_name
                )));
            }
        }

        // Generate all SeqIds in single mutex acquisition
        let sys_cols = self.core.system_columns.clone();
        let seq_ids = sys_cols.generate_seq_ids(row_count)?;

        // Build all entities and keys
        let mut entries: Vec<(SharedTableRowId, SharedTableRow)> = Vec::with_capacity(row_count);
        let mut row_keys: Vec<SharedTableRowId> = Vec::with_capacity(row_count);

        for (row_data, seq_id) in coerced_rows.into_iter().zip(seq_ids.into_iter()) {
            let row_key = seq_id;
            let entity = SharedTableRow {
                _seq: seq_id,
                _deleted: false,
                fields: row_data,
            };

            row_keys.push(row_key);
            entries.push((row_key, entity));
        }

        // Single atomic RocksDB WriteBatch for ALL rows
        self.store.insert_batch(&entries).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to batch insert shared table rows: {}", e))
        })?;

        log::debug!(
            "Batch inserted {} shared table rows with _seq range [{}, {}]",
            row_count,
            row_keys.first().map(|k| k.as_i64()).unwrap_or(0),
            row_keys.last().map(|k| k.as_i64()).unwrap_or(0)
        );

        Ok(row_keys)
    }

    fn update(
        &self,
        _user_id: &UserId,
        key: &SharedTableRowId,
        updates: Row,
    ) -> Result<SharedTableRowId, KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        // Merge updates onto latest resolved row by PK
        let pk_name = self.primary_key_field_name().to_string();
        // Load referenced prior version to derive PK value if not present in updates
        // Try RocksDB first, then Parquet
        let prior_opt = EntityStore::get(&*self.store, key)
            .into_kalamdb_error("Failed to load prior version")?;

        let prior = if let Some(p) = prior_opt {
            p
        } else {
            load_row_from_parquet_by_seq(
                &self.core,
                self.core.table_type(),
                &self.schema,
                None,
                *key,
                |row_data| SharedTableRow {
                    _seq: row_data.seq_id,
                    _deleted: row_data.deleted,
                    fields: row_data.fields,
                },
            )?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?
        };

        let pk_value_scalar = prior.fields.get(&pk_name).cloned().ok_or_else(|| {
            KalamDbError::InvalidOperation(format!("Prior row missing PK {}", pk_name))
        })?;

        // Validate PK update (check if new PK value already exists)
        base::validate_pk_update(self, None, &updates, &pk_value_scalar)?;

        // Resolve latest per PK - first try hot storage (O(1) via PK index), 
        // then fall back to cold storage (Parquet scan)
        let (_latest_key, latest_row) = if let Some(result) = self.find_by_pk(&pk_value_scalar)? {
            result
        } else {
            // Not in hot storage, check cold storage
            let pk_value_str = pk_value_scalar.to_string();
            base::find_row_by_pk(self, None, &pk_value_str)?.ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row with {}={} not found",
                    pk_name, pk_value_scalar
                ))
            })?
        };

        let mut merged = latest_row.fields.values.clone();
        for (k, v) in &updates.values {
            merged.insert(k.clone(), v.clone());
        }
        let new_fields = Row::new(merged);

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: false,
            fields: new_fields,
        };
        let row_key = seq_id;
        // Use insert() to update PK index for the new MVCC version
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update shared table row: {}", e))
        })?;
        Ok(row_key)
    }

    fn update_by_pk_value(
        &self,
        _user_id: &UserId,
        pk_value: &str,
        updates: Row,
    ) -> Result<SharedTableRowId, KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        let pk_name = self.primary_key_field_name().to_string();
        let pk_value_scalar = ScalarValue::Utf8(Some(pk_value.to_string()));

        // Resolve latest per PK - first try hot storage (O(1) via PK index),
        // then fall back to cold storage (Parquet scan)
        let (_latest_key, latest_row) = if let Some(result) = self.find_by_pk(&pk_value_scalar)? {
            result
        } else {
            // Not in hot storage, check cold storage
            base::find_row_by_pk(self, None, pk_value)?.ok_or_else(|| {
                KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value))
            })?
        };

        let mut merged = latest_row.fields.values.clone();
        for (k, v) in &updates.values {
            merged.insert(k.clone(), v.clone());
        }
        let new_fields = Row::new(merged);

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: false,
            fields: new_fields,
        };
        let row_key = seq_id;
        // Use insert() to update PK index for the new MVCC version
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update shared table row: {}", e))
        })?;
        Ok(row_key)
    }

    fn delete(&self, _user_id: &UserId, key: &SharedTableRowId) -> Result<(), KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        // Load referenced version to extract PK so tombstone groups with same logical row
        // Try RocksDB first, then Parquet
        let prior_opt = EntityStore::get(&*self.store, key)
            .into_kalamdb_error("Failed to load prior version")?;

        let prior = if let Some(p) = prior_opt {
            p
        } else {
            load_row_from_parquet_by_seq(
                &self.core,
                self.core.table_type(),
                &self.schema,
                None,
                *key,
                |row_data| SharedTableRow {
                    _seq: row_data.seq_id,
                    _deleted: row_data.deleted,
                    fields: row_data.fields,
                },
            )?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?
        };

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        // Preserve ALL fields in the tombstone
        let values = prior.fields.values.clone();

        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: true,
            fields: Row::new(values),
        };
        let row_key = seq_id;
        // Use insert() to update PK index for the tombstone record
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete shared table row: {}", e))
        })?;
        Ok(())
    }

    fn scan_rows(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filter: Option<&Expr>,
        limit: Option<usize>,
    ) -> Result<RecordBatch, KalamDbError> {
        // Extract sequence bounds from filter to optimize RocksDB scan
        let (since_seq, _until_seq) = if let Some(expr) = filter {
            base::extract_seq_bounds_from_filter(expr)
        } else {
            (None, None)
        };

        let keep_deleted = filter
            .map(base::filter_uses_deleted_column)
            .unwrap_or(false);

        // NO user_id extraction - shared tables scan ALL rows
        let kvs = self.scan_with_version_resolution_to_kvs(
            base::system_user_id(),
            filter,
            since_seq,
            limit,
            keep_deleted,
        )?;

        // Convert to JSON rows aligned with schema
        let schema = self.schema_ref();
        crate::providers::base::rows_to_arrow_batch(&schema, kvs, projection, |_, _| {})
    }

    fn scan_with_version_resolution_to_kvs(
        &self,
        _user_id: &UserId,
        filter: Option<&Expr>,
        since_seq: Option<kalamdb_commons::ids::SeqId>,
        limit: Option<usize>,
        keep_deleted: bool,
    ) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        // Warn if no filter or limit - potential performance issue
        base::warn_if_unfiltered_scan(self.core.table_id(), filter, limit, self.core.table_type());

        // IGNORE user_id parameter - scan ALL rows (hot storage)

        // Construct start_key if since_seq is provided
        let start_key_bytes = if let Some(seq) = since_seq {
            // since_seq is exclusive, so start at seq + 1
            let start_seq = kalamdb_commons::ids::SeqId::from(seq.as_i64() + 1);
            Some(kalamdb_commons::StorageKey::storage_key(&start_seq))
        } else {
            None
        };

        // Calculate scan limit using common helper
        let scan_limit = base::calculate_scan_limit(limit);

        let raw = self
            .store
            .scan_limited_with_prefix_and_start(None, start_key_bytes.as_deref(), scan_limit)
            .map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to scan shared table hot storage: {}",
                    e
                ))
            })?;
        log::debug!("[SharedProvider] RocksDB scan returned {} rows", raw.len());

        // Scan cold storage (Parquet files) - pass filter for pruning
        let parquet_batch = self.scan_parquet_files_as_batch(filter)?;

        let pk_name = self.primary_key_field_name().to_string();

        let hot_rows: Vec<(SharedTableRowId, SharedTableRow)> = raw
            .into_iter()
            .filter_map(|(key_bytes, row)| {
                match kalamdb_commons::ids::SeqId::from_bytes(&key_bytes) {
                    Ok(seq) => Some((seq, row)),
                    Err(err) => {
                        log::warn!("Skipping invalid SeqId key bytes: {}", err);
                        None
                    }
                }
            })
            .collect();

        let cold_rows: Vec<(SharedTableRowId, SharedTableRow)> =
            parquet_batch_to_rows(&parquet_batch)?
                .into_iter()
                .map(|row_data| {
                    let seq_id = row_data.seq_id;
                    (
                        seq_id,
                        SharedTableRow {
                            _seq: seq_id,
                            _deleted: row_data.deleted,
                            fields: row_data.fields,
                        },
                    )
                })
                .collect();

        let mut result = merge_versioned_rows(&pk_name, hot_rows, cold_rows, keep_deleted);

        // Apply limit after resolution using common helper
        base::apply_limit(&mut result, limit);

        log::debug!(
            "[SharedProvider] Version-resolved rows (post-tombstone filter): {}",
            result.len()
        );
        Ok(result)
    }

    fn extract_row(row: &SharedTableRow) -> &Row {
        &row.fields
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for SharedTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let table_id = self.core.table_id_arc();
        f.debug_struct("SharedTableProvider")
            .field("table_id", &table_id)
            .field("table_type", &self.core.table_type())
            .field("primary_key_field_name", &self.primary_key_field_name)
            .finish()
    }
}

// Implement DataFusion TableProvider trait
#[async_trait]
impl TableProvider for SharedTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema_ref()
    }

    fn table_type(&self) -> datafusion::logical_expr::TableType {
        datafusion::logical_expr::TableType::Base
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        self.base_scan(state, projection, filters, limit).await
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        self.base_supports_filters_pushdown(filters)
    }

    fn statistics(&self) -> Option<datafusion::physical_plan::Statistics> {
        self.base_statistics()
    }
}
