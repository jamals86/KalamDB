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
use datafusion::datasource::TableProvider;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::{ExecutionPlan, Statistics};
use kalamdb_commons::ids::UserTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{Role, StorageKey, TableId};
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{UserTableRow, UserTableStore};
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::live_query::ChangeNotification;
use crate::providers::version_resolution::{merge_versioned_rows, parquet_batch_to_rows};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::Row;
use std::collections::BTreeMap;

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

    /// Cached Arrow schema (prevents panics if table is dropped while provider is in use)
    schema: SchemaRef,
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

    /// Get the primary key field name
    pub fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
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

    /// Ensure manifest.json exists (and is cached) for the current user scope before hot writes
    fn ensure_manifest_ready(&self, user_id: &UserId) -> Result<(), KalamDbError> {
        let table_id = self.core.table_id();
        let namespace = table_id.namespace_id().clone();
        let table = table_id.table_name().clone();
        let manifest_cache = self.core.app_context.manifest_cache_service();

        match manifest_cache.get_or_load(table_id, Some(user_id)) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(e) => {
                log::warn!(
                    "[UserTableProvider] Manifest cache lookup failed for {}.{} user={} err={}",
                    namespace.as_str(),
                    table.as_str(),
                    user_id.as_str(),
                    e
                );
            }
        }

        let manifest_service = self.core.app_context.manifest_service();
        let manifest = manifest_service.ensure_manifest_initialized(
            table_id,
            self.core.table_type(),
            Some(user_id),
        )?;

        let manifest_path = manifest_service
            .manifest_path(table_id, Some(user_id))?
            .to_string_lossy()
            .to_string();

        manifest_cache.stage_before_flush(table_id, Some(user_id), &manifest, manifest_path)?;

        Ok(())
    }

    /// Retrieve a specific row version from Parquet storage by SeqId
    fn get_row_from_parquet(
        &self,
        user_id: &UserId,
        seq_id: kalamdb_commons::ids::SeqId,
    ) -> Result<Option<UserTableRow>, KalamDbError> {
        use datafusion::prelude::{col, lit};
        let filter = col(SystemColumnNames::SEQ).eq(lit(seq_id.as_i64()));
        let batch = self.scan_parquet_files_as_batch(user_id, Some(&filter))?;
        let rows = parquet_batch_to_rows(&batch)?;
        
        if let Some(row_data) = rows.into_iter().next() {
             Ok(Some(UserTableRow {
                user_id: user_id.clone(),
                _seq: row_data.seq_id,
                _deleted: row_data.deleted,
                fields: row_data.fields,
            }))
        } else {
            Ok(None)
        }
    }
}

impl BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider {
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

    fn insert(&self, user_id: &UserId, row_data: Row) -> Result<UserTableRowId, KalamDbError> {
        self.ensure_manifest_ready(user_id)?;

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

            // Flatten row fields (user_id is injected by manager for filtering)
            let obj = entity.fields.values.clone();
            let row = Row::new(obj);

            let notification = ChangeNotification::insert(table_id.clone(), row);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

        Ok(row_key)
    }

