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

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::providers::base::{self, BaseTableProvider, TableProviderCore};
use crate::schema_registry::TableType;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::ids::SharedTableRowId;
use kalamdb_commons::models::{Row, UserId};
use kalamdb_commons::TableId;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{SharedTableRow, SharedTableStore};
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
pub struct SharedTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,

    /// SharedTableStore for DML operations (public for flush jobs)
    pub(crate) store: Arc<SharedTableStore>,

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
    /// * `store` - SharedTableStore for this table
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        store: Arc<SharedTableStore>,
        primary_key_field_name: String,
    ) -> Self {
        // Cache schema at creation time to avoid "Table not found" panics if table is dropped
        // while provider is still in use by a query plan
        let schema = core
            .app_context
            .schema_registry()
            .get_arrow_schema(core.table_id())
            .expect("Failed to get Arrow schema from registry during provider creation");

        Self {
            core,
            store,
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
    /// **Phase 4 (US6, T082-T084)**: Integrated with ManifestCacheService for manifest caching.
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

    fn ensure_manifest_ready(&self) -> Result<(), KalamDbError> {
        let table_id = self.core.table_id();
        let namespace = table_id.namespace_id().clone();
        let table = table_id.table_name().clone();
        let manifest_cache = self.core.app_context.manifest_cache_service();

        match manifest_cache.get_or_load(table_id, None) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(e) => {
                log::warn!(
                    "[SharedTableProvider] Manifest cache lookup failed for {}.{} err={}",
                    namespace.as_str(),
                    table.as_str(),
                    e
                );
            }
        }

        let manifest_service = self.core.app_context.manifest_service();
        let manifest =
            manifest_service.ensure_manifest_initialized(table_id, self.core.table_type(), None)?;

        let manifest_path = manifest_service
            .manifest_path(table_id, None)?
            .to_string_lossy()
            .to_string();

        manifest_cache.stage_before_flush(table_id, None, &manifest, manifest_path)?;

        Ok(())
    }
    /// Retrieve a specific row version from Parquet storage by SeqId
    fn get_row_from_parquet(
        &self,
        seq_id: kalamdb_commons::ids::SeqId,
    ) -> Result<Option<SharedTableRow>, KalamDbError> {
        use datafusion::prelude::{col, lit};
        let filter = col(SystemColumnNames::SEQ).eq(lit(seq_id.as_i64()));
        let batch = self.scan_parquet_files_as_batch(Some(&filter))?;
        let rows = parquet_batch_to_rows(&batch)?;

        if let Some(row_data) = rows.into_iter().next() {
            Ok(Some(SharedTableRow {
                _seq: row_data.seq_id,
                _deleted: row_data.deleted,
                fields: row_data.fields,
            }))
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

    fn insert(&self, _user_id: &UserId, row_data: Row) -> Result<SharedTableRowId, KalamDbError> {
        self.ensure_manifest_ready()?;

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

        // Store the entity in RocksDB (hot storage)
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to insert shared table row: {}", e))
        })?;

        log::debug!("Inserted shared table row with _seq {}", seq_id);

        Ok(row_key)
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
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?;

        let prior = if let Some(p) = prior_opt {
            p
        } else {
            self.get_row_from_parquet(key.clone())?
                .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?
        };

        let pk_value = prior
            .fields
            .get(&pk_name)
            .map(|v| v.to_string())
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Prior row missing PK {}", pk_name))
            })?;

        // Resolve latest per PK
        let (_latest_key, latest_row) =
            base::find_row_by_pk(self, None, &pk_value)?.ok_or_else(|| {
                KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value))
            })?;

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
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update shared table row: {}", e))
        })?;
        Ok(row_key)
    }

    fn delete(&self, _user_id: &UserId, key: &SharedTableRowId) -> Result<(), KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        // Load referenced version to extract PK so tombstone groups with same logical row
        // Try RocksDB first, then Parquet
        let prior_opt = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?;

        let prior = if let Some(p) = prior_opt {
            p
        } else {
            self.get_row_from_parquet(key.clone())?
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
        self.store.put(&row_key, &entity).map_err(|e| {
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

        let keep_deleted = filter.map(|f| base::filter_uses_deleted_column(f)).unwrap_or(false);

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
        _filter: Option<&Expr>,
        since_seq: Option<kalamdb_commons::ids::SeqId>,
        limit: Option<usize>,
        keep_deleted: bool,
    ) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        // IGNORE user_id parameter - scan ALL rows (hot storage)

        // Construct start_key if since_seq is provided
        let start_key_bytes = if let Some(seq) = since_seq {
            // since_seq is exclusive, so start at seq + 1
            let start_seq = kalamdb_commons::ids::SeqId::from(seq.as_i64() + 1);
            Some(kalamdb_commons::StorageKey::storage_key(&start_seq))
        } else {
            None
        };

        // Use limit if provided, otherwise default to 100,000
        // Note: We might need to scan more than limit to account for version resolution/tombstones
        // For now, let's use limit * 2 + 1000 as a heuristic if limit is small, or just 100,000
        let scan_limit = limit.map(|l| std::cmp::max(l * 2, 1000)).unwrap_or(100_000);

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

        // Scan cold storage (Parquet files)
        let parquet_batch = self.scan_parquet_files_as_batch(_filter)?;

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

        // Apply limit after resolution
        if let Some(l) = limit {
            if result.len() > l {
                result.truncate(l);
            }
        }

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
