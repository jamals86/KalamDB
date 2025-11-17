//! User table provider implementation with RLS
//!
//! This module provides UserTableProvider implementing BaseTableProvider<UserTableRowId, UserTableRow>
//! with Row-Level Security (RLS) enforced via user_id parameter.
//!
//! **Key Features**:
//! - Direct fields (no UserTableShared wrapper)
//! - Shared core via Arc<TableProviderCore>
//! - No handlers - all DML logic inline
//! - RLS via user_id parameter in DML methods
//! - SessionState extraction for scan_rows()

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
use kalamdb_commons::ids::UserTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{Role, TableId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{UserTableRow, UserTableStore};
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::live_query::manager::ChangeNotification;
use crate::providers::version_resolution::{merge_versioned_rows, parquet_batch_to_rows};
use serde_json::json;

/// User table provider with RLS
///
/// **Architecture**:
/// - Stateless provider (user context passed per-operation)
/// - Direct fields (no wrapper layer)
/// - Shared core via Arc<TableProviderCore>
/// - RLS enforced via user_id parameter
pub struct UserTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,

    /// UserTableStore for DML operations (public for flush jobs)
    pub(crate) store: Arc<UserTableStore>,

    /// Cached primary key field name
    primary_key_field_name: String,
}

impl UserTableProvider {
    /// Create a new user table provider
    ///
    /// # Arguments
    /// * `core` - Shared core with app_context and optional services
    /// * `table_id` - Table identifier
    /// * `store` - UserTableStore for this table
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        store: Arc<UserTableStore>,
        primary_key_field_name: String,
    ) -> Self {
        Self {
            core,
            store,
            primary_key_field_name,
        }
    }

    /// Get the primary key field name
    pub fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }

    /// Return a snapshot of all rows as JSON objects (no RLS filtering)
    ///
    /// Used by live initial snapshot where we need table-wide data. RLS is
    /// enforced at the WebSocket layer so this method intentionally returns
    /// all rows for the table.
    ///
    /// Returns rows with user_id field injected, matching notification format.
    pub fn snapshot_all_rows_json(&self) -> Result<Vec<JsonValue>, KalamDbError> {
        use kalamdb_store::entity_store::EntityStore;
        let rows = self.store.scan_all().map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to scan user table store: {}", e))
        })?;
        let count = rows.len();
        let table_id = self.core.table_id();
        log::debug!(
            "[UserTableProvider] snapshot_all_rows_json: table={}.{} rows={}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            count
        );

        // Build flat row JSON: fields + user_id (matching notification format)
        Ok(rows
            .into_iter()
            .map(|(_k, row)| {
                let mut obj = row.fields.as_object().cloned().unwrap_or_default();
                obj.insert(
                    "user_id".to_string(),
                    serde_json::json!(row.user_id.as_str()),
                );
                serde_json::Value::Object(obj)
            })
            .collect())
    }

    /// Extract user context from DataFusion SessionState
    ///
    /// **Purpose**: Read (user_id, role) from SessionState.config.options.extensions
    /// injected by ExecutionContext.create_session_with_user()
    ///
    /// **Returns**: (UserId, Role) tuple for RLS enforcement
    fn extract_user_context(state: &dyn Session) -> Result<(UserId, Role), KalamDbError> {
        use crate::sql::executor::models::SessionUserContext;

        let session_state = state
            .as_any()
            .downcast_ref::<datafusion::execution::context::SessionState>()
            .ok_or_else(|| KalamDbError::InvalidOperation("Expected SessionState".to_string()))?;

        let user_ctx = session_state
            .config()
            .options()
            .extensions
            .get::<SessionUserContext>()
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(
                    "SessionUserContext not found in extensions".to_string(),
                )
            })?;

        Ok((user_ctx.user_id.clone(), user_ctx.role.clone()))
    }

    /// Scan Parquet files from cold storage for a specific user
    ///
    /// Lists all *.parquet files in the user's storage directory and merges them into a single RecordBatch.
    /// Returns an empty batch if no Parquet files exist.
    ///
    /// **Phase 4 (US6, T082-T084)**: Integrated with ManifestCacheService for manifest caching.
    /// Logs cache hits/misses and updates last_accessed timestamp. Full query optimization
    /// (batch file pruning based on manifest metadata) implemented in Phase 5 (US2, T119-T123).
    fn scan_parquet_files_as_batch(
        &self,
        user_id: &UserId,
        filter: Option<&Expr>,
    ) -> Result<RecordBatch, KalamDbError> {
        base::scan_parquet_files_as_batch(
            &self.core,
            self.core.table_id(),
            self.core.table_type(),
            Some(user_id),
            self.schema_ref(),
            filter,
        )
    }
}