    fn update(
        &self,
        user_id: &UserId,
        key: &UserTableRowId,
        updates: Row,
    ) -> Result<UserTableRowId, KalamDbError> {
        // Load referenced version to extract PK
        // Try RocksDB first, then Parquet
        let prior_opt = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?;
            
        let prior = if let Some(p) = prior_opt {
            p
        } else {
            self.get_row_from_parquet(user_id, key.seq)?
                .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?
        };

        let pk_name = self.primary_key_field_name().to_string();
        let pk_value = prior
            .fields
            .get(&pk_name)
            .map(|v| v.to_string())
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Prior row missing PK {}", pk_name))
            })?;

        // Find latest resolved row for this PK under same user
        let (_latest_key, latest_row) = base::find_row_by_pk(self, Some(user_id), &pk_value)?
            .ok_or_else(|| {
                KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value))
            })?;

        // Merge updates onto latest
        let mut merged = latest_row.fields.values.clone();
        for (k, v) in &updates.values {
            merged.insert(k.clone(), v.clone());
        }

        let new_fields = Row::new(merged);
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

            // Old data: latest prior resolved row
            let old_obj = latest_row.fields.values.clone();
            let old_row = Row::new(old_obj);

            // New data: merged entity
            let new_obj = entity.fields.values.clone();
            let new_row = Row::new(new_obj);

            let notification = ChangeNotification::update(table_id.clone(), old_row, new_row);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(row_key)
    }

    fn delete(&self, user_id: &UserId, key: &UserTableRowId) -> Result<(), KalamDbError> {
        // Load referenced version to extract PK (for validation; we append tombstone regardless)
        // Try RocksDB first, then Parquet
        let prior_opt = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?;
            
        let prior = if let Some(p) = prior_opt {
            p
        } else {
            self.get_row_from_parquet(user_id, key.seq)?
                .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?
        };
        
        let pk_name = self.primary_key_field_name().to_string();

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        // Preserve the primary key value in the tombstone so version resolution groups
        // the tombstone with the same logical row and suppresses older versions.
        let pk_val = prior
            .fields
            .get(&pk_name)
            .cloned()
            .unwrap_or(ScalarValue::Null);

        let mut values = BTreeMap::new();
        values.insert(pk_name.clone(), pk_val.clone());

        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: true,
            fields: Row::new(values),
        };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        log::info!(
            "[UserProvider DELETE] Writing tombstone: user={}, _seq={}, PK={}:{}",
            user_id.as_str(),
            seq_id.as_i64(),
            pk_name,
            pk_val
        );
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete user table row: {}", e))
        })?;

        // Fire live query notification (DELETE soft)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            // Provide prior fields for filter matching (user_id injected by manager)
            let obj = prior.fields.values.clone();
            let row = Row::new(obj);

            let notification = ChangeNotification::delete_soft(table_id.clone(), row);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(())
    }

    fn scan_rows(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filter: Option<&Expr>,
        limit: Option<usize>,
    ) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id and role from SessionState for RLS
        let (user_id, role) = Self::extract_user_context(state)?;

        // Extract sequence bounds from filter to optimize RocksDB scan
        let (since_seq, _until_seq) = if let Some(expr) = filter {
            base::extract_seq_bounds_from_filter(expr)
        } else {
            (None, None)
        };

        // All roles operate within the current effective user scope. Admins must use AS USER to
        // impersonate other users instead of bypassing RLS.
        let kvs = self.scan_with_version_resolution_to_kvs(&user_id, filter, since_seq, limit)?;

        let table_id = self.core.table_id();
        log::debug!(
            "[UserTableProvider] scan_rows resolved {} row(s) for user={} role={:?} table={}.{}",
            kvs.len(),
            user_id.as_str(),
            role,
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );

        // Convert rows to JSON values aligned with schema
        let schema = self.schema_ref();
        crate::providers::base::rows_to_arrow_batch(&schema, kvs, projection, |_, _| {})
    }

    fn scan_with_version_resolution_to_kvs(
        &self,
        user_id: &UserId,
        _filter: Option<&Expr>,
        since_seq: Option<kalamdb_commons::ids::SeqId>,
        limit: Option<usize>,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        let table_id = self.core.table_id();
        // 1) Scan hot storage (RocksDB) with per-user filtering using prefix scan
        let user_bytes = user_id.as_str().as_bytes();
        let len = (user_bytes.len().min(255)) as u8;
        let mut prefix = Vec::with_capacity(1 + len as usize);
        prefix.push(len);
        prefix.extend_from_slice(&user_bytes[..len as usize]);

        // Construct start_key if since_seq is provided
        let start_key_bytes = if let Some(seq) = since_seq {
            // since_seq is exclusive, so start at seq + 1
            let start_seq = kalamdb_commons::ids::SeqId::from(seq.as_i64() + 1);
            let key = UserTableRowId::new(user_id.clone(), start_seq);
            Some(key.storage_key())
        } else {
            None
        };

        // Use limit if provided, otherwise default to 100,000
        let scan_limit = limit.map(|l| std::cmp::max(l * 2, 1000)).unwrap_or(100_000);

        let raw_all = self
            .store
            .scan_limited_with_prefix_and_start(
                Some(&prefix),
                start_key_bytes.as_deref(),
                scan_limit,
            )
            .map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to scan user table hot storage: {}",
                    e
                ))
            })?;

        let hot_rows: Vec<(UserTableRowId, UserTableRow)> = raw_all
            .into_iter()
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
                    fields: row_data.fields,
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
        let mut result = merge_versioned_rows(&pk_name, hot_rows, cold_rows);

        // Apply limit after resolution
        if let Some(l) = limit {
            if result.len() > l {
                result.truncate(l);
            }
        }

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

    fn extract_row(row: &UserTableRow) -> &Row {
        &row.fields
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

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
        self.base_supports_filters_pushdown(filters)
    }

    fn statistics(&self) -> Option<Statistics> {
        self.base_statistics()
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
}
