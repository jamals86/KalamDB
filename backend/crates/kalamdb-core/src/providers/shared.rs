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

use super::base::{BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use kalamdb_tables::{SharedTableRow, SharedTableStore};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemTable;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::ids::SharedTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::TableId;
use kalamdb_store::entity_store::EntityStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::providers::arrow_json_conversion::json_rows_to_arrow_batch;
use serde_json::json;

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
    
    /// Table identifier
    table_id: Arc<TableId>,
    
    /// Logical table type
    table_type: TableType,
    
    /// SharedTableStore for DML operations
    store: Arc<SharedTableStore>,
    
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
        table_id: TableId,
        store: Arc<SharedTableStore>,
        primary_key_field_name: String,
    ) -> Self {
        Self {
            core,
            table_id: Arc::new(table_id),
            table_type: TableType::Shared,
            store,
            primary_key_field_name,
        }
    }
}

impl BaseTableProvider<SharedTableRowId, SharedTableRow> for SharedTableProvider {
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
    
    fn insert(&self, _user_id: &UserId, row_data: JsonValue) -> Result<SharedTableRowId, KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        
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
            KalamDbError::InvalidOperation(format!(
                "Failed to insert shared table row: {}",
                e
            ))
        })?;
        
        log::debug!(
            "Inserted shared table row with _seq {}",
            seq_id
        );
        
        Ok(row_key)
    }
    
    fn update(&self, _user_id: &UserId, key: &SharedTableRowId, updates: JsonValue) -> Result<SharedTableRowId, KalamDbError> {
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
        let pk_value = crate::providers::unified_dml::extract_user_pk_value(&prior.fields, &pk_name)?;
        // Resolve latest per PK
        let resolved = self.scan_with_version_resolution_to_kvs(&_user_id.clone(), None)?;

        let mut latest_opt: Option<(SharedTableRowId, SharedTableRow)> = None;
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

        let mut merged = latest_row.fields.as_object().cloned().unwrap_or_default();
        for (k, v) in update_obj.into_iter() { merged.insert(k, v); }
        let new_fields = JsonValue::Object(merged);

        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = SharedTableRow { _seq: seq_id, _deleted: false, fields: new_fields };
        let row_key = seq_id;
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to update shared table row: {}", e))
        })?;
        Ok(row_key)
    }
    
    fn delete(&self, _user_id: &UserId, _key: &SharedTableRowId) -> Result<(), KalamDbError> {
        // IGNORE user_id parameter - no RLS for shared tables
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        let entity = SharedTableRow { _seq: seq_id, _deleted: true, fields: serde_json::json!({}) };
        let row_key = seq_id;
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete shared table row: {}", e))
        })?;
        Ok(())
    }
    
    fn scan_rows(&self, _state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // NO user_id extraction - shared tables scan ALL rows
        let kvs = self.scan_with_version_resolution_to_kvs(&UserId::new("_ignored"), filter)?;

        // Convert to JSON rows aligned with schema
        let schema = self.schema_ref();
        let mut rows: Vec<JsonValue> = Vec::with_capacity(kvs.len());
        let has_seq = schema.field_with_name("_seq").is_ok();
        let has_deleted = schema.field_with_name("_deleted").is_ok();

        for (_key, row) in kvs.into_iter() {
            let mut obj = row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            if has_seq { obj.insert("_seq".to_string(), json!(row._seq.as_i64())); }
            if has_deleted { obj.insert("_deleted".to_string(), json!(row._deleted)); }
            rows.push(JsonValue::Object(obj));
        }

        let batch = json_rows_to_arrow_batch(&schema, rows)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))?;
        Ok(batch)
    }
    
    fn scan_with_version_resolution_to_kvs(
        &self,
    _user_id: &UserId,
    _filter: Option<&Expr>,
    ) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        // IGNORE user_id parameter - scan ALL rows (hot storage)
        let raw = self
            .store
            .scan_all()
            .map_err(|e| KalamDbError::InvalidOperation(format!(
                "Failed to scan shared table hot storage: {}",
                e
            )))?;

        // TODO(phase 13.5): Merge cold storage (Parquet) snapshots as well

        // Version resolution: MAX(_seq) per PK; honor tombstones
        use std::collections::HashMap;
        let pk_name = self.primary_key_field_name().to_string();
        let mut best: HashMap<String, (SharedTableRowId, SharedTableRow)> = HashMap::new();

        for (key_bytes, row) in raw.into_iter() {
            // Parse key (SeqId) from bytes
            let seq = match kalamdb_commons::ids::SeqId::from_bytes(&key_bytes) {
                Ok(s) => s,
                Err(err) => {
                    log::warn!("Skipping invalid SeqId key bytes: {}", err);
                    continue;
                }
            };
            let key: SharedTableRowId = seq;

            // Extract PK from fields
            let pk_val = match row.fields.get(&pk_name) {
                Some(v) => v,
                None => {
                    log::warn!("Row missing primary key field '{}', skipping", pk_name);
                    continue;
                }
            };
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

        let result: Vec<(SharedTableRowId, SharedTableRow)> = best
            .into_values()
            .filter(|(_k, r)| !r._deleted)
            .collect();
        Ok(result)
    }
    
    fn extract_fields(row: &SharedTableRow) -> Option<&JsonValue> {
        Some(&row.fields)
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for SharedTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedTableProvider")
            .field("table_id", &self.table_id)
            .field("table_type", &self.table_type)
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
        let batch = self
            .scan_rows(state, None)
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