impl BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider {
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
        user_id: &UserId,
        row_data: JsonValue,
    ) -> Result<UserTableRowId, KalamDbError> {
        // Validate PRIMARY KEY uniqueness if user provided PK value
        base::ensure_unique_pk_value(self, Some(user_id), &row_data)?;

        // Generate new SeqId via SystemColumnsService
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;

        // Create UserTableRow directly
        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: false,
            fields: row_data,
        };

        // Create composite key
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);

        // Store the entity in RocksDB (hot storage)
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to insert user table row: {}", e))
        })?;

        log::debug!(
            "Inserted user table row for user {} with _seq {}",
            user_id.as_str(),
            seq_id
        );

        // Fire live query notification (INSERT)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            //TODO: Should we keep the user_id in the row JSON?
            // Flatten row fields and include user_id for filter matching
            let mut obj = entity.fields.as_object().cloned().unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::insert(table_id.clone(), row_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

        Ok(row_key)
    }

    fn update(
        &self,
        user_id: &UserId,
        key: &UserTableRowId,
        updates: JsonValue,
    ) -> Result<UserTableRowId, KalamDbError> {
        let update_obj = updates.as_object().cloned().ok_or_else(|| {
            KalamDbError::InvalidSql("UPDATE requires object of column assignments".to_string())
        })?;

        // Load referenced version to extract PK
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?;

        let pk_name = self.primary_key_field_name().to_string();
        let pk_value =
            crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;

        // Find latest resolved row for this PK under same user
        let (_latest_key, latest_row) = base::find_row_by_pk(self, Some(user_id), &pk_value)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value))
            })?;

        // Merge updates onto latest
        let mut merged = latest_row.fields.as_object().cloned().unwrap_or_default();
        for (k, v) in update_obj.into_iter() {
            merged.insert(k, v);
        }

        let new_fields = JsonValue::Object(merged);
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: false,
            fields: new_fields,
        };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update user table row: {}", e))
        })?;

        // Fire live query notification (UPDATE)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            //TODO: Should we keep the user_id in the row JSON?
            // Old data: latest prior resolved row
            let mut old_obj = latest_row.fields.as_object().cloned().unwrap_or_default();
            old_obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let old_json = JsonValue::Object(old_obj);

            // New data: merged entity
            let mut new_obj = entity.fields.as_object().cloned().unwrap_or_default();
            new_obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let new_json = JsonValue::Object(new_obj);

            let notification = ChangeNotification::update(table_id.clone(), old_json, new_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(row_key)
    }

    fn delete(&self, user_id: &UserId, key: &UserTableRowId) -> Result<(), KalamDbError> {
        // Load referenced version to extract PK (for validation; we append tombstone regardless)
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?;
        let pk_name = self.primary_key_field_name().to_string();
        let _pk_value =
            crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        // Preserve the primary key value in the tombstone so version resolution groups
        // the tombstone with the same logical row and suppresses older versions.
        let pk_json_val = prior
            .fields
            .get(&pk_name)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: true,
            fields: serde_json::json!({ pk_name.clone(): pk_json_val.clone() }),
        };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        log::info!(
            "[UserProvider DELETE] Writing tombstone: user={}, _seq={}, PK={}:{}",
            user_id.as_str(),
            seq_id.as_i64(),
            pk_name,
            pk_json_val
        );
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete user table row: {}", e))
        })?;

        // Fire live query notification (DELETE soft)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            //TODO: Should we keep the user_id in the row JSON?
            // Provide prior fields for filter matching, include user_id
            let mut obj = prior.fields.as_object().cloned().unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::delete_soft(table_id.clone(), row_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(())
    }

    fn scan_rows(
        &self,
        state: &dyn Session,
        filter: Option<&Expr>,
    ) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id from SessionState for RLS
        let (user_id, _role) = Self::extract_user_context(state)?;

        // Perform KV scan with version resolution
        let kvs = self.scan_with_version_resolution_to_kvs(&user_id, filter)?;
        let table_id = self.core.table_id();
        log::debug!(
            "[UserTableProvider] scan_rows resolved {} row(s) for user={} table={}.{}",
            kvs.len(),
            user_id.as_str(),
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );

        // Convert rows to JSON values aligned with current schema
        let schema = self.schema_ref();
        crate::providers::base::rows_to_arrow_batch(&schema, kvs, |_, _| {})
    }

    fn scan_with_version_resolution_to_kvs(
        &self,
        user_id: &UserId,
        _filter: Option<&Expr>,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        let table_id = self.core.table_id();
        // 1) Scan hot storage (RocksDB) with per-user filtering
        let raw_all = self.store.scan_all().map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to scan user table hot storage: {}", e))
        })?;
        let hot_rows: Vec<(UserTableRowId, UserTableRow)> = raw_all
            .into_iter()
            .filter(|(_k, r)| &r.user_id == user_id)
            .filter_map(|(key_bytes, row)| {
                match kalamdb_commons::ids::UserTableRowId::from_bytes(&key_bytes) {
                    Ok(k) => Some((k, row)),
                    Err(err) => {
                        log::warn!("Skipping invalid UserTableRowId key bytes: {}", err);
                        None
                    }
                }
            })
            .collect();

        if log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "[UserProvider] Hot scan (filtered): {} rows for user {} (table={}.{})",
                hot_rows.len(),
                user_id.as_str(),
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );
        }

        // 2) Scan cold storage (Parquet files)
        let parquet_batch = self.scan_parquet_files_as_batch(user_id, _filter)?;

        let cold_rows: Vec<(UserTableRowId, UserTableRow)> = parquet_batch_to_rows(&parquet_batch)?
            .into_iter()
            .map(|row_data| {
                let seq_id = row_data.seq_id;
                let row = UserTableRow {
                    user_id: user_id.clone(),
                    _seq: seq_id,
                    _deleted: row_data.deleted,
                    fields: JsonValue::Object(row_data.fields),
                };
                (UserTableRowId::new(user_id.clone(), seq_id), row)
            })
            .collect();

        if log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "[UserProvider] Cold scan: {} Parquet rows (table={}.{}; user={})",
                cold_rows.len(),
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str(),
                user_id.as_str()
            );
        }

        // 3) Version resolution: keep MAX(_seq) per primary key; honor tombstones
        let pk_name = self.primary_key_field_name().to_string();
        let result = merge_versioned_rows(&pk_name, hot_rows, cold_rows);

        if log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "[UserProvider] Final version-resolved (post-tombstone): {} rows (table={}.{}; user={})",
                result.len(),
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str(),
                user_id.as_str()
            );
        }

        Ok(result)
    }

    fn extract_fields(row: &UserTableRow) -> Option<&JsonValue> {
        Some(&row.fields)
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for UserTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserTableProvider")
            .field("table_id", self.core.table_id())
            .field("table_type", &self.core.table_type())
            .field("primary_key_field_name", &self.primary_key_field_name)
            .finish()
    }
}

