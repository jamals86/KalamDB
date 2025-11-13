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

use super::base::{BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use crate::tables::user_tables::user_table_store::{UserTableRow, UserTableStore};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemTable;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::ids::UserTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{Role, TableId};
use kalamdb_store::entity_store::EntityStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use serde_json::json;
use crate::live_query::manager::ChangeNotification;

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
    
    /// Table identifier
    table_id: Arc<TableId>,
    
    /// Logical table type
    table_type: TableType,
    
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
        table_id: TableId,
        store: Arc<UserTableStore>,
        primary_key_field_name: String,
    ) -> Self {
        Self {
            core,
            table_id: Arc::new(table_id),
            table_type: TableType::User,
            store,
            primary_key_field_name,
        }
    }
    
    /// Extract user context from DataFusion SessionState
    ///
    /// **Purpose**: Read (user_id, role) from SessionState.config.options.extensions
    /// injected by ExecutionContext.create_session_with_user()
    ///
    /// **Returns**: (UserId, Role) tuple for RLS enforcement
    fn extract_user_context(state: &dyn Session) -> Result<(UserId, Role), KalamDbError> {
        use crate::sql::executor::models::SessionUserContext;
        
        let session_state = state.as_any()
            .downcast_ref::<datafusion::execution::context::SessionState>()
            .ok_or_else(|| KalamDbError::InvalidOperation("Expected SessionState".to_string()))?;
        
        let user_ctx = session_state
            .config()
            .options()
            .extensions
            .get::<SessionUserContext>()
            .ok_or_else(|| KalamDbError::InvalidOperation("SessionUserContext not found in extensions".to_string()))?;
        
        Ok((user_ctx.user_id.clone(), user_ctx.role.clone()))
    }
}

impl BaseTableProvider<UserTableRowId, UserTableRow> for UserTableProvider {
    fn table_id(&self) -> &TableId {
        &self.table_id
    }
    
    fn schema_ref(&self) -> SchemaRef {
        // Get memoized Arrow schema from SchemaRegistry via AppContext
        self.core.app_context
            .schema_registry()
            .get_arrow_schema(&self.table_id)
            .expect("Failed to get Arrow schema from registry")
    }
    
    fn provider_table_type(&self) -> TableType { self.table_type }
    
    fn app_context(&self) -> &Arc<AppContext> {
        &self.core.app_context
    }
    
    fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }
    
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        // Generate new SeqId via SystemColumnsService
        let sys_cols = self.core.system_columns;
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
            KalamDbError::InvalidOperation(format!(
                "Failed to insert user table row: {}",
                e
            ))
        })?;
        
        log::debug!(
            "Inserted user table row for user {} with _seq {}",
            user_id.as_str(),
            seq_id
        );

        // Fire live query notification (INSERT)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

            //TODO: Should we keep the user_id in the row JSON?
            // Flatten row fields and include user_id for filter matching
            let mut obj = entity
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::insert(table_name, row_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        
        Ok(row_key)
    }
    
    fn update(&self, user_id: &UserId, key: &UserTableRowId, updates: JsonValue) -> Result<UserTableRowId, KalamDbError> {
        let update_obj = updates.as_object().cloned().ok_or_else(|| {
            KalamDbError::InvalidSql("UPDATE requires object of column assignments".to_string())
        })?;

        // Load referenced version to extract PK
        let prior = EntityStore::get(&*self.store, key)
            .map_err(|e| KalamDbError::Other(format!("Failed to load prior version: {}", e)))?
            .ok_or_else(|| KalamDbError::NotFound("Row not found for update".to_string()))?;

        let pk_name = self.primary_key_field_name().to_string();
        let pk_value = crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;

        // Find latest resolved row for this PK under same user
        let resolved = self.scan_with_version_resolution_to_kvs(user_id, None)?;
        let mut latest_opt: Option<(UserTableRowId, UserTableRow)> = None;
        for (k, r) in resolved.into_iter() {
            if let Some(val) = r.fields.get(&pk_name) {
                if val.to_string() == pk_value {
                    latest_opt = Some((k, r));
                    break;
                }
            }
        }
        let (_latest_key, latest_row) = latest_opt
            .ok_or_else(|| KalamDbError::NotFound(format!("Row with {}={} not found", pk_name, pk_value)))?;

        // Merge updates onto latest
        let mut merged = latest_row
            .fields
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (k, v) in update_obj.into_iter() { merged.insert(k, v); }

        let new_fields = JsonValue::Object(merged);
        let sys_cols = self.core.system_columns;
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = UserTableRow { user_id: user_id.clone(), _seq: seq_id, _deleted: false, fields: new_fields };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update user table row: {}", e))
        })?;

        // Fire live query notification (UPDATE)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

            //TODO: Should we keep the user_id in the row JSON?
            // Old data: latest prior resolved row
            let mut old_obj = latest_row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            old_obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let old_json = JsonValue::Object(old_obj);

            // New data: merged entity
            let mut new_obj = entity
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            new_obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let new_json = JsonValue::Object(new_obj);

            let notification = ChangeNotification::update(table_name, old_json, new_json);
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
        let _pk_value = crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;

        let sys_cols = self.core.system_columns;
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = UserTableRow { user_id: user_id.clone(), _seq: seq_id, _deleted: true, fields: serde_json::json!({}) };
        let row_key = UserTableRowId::new(user_id.clone(), seq_id);
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete user table row: {}", e))
        })?;

        // Fire live query notification (DELETE soft)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

                        //TODO: Should we keep the user_id in the row JSON?
            // Provide prior fields for filter matching, include user_id
            let mut obj = prior
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::delete_soft(table_name, row_json);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }
        Ok(())
    }
    
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id from SessionState for RLS
        let (user_id, _role) = Self::extract_user_context(state)?;
        
        // Perform KV scan with version resolution
        let kvs = self.scan_with_version_resolution_to_kvs(&user_id, filter)?;

        // Convert rows to JSON values aligned with current schema
        let schema = self.schema_ref();
        let mut rows: Vec<JsonValue> = Vec::with_capacity(kvs.len());
        let has_seq = schema.field_with_name("_seq").is_ok();
        let has_deleted = schema.field_with_name("_deleted").is_ok();

        for (_key, row) in kvs.into_iter() {
            // Base fields from stored row
            let mut obj = row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();

            if has_seq {
                obj.insert("_seq".to_string(), json!(row._seq.as_i64()));
            }
            if has_deleted {
                obj.insert("_deleted".to_string(), json!(row._deleted));
            }

            rows.push(JsonValue::Object(obj));
        }

        // Build record batch from JSON rows
        let batch = json_rows_to_arrow_batch(&schema, rows)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))?;

        Ok(batch)
    }
    
    fn scan_with_version_resolution_to_kvs(
        &self,
    user_id: &UserId,
    _filter: Option<&Expr>,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        // 1) Scan hot storage (RocksDB) with user_id prefix filter
        // Build prefix bytes: [len:1][user_id]
        let user_bytes = user_id.as_str().as_bytes();
        let len = (user_bytes.len().min(255)) as u8;
        let mut prefix = Vec::with_capacity(1 + len as usize);
        prefix.push(len);
        prefix.extend_from_slice(&user_bytes[..len as usize]);

        // Use streaming-limited scan to avoid unbounded memory; default cap consistent with EntityStore::scan_all
        let raw = self
            .store
            .scan_limited_with_prefix_bytes(Some(&prefix), 100_000)
            .map_err(|e| KalamDbError::InvalidOperation(format!(
                "Failed to scan user table hot storage: {}",
                e
            )))?;

        // TODO(phase 13.5): Merge cold storage (Parquet) snapshots as well

        // 2) Version resolution: keep MAX(_seq) per primary key; honor tombstones
        use std::collections::HashMap;
        let pk_name = self.primary_key_field_name().to_string();
        let mut best: HashMap<String, (UserTableRowId, UserTableRow)> = HashMap::new();

        for (key_bytes, row) in raw.into_iter() {
            // Parse typed key from bytes
            let maybe_key = kalamdb_commons::ids::UserTableRowId::from_bytes(&key_bytes);
            let key = match maybe_key {
                Ok(k) => k,
                Err(err) => {
                    log::warn!("Skipping invalid UserTableRowId key bytes: {}", err);
                    continue;
                }
            };

            // Extract PK from fields
            let pk_val = match row.fields.get(&pk_name) {
                Some(v) => v,
                None => {
                    // Missing PK in row fields; skip corrupt row
                    log::warn!("Row missing primary key field '{}', skipping", pk_name);
                    continue;
                }
            };

            // Use string key for grouping (stable across primitives)
            let pk_key = pk_val.to_string();

            match best.get(&pk_key) {
                Some((_existing_key, existing_row)) => {
                    if row._seq > existing_row._seq {
                        best.insert(pk_key, (key, row));
                    }
                }
                None => {
                    best.insert(pk_key, (key, row));
                }
            }
        }

        // Filter winners where latest version is not deleted
        let result: Vec<(UserTableRowId, UserTableRow)> = best
            .into_values()
            .filter(|(_k, r)| !r._deleted)
            .collect();
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
            .field("table_id", &self.table_id)
            .field("table_type", &self.table_type)
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
        // Build a single RecordBatch using our scan_rows(), then wrap in MemTable
        let batch = self
            .scan_rows(state, None)
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
