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

use super::base::{self, BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::memory::MemTable;
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::ids::SharedTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::TableId;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{SharedTableRow, SharedTableStore};
use serde_json::Value as JsonValue;
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
        Self {
            core,
            store,
            primary_key_field_name,
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
}

impl BaseTableProvider<SharedTableRowId, SharedTableRow> for SharedTableProvider {
    fn table_id(&self) -> &TableId {
        self.core.table_id()
    }

    fn schema_ref(&self) -> SchemaRef {
        // Get memoized Arrow schema from SchemaRegistry via AppContext
        self.core
            .app_context
            .schema_registry()
            .get_arrow_schema(self.core.table_id())
            .expect("Failed to get Arrow schema from registry")
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

    fn insert(
        &self,
        _user_id: &UserId,
        row_data: JsonValue,
    ) -> Result<SharedTableRowId, KalamDbError> {
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
        updates: JsonValue,
    ) -> Result<SharedTableRowId, KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        // Merge updates onto latest resolved row by PK
        let update_obj = updates.as_object().cloned().ok_or_else(|| {
            KalamDbError::InvalidSql("UPDATE requires object of column assignments".to_string())
        })?;

        let pk_name = self.primary_key_field_name().to_string();
        // Load referenced prior version to derive PK value if not present in updates
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?;
        let pk_value =
            crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;
        // Resolve latest per PK
        let (_latest_key, latest_row) =
            base::find_row_by_pk(self, None, &pk_value)?.ok_or_else(|| {
                KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value))
            })?;

        let mut merged = latest_row.fields.as_object().cloned().unwrap_or_default();
        for (k, v) in update_obj.into_iter() {
            merged.insert(k, v);
        }
        let new_fields = JsonValue::Object(merged);

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
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?;

        let pk_name = self.primary_key_field_name().to_string();
        // Preserve existing PK value if present; may be null if malformed
        let pk_json_val = prior
            .fields
            .get(&pk_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        // Include PK in tombstone fields so version resolution collapses correctly
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: true,
            fields: serde_json::json!({ pk_name: pk_json_val }),
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
        filter: Option<&Expr>,
    ) -> Result<RecordBatch, KalamDbError> {
        // NO user_id extraction - shared tables scan ALL rows
        let kvs = self.scan_with_version_resolution_to_kvs(base::system_user_id(), filter)?;

        // Convert to JSON rows aligned with schema
        let schema = self.schema_ref();
        crate::providers::base::rows_to_arrow_batch(&schema, kvs, |_, _| {})
    }

    fn scan_with_version_resolution_to_kvs(
        &self,
        _user_id: &UserId,
        _filter: Option<&Expr>,
    ) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        // IGNORE user_id parameter - scan ALL rows (hot storage)
        let raw = self.store.scan_all().map_err(|e| {
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
                            fields: JsonValue::Object(row_data.fields),
                        },
                    )
                })
                .collect();

        let result = merge_versioned_rows(&pk_name, hot_rows, cold_rows);
        log::debug!(
            "[SharedProvider] Version-resolved rows (post-tombstone filter): {}",
            result.len()
        );
        Ok(result)
    }

    fn extract_fields(row: &SharedTableRow) -> Option<&JsonValue> {
        Some(&row.fields)
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

        // Current implementation: Unified MemTable (hot RocksDB + cold Parquet)
        // scan_rows() already uses ManifestAccessPlanner.scan_parquet_files() for file-level pruning
        //
        // TODO: NEXT PHASE - DataFusion Native ParquetExec Integration
        // Goal: Replace MemTable with hybrid approach for optimal performance:
        //
        // STEP 1: Split data sources
        //   - Cold data: Use ParquetExec with manifest-driven row-group pruning
        //   - Hot data: Use MemTable for RocksDB records
        //
        // STEP 2: Build ParquetExec with row-group selections
        //   ```rust
        //   use datafusion::datasource::physical_plan::parquet::ParquetExec;
        //   use datafusion_physical_plan::parquet::ParquetAccessPlan;
        //
        //   let planner = ManifestAccessPlanner::new();
        //   let selections = planner.plan_by_seq_range(&manifest, min_seq, max_seq);
        //
        //   // Build file-level ParquetAccessPlan
        //   let mut file_groups = Vec::new();
        //   for selection in selections {
        //       let file_path = storage_dir.join(&selection.file_path);
        //       let partitioned_file = PartitionedFile::new(file_path, file_size);
        //
        //       // Row-group access plan per file
        //       let rg_access = if selection.row_groups.is_empty() {
        //           RowGroupAccess::Scan  // No metadata, scan all
        //       } else {
        //           RowGroupAccess::Selection(selection.row_groups.into_iter().collect())
        //       };
        //
        //       file_groups.push((partitioned_file, vec![rg_access]));
        //   }
        //
        //   let parquet_exec = ParquetExec::builder(file_scan_config)
        //       .with_parquet_access_plan(ParquetAccessPlan::new(file_groups))
        //       .build();
        //   ```
        //
        // STEP 3: Union cold + hot
        //   ```rust
        //   use datafusion::physical_plan::union::UnionExec;
        //   let hot_batch = self.scan_hot_data(state, &combined_filter)?;
        //   let hot_mem = MemTable::try_new(schema, vec![vec![hot_batch]])?;
        //   let hot_exec = hot_mem.scan(state, projection, filters, limit).await?;
        //
        //   let union_exec = UnionExec::new(vec![parquet_exec, hot_exec]);
        //   return Ok(Arc::new(union_exec));
        //   ```
        //
        // STEP 4: Enable page-level pruning (when has_page_index=true)
        //   - Use RowSelection API for sub-row-group pruning
        //   - Leverage Parquet page index statistics
        //
        // Benefits:
        //   - True DataFusion-native row-group/page pruning
        //   - Streaming execution (no full Parquet load into memory)
        //   - Parallel file reads via DataFusion scheduler
        //   - Automatic predicate pushdown to Parquet reader
        //
        let batch = self
            .scan_rows(state, combined_filter.as_ref())
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
