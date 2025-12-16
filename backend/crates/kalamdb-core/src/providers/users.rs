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
//! - PK Index for efficient row lookup (Phase 14)

use super::base::{self, BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::providers::helpers::extract_user_context;
use crate::providers::manifest_helpers::{ensure_manifest_ready, load_row_from_parquet_by_seq};
use crate::schema_registry::TableType;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::{ExecutionPlan, Statistics};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::ids::UserTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{Role, StorageKey, TableId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{UserTableIndexedStore, UserTablePkIndex, UserTableRow};
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::live_query::ChangeNotification;
use crate::providers::version_resolution::{merge_versioned_rows, parquet_batch_to_rows};
use kalamdb_commons::models::Row;

/// User table provider with RLS
///
/// **Architecture**:
/// - Stateless provider (user context passed per-operation)
/// - Direct fields (no wrapper layer)
/// - Shared core via Arc<TableProviderCore>
/// - RLS enforced via user_id parameter
/// - PK Index for efficient row lookup (Phase 14)
pub struct UserTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,

    /// IndexedEntityStore with PK index for DML operations (public for flush jobs)
    pub(crate) store: Arc<UserTableIndexedStore>,

    /// PK index for efficient lookups
    pk_index: UserTablePkIndex,

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
    /// * `store` - IndexedEntityStore with PK index for this table
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        store: Arc<UserTableIndexedStore>,
        primary_key_field_name: String,
    ) -> Self {
        // Cache schema at creation time to avoid "Table not found" panics if table is dropped
        // while provider is still in use by a query plan
        let schema = core
            .app_context
            .schema_registry()
            .get_arrow_schema(core.table_id())
            .expect("Failed to get Arrow schema from registry during provider creation");

        let pk_index = UserTablePkIndex::new(
            core.table_id().namespace_id().as_str(),
            core.table_id().table_name().as_str(),
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

    /// Get the primary key field name
    pub fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }

    /// Build a complete Row from UserTableRow including system columns (_seq, _deleted)
    ///
    /// This ensures live query notifications include all columns, not just user-defined fields.
    fn build_notification_row(entity: &UserTableRow) -> Row {
        let mut values = entity.fields.values.clone();
        values.insert(
            SystemColumnNames::SEQ.to_string(),
            ScalarValue::Int64(Some(entity._seq.as_i64())),
        );
        values.insert(
            SystemColumnNames::DELETED.to_string(),
            ScalarValue::Boolean(Some(entity._deleted)),
        );
        Row::new(values)
    }

    /// Find a row by primary key value using the PK index
    ///
    /// Returns the latest non-deleted version of the row with the given PK.
    /// This is more efficient than scanning all rows.
    ///
    /// # Arguments
    /// * `user_id` - User scope for RLS
    /// * `pk_value` - Primary key value to search for
    ///
    /// # Returns
    /// Option<(UserTableRowId, UserTableRow)> if found
    pub fn find_by_pk(
        &self,
        user_id: &UserId,
        pk_value: &ScalarValue,
    ) -> Result<Option<(UserTableRowId, UserTableRow)>, KalamDbError> {
        // Build prefix for PK index scan
        let prefix = self
            .pk_index
            .build_prefix_for_pk(user_id.as_str(), pk_value);

        // Scan index for all versions with this PK
        let index_results = self
            .store
            .scan_by_index(0, Some(&prefix), None)
            .into_kalamdb_error("PK index scan failed")?;

        if index_results.is_empty() {
            return Ok(None);
        }

        if let Some((row_id, row)) = index_results
            .into_iter()
            .max_by_key(|(row_id, _)| row_id.seq)
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

    /// Extract user context from DataFusion SessionState
    ///
    /// **Purpose**: Read (user_id, role) from SessionState.config.options.extensions
    /// injected by ExecutionContext.create_session_with_user()
    ///
    /// **Returns**: (UserId, Role) tuple for RLS enforcement
    fn extract_user_context(state: &dyn Session) -> Result<(UserId, Role), KalamDbError> {
        extract_user_context(state)
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

    /// Override find_row_key_by_id_field to use PK index for efficient lookup
    ///
    /// This avoids scanning all rows and instead uses the secondary index.
    /// For hot storage (RocksDB), uses fast existence check. If not found in hot storage,
    /// falls back to checking cold storage using manifest-based pruning.
    ///
    /// OPTIMIZED: Uses `pk_exists_in_hot` for fast hot-path check (single index lookup + 1 entity fetch max).
    /// OPTIMIZED: Uses `pk_exists_in_cold` with manifest-based segment pruning for cold storage.
    fn find_row_key_by_id_field(
        &self,
        user_id: &UserId,
        id_value: &str,
    ) -> Result<Option<UserTableRowId>, KalamDbError> {
        // Use shared helper to parse PK value
        let pk_value = crate::providers::pk_helpers::parse_pk_value(id_value);

        // Fast path: check hot storage for a non-deleted version
        if let Some((row_id, _row)) = self.find_by_pk(user_id, &pk_value)? {
            log::trace!(
                "[UserTableProvider] PK collision in hot storage: id={}",
                id_value
            );
            return Ok(Some(row_id));
        }

        // If hot storage has entries but all are deleted, the PK can be reused.
        let hot_prefix = self
            .pk_index
            .build_prefix_for_pk(user_id.as_str(), &pk_value);
        let hot_has_versions = self
            .store
            .exists_by_index(0, &hot_prefix)
            .into_kalamdb_error("PK index scan failed")?;
        if hot_has_versions {
            log::trace!(
                "[UserTableProvider] PK {} has only deleted versions in hot storage",
                id_value
            );
            return Ok(None);
        }

        log::trace!(
            "[UserTableProvider] PK {} not in hot storage, checking cold",
            id_value
        );

        // Not found in hot storage - check cold storage using optimized manifest-based lookup
        // This uses column_stats to prune segments that can't contain the PK
        let pk_name = self.primary_key_field_name();
        let exists_in_cold = base::pk_exists_in_cold(
            &self.core,
            self.core.table_id(),
            self.core.table_type(),
            Some(user_id),
            pk_name,
            id_value,
        )?;

        if exists_in_cold {
            log::trace!(
                "[UserTableProvider] PK {} exists in cold storage",
                id_value
            );
            // Load the actual row_id from cold storage so DELETE/UPDATE can target correct version
            if let Some((row_id, _row)) =
                base::find_row_by_pk(self, Some(user_id), id_value)?
            {
                return Ok(Some(row_id));
            }

            return Ok(None);
        }

        Ok(None)
    }

    fn insert(&self, user_id: &UserId, row_data: Row) -> Result<UserTableRowId, KalamDbError> {
        ensure_manifest_ready(
            &self.core,
            self.core.table_type(),
            Some(user_id),
            "UserTableProvider",
        )?;

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

        // Store the entity in RocksDB (hot storage) with PK index maintenance
        self.store.insert(&row_key, &entity).map_err(|e| {
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

            // Build complete row including system columns (_seq, _deleted)
            let row = Self::build_notification_row(&entity);

            let notification = ChangeNotification::insert(table_id.clone(), row);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

        Ok(row_key)
    }

    /// Optimized batch insert using single RocksDB WriteBatch
    ///
    /// **Performance**: This method is significantly faster than calling insert() N times:
    /// - Single mutex acquisition for all SeqId generation
    /// - Single RocksDB WriteBatch for all rows (one disk write vs N)
    /// - Batch PK validation (single scan + HashSet lookup instead of N individual checks)
    ///
    /// # Arguments
    /// * `user_id` - Subject user ID for RLS
    /// * `rows` - Vector of Row objects to insert
    ///
    /// # Returns
    /// Vector of generated UserTableRowIds
    fn insert_batch(
        &self,
        user_id: &UserId,
        rows: Vec<Row>,
    ) -> Result<Vec<UserTableRowId>, KalamDbError> {
        use crate::providers::arrow_json_conversion::coerce_rows;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Ensure manifest is ready
        ensure_manifest_ready(
            &self.core,
            self.core.table_type(),
            Some(user_id),
            "UserTableProvider",
        )?;

        // Coerce rows to match schema types (e.g. String -> Timestamp)
        let coerced_rows = coerce_rows(rows, &self.schema_ref()).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Schema coercion failed: {}", e))
        })?;

        let row_count = coerced_rows.len();

        // Batch PK validation: collect all user-provided PK values and their prefixes
        let pk_name = self.primary_key_field_name();
        let mut pk_values_to_check: Vec<(String, Vec<u8>)> = Vec::new();

        for row_data in &coerced_rows {
            if let Some(pk_value) = row_data.get(pk_name) {
                if !matches!(pk_value, ScalarValue::Null) {
                    let pk_str = crate::providers::unified_dml::extract_user_pk_value(row_data, pk_name)?;
                    let prefix = self.pk_index.build_prefix_for_pk(user_id.as_str(), pk_value);
                    pk_values_to_check.push((pk_str, prefix));
                }
            }
        }

        // OPTIMIZED: Check all PKs in single index scan + HashSet lookup
        // For small batches (1-2 PKs), use individual lookups to avoid scan overhead
        if !pk_values_to_check.is_empty() {
            if pk_values_to_check.len() <= 2 {
                // Small batch: individual lookups are faster
                for (pk_str, _prefix) in &pk_values_to_check {
                    if self.find_row_key_by_id_field(user_id, pk_str)?.is_some() {
                        return Err(KalamDbError::AlreadyExists(format!(
                            "Primary key violation: value '{}' already exists in column '{}'",
                            pk_str, pk_name
                        )));
                    }
                }
            } else {
                // Larger batch: use batch index scan for efficiency
                // Build common prefix for this user's PKs
                let user_prefix = self.pk_index.build_user_prefix(user_id.as_str());
                let prefixes: Vec<Vec<u8>> = pk_values_to_check.iter().map(|(_, p)| p.clone()).collect();

                let existing = self.store.exists_batch_by_index(0, &user_prefix, &prefixes)
                    .into_kalamdb_error("Batch PK index scan failed")?;

                // Check if any of the requested PKs already exist
                for (pk_str, prefix) in &pk_values_to_check {
                    if existing.contains(prefix) {
                        return Err(KalamDbError::AlreadyExists(format!(
                            "Primary key violation: value '{}' already exists in column '{}'",
                            pk_str, pk_name
                        )));
                    }
                }
            }
        }

        // Generate all SeqIds in single mutex acquisition
        let sys_cols = self.core.system_columns.clone();
        let seq_ids = sys_cols.generate_seq_ids(row_count)?;

        // Build all entities and keys
        let mut entries: Vec<(UserTableRowId, UserTableRow)> = Vec::with_capacity(row_count);
        let mut row_keys: Vec<UserTableRowId> = Vec::with_capacity(row_count);

        for (row_data, seq_id) in coerced_rows.into_iter().zip(seq_ids.into_iter()) {
            let row_key = UserTableRowId::new(user_id.clone(), seq_id);
            let entity = UserTableRow {
                user_id: user_id.clone(),
                _seq: seq_id,
                _deleted: false,
                fields: row_data,
            };

            row_keys.push(row_key.clone());
            entries.push((row_key, entity));
        }

        // Single atomic RocksDB WriteBatch for ALL rows
        self.store.insert_batch(&entries).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to batch insert user table rows: {}", e))
        })?;

        log::debug!(
            "Batch inserted {} user table rows for user {} with _seq range [{}, {}]",
            row_count,
            user_id.as_str(),
            row_keys.first().map(|k| k.seq.as_i64()).unwrap_or(0),
            row_keys.last().map(|k| k.seq.as_i64()).unwrap_or(0)
        );

        // Fire live query notifications (one per row - async fire-and-forget)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            for (_row_key, entity) in entries.iter() {
                // Build complete row including system columns (_seq, _deleted)
                let row = Self::build_notification_row(entity);
                let notification = ChangeNotification::insert(table_id.clone(), row);
                manager.notify_table_change_async(user_id.clone(), table_id.clone(), notification);
            }
        }

        Ok(row_keys)
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
            .into_kalamdb_error("Failed to load prior version")?;

        let prior = if let Some(p) = prior_opt {
            p
        } else {
            load_row_from_parquet_by_seq(
                &self.core,
                self.core.table_type(),
                &self.schema,
                Some(user_id),
                key.seq,
                |row_data| UserTableRow {
                    user_id: user_id.clone(),
                    _seq: row_data.seq_id,
                    _deleted: row_data.deleted,
                    fields: row_data.fields,
                },
            )?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?
        };

        let pk_name = self.primary_key_field_name().to_string();
        let pk_value_scalar = prior.fields.get(&pk_name).cloned().ok_or_else(|| {
            KalamDbError::InvalidOperation(format!("Prior row missing PK {}", pk_name))
        })?;

        // Find latest resolved row for this PK under same user
        // First try hot storage (O(1) via PK index), then fall back to cold storage (Parquet scan)
        let (_latest_key, latest_row) = if let Some(result) =
            self.find_by_pk(user_id, &pk_value_scalar)?
        {
            result
        } else {
            // Not in hot storage, check cold storage
            let pk_value_str = pk_value_scalar.to_string();
            base::find_row_by_pk(self, Some(user_id), &pk_value_str)?.ok_or_else(|| {
                KalamDbError::NotFound(format!(
                    "Row with {}={} not found",
                    pk_name, pk_value_scalar
                ))
            })?
        };

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
        // Insert new version (MVCC - all writes are inserts with new SeqId)
        log::trace!(
            "[UserProvider UPDATE] Inserting new version: user={}, pk={}, _seq={}",
            user_id.as_str(),
            pk_value_scalar,
            seq_id.as_i64()
        );
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update user table row: {}", e))
        })?;

        // Fire live query notification (UPDATE)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            // Old data: latest prior resolved row (with system columns)
            let old_row = Self::build_notification_row(&latest_row);

            // New data: merged entity (with system columns)
            let new_row = Self::build_notification_row(&entity);

            let notification = ChangeNotification::update(table_id.clone(), old_row, new_row);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(row_key)
    }

    fn update_by_pk_value(
        &self,
        user_id: &UserId,
        pk_value: &str,
        updates: Row,
    ) -> Result<UserTableRowId, KalamDbError> {
        let pk_name = self.primary_key_field_name().to_string();
        let pk_value_scalar = ScalarValue::Utf8(Some(pk_value.to_string()));

        // Find latest resolved row for this PK under same user
        // First try hot storage (O(1) via PK index), then fall back to cold storage (Parquet scan)
        let (_latest_key, latest_row) = if let Some(result) =
            self.find_by_pk(user_id, &pk_value_scalar)?
        {
            result
        } else {
            // Not in hot storage, check cold storage
            base::find_row_by_pk(self, Some(user_id), pk_value)?.ok_or_else(|| {
                KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value))
            })?
        };

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
        // Insert new version (MVCC - all writes are inserts with new SeqId)
        log::trace!(
            "[UserProvider UPDATE_BY_PK] Inserting new version: user={}, pk={}, _seq={}",
            user_id.as_str(),
            pk_value,
            seq_id.as_i64()
        );
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update user table row: {}", e))
        })?;

        // Fire live query notification (UPDATE)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            // Old data: latest prior resolved row (with system columns)
            let old_row = Self::build_notification_row(&latest_row);

            // New data: merged entity (with system columns)
            let new_row = Self::build_notification_row(&entity);

            let notification = ChangeNotification::update(table_id.clone(), old_row, new_row);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(row_key)
    }

    fn delete(&self, user_id: &UserId, key: &UserTableRowId) -> Result<(), KalamDbError> {
        // Load referenced version to extract PK (for validation; we append tombstone regardless)
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
                Some(user_id),
                key.seq,
                |row_data| UserTableRow {
                    user_id: user_id.clone(),
                    _seq: row_data.seq_id,
                    _deleted: row_data.deleted,
                    fields: row_data.fields,
                },
            )?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for delete".to_string()))?
        };

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;

        // Preserve ALL fields in the tombstone so they can be queried if _deleted=true
        // This allows "undo" functionality and auditing of deleted records
        let values = prior.fields.values.clone();

        let entity = UserTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            _deleted: true,
            fields: Row::new(values),
        };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        log::info!(
            "[UserProvider DELETE] Writing tombstone: user={}, _seq={}",
            user_id.as_str(),
            seq_id.as_i64()
        );
        // Insert tombstone version (MVCC - all writes are inserts with new SeqId)
        self.store.insert(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete user table row: {}", e))
        })?;

        // Fire live query notification (DELETE soft)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            // Provide tombstone entity with system columns for filter matching
            let row = Self::build_notification_row(&entity);

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
        let keep_deleted = filter
            .map(base::filter_uses_deleted_column)
            .unwrap_or(false);

        let kvs = self.scan_with_version_resolution_to_kvs(
            &user_id,
            filter,
            since_seq,
            limit,
            keep_deleted,
        )?;

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
        filter: Option<&Expr>,
        since_seq: Option<kalamdb_commons::ids::SeqId>,
        limit: Option<usize>,
        keep_deleted: bool,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        let table_id = self.core.table_id();
        
        // Warn if no filter or limit - potential performance issue
        base::warn_if_unfiltered_scan(table_id, filter, limit, self.core.table_type());

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

        // Calculate scan limit using common helper
        let scan_limit = base::calculate_scan_limit(limit);

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

        log::trace!(
            "[UserProvider] Hot scan: {} rows for user={} (table={}.{})",
            hot_rows.len(),
            user_id.as_str(),
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        );

        // 2) Scan cold storage (Parquet files) - pass filter for pruning
        let parquet_batch = self.scan_parquet_files_as_batch(user_id, filter)?;

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
        let mut result = merge_versioned_rows(&pk_name, hot_rows, cold_rows, keep_deleted);

        // Apply limit after resolution using common helper
        base::apply_limit(&mut result, limit);

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