// Implement DataFusion TableProvider trait
#[async_trait]
impl TableProvider for UserTableProvider {
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
        // log::info!(
        //     "[UserTableProvider::scan] INVOKED for table {}.{}",
        //     self.table_id.namespace_id().as_str(),
        //     self.table_id.table_name().as_str()
        // );
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

        // Current implementation: Unified MemTable (hot RocksDB + cold Parquet) with RLS
        // scan_rows() → scan_parquet_files_as_batch() uses ManifestAccessPlanner.scan_parquet_files()
        //
        // TODO: NEXT PHASE - DataFusion Native ParquetExec Integration with RLS
        // Same pattern as SharedTableProvider but with user_id filtering:
        //
        // STEP 1: Build ParquetExec for user's cold data
        //   - Filter manifest by user_id (already done in scan_parquet_files_as_batch)
        //   - Apply row-group selections from ManifestAccessPlanner
        //
        // STEP 2: Build MemTable for user's hot RocksDB data
        //   - scan_hot_data_for_user(user_id, filter)
        //
        // STEP 3: Union cold + hot with consistent RLS
        //   ```rust
        //   let union_exec = UnionExec::new(vec![parquet_exec, hot_exec]);
        //   ```
        //
        // Note: RLS enforcement happens at file selection level (user_id directory path)
        // and at RocksDB scan level (prefix scan by user_id)
        //
        let batch = self
            .scan_rows(state, combined_filter.as_ref())
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
