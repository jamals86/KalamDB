//! Stream table provider implementation with RLS + TTL
//!
//! This module provides StreamTableProvider implementing BaseTableProvider<StreamTableRowId, StreamTableRow>
//! for ephemeral event streams with Row-Level Security and TTL-based eviction.
//!
//! **Key Features**:
//! - Direct fields (no wrapper layer)
//! - Shared core via Arc<TableProviderCore>
//! - No handlers - all DML logic inline
//! - RLS via user_id parameter in DML methods
//! - HOT-ONLY storage (RocksDB, no Parquet merging)
//! - TTL-based eviction in scan operations

use super::base::{BaseTableProvider, TableProviderCore};
use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::schema_registry::TableType;
use kalamdb_tables::{StreamTableRow, StreamTableStore};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemTable;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::ids::StreamTableRowId;
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

/// Stream table provider with RLS and TTL filtering
///
/// **Architecture**:
/// - Stateless provider (user context passed per-operation)
/// - Direct fields (no wrapper layer)
/// - Shared core via Arc<TableProviderCore>
/// - RLS enforced via user_id parameter
/// - HOT-ONLY storage (ephemeral data, no Parquet)
/// - TTL-based eviction
pub struct StreamTableProvider {
    /// Shared core (app_context, live_query_manager, storage_registry)
    core: Arc<TableProviderCore>,
    
    /// Table identifier
    table_id: Arc<TableId>,
    
    /// Logical table type
    table_type: TableType,
    
    /// StreamTableStore for DML operations (hot-only, in-memory)
    store: Arc<StreamTableStore>,
    
    /// TTL in seconds (optional)
    ttl_seconds: Option<u64>,
    
    /// Cached primary key field name
    primary_key_field_name: String,
}

impl StreamTableProvider {
    /// Create a new stream table provider
    ///
    /// # Arguments
    /// * `core` - Shared core with app_context and optional services
    /// * `table_id` - Table identifier
    /// * `store` - StreamTableStore for this table (hot-only)
    /// * `ttl_seconds` - Optional TTL for event eviction
    /// * `primary_key_field_name` - Primary key field name from schema
    pub fn new(
        core: Arc<TableProviderCore>,
        table_id: TableId,
        store: Arc<StreamTableStore>,
        ttl_seconds: Option<u64>,
        primary_key_field_name: String,
    ) -> Self {
        Self {
            core,
            table_id: Arc::new(table_id),
            table_type: TableType::Stream,
            store,
            ttl_seconds,
            primary_key_field_name,
        }
    }

    /// Return a snapshot of all rows as JSON objects (no RLS filtering)
    ///
    /// Used by live initial snapshot where we need table-wide data. RLS is
    /// enforced at the WebSocket layer so this method intentionally returns
    /// all rows for the table.
    pub fn snapshot_all_rows_json(&self) -> Result<Vec<JsonValue>, KalamDbError> {
        use kalamdb_store::entity_store::EntityStore;
        let rows = self
            .store
            .scan_all()
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to scan stream store: {}", e)))?;
        let count = rows.len();
        log::debug!(
            "[StreamProvider] snapshot_all_rows_json: table={}.{} rows={} ttl={:?}",
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str(),
            count,
            self.ttl_seconds
        );
        Ok(rows.into_iter().map(|(_k, r)| r.fields).collect())
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

impl BaseTableProvider<StreamTableRowId, StreamTableRow> for StreamTableProvider {
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
    
    fn insert(&self, user_id: &UserId, row_data: JsonValue) -> Result<StreamTableRowId, KalamDbError> {
        // Call SystemColumnsService to generate SeqId
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols.generate_seq_id()?;
        
        // Create StreamTableRow (no _deleted field for stream tables)
        let entity = StreamTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            fields: row_data,
        };
        
        // Create composite key
        let row_key = StreamTableRowId::new(user_id.clone(), seq_id);
        
        // Store in hot-only storage (RocksDB, no Parquet)
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!(
                "Failed to insert stream event: {}",
                e
            ))
        })?;
        
        log::debug!(
            "[StreamProvider] Inserted event: table={} seq={} user={}",
            format!("{}.{}", self.table_id.namespace_id().as_str(), self.table_id.table_name().as_str()),
            seq_id.as_i64(),
            user_id.as_str()
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
            // Flatten event fields and include user_id
            let mut obj = entity
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            obj.insert("user_id".to_string(), json!(user_id.as_str()));
            let row_json = JsonValue::Object(obj);

            let notification = ChangeNotification::insert(table_name.clone(), row_json);
            log::debug!(
                "[StreamProvider] Notifying change: table={} type=INSERT user={} seq={}",
                table_name,
                user_id.as_str(),
                seq_id.as_i64()
            );
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

        Ok(row_key)
    }
    
    fn update(&self, user_id: &UserId, _key: &StreamTableRowId, updates: JsonValue) -> Result<StreamTableRowId, KalamDbError> {
        // TODO: Implement full UPDATE logic for stream tables
        // 1. Scan ONLY RocksDB (hot storage, no Parquet)
        // 2. Find row by key (user-scoped)
        // 3. Merge updates
        // 4. Append new version
        
        // Placeholder: Just append as new version (incomplete implementation)
        self.insert(user_id, updates)
    }
    
    fn delete(&self, user_id: &UserId, key: &StreamTableRowId) -> Result<(), KalamDbError> {
        // TODO: Implement DELETE logic for stream tables
        // Stream tables may use hard delete or tombstone depending on requirements
        
        // Placeholder: Delete from hot storage directly
        self.store.delete(key).map_err(|e| {
            KalamDbError::InvalidOperation(format!(
                "Failed to delete stream event: {}",
                e
            ))
        })?;

        // Fire live query notification (DELETE hard)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = (*self.table_id).clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

            let row_id_str = format!("{}:{}", key.user_id().as_str(), key.seq().as_i64());
            let notification = ChangeNotification::delete_hard(table_name, row_id_str);
            manager.notify_table_change_async(user_id.clone(), table_id, notification);
        }

        Ok(())
    }
    
    fn scan_rows(&self, state: &dyn Session, filter: Option<&Expr>) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id from SessionState for RLS
        let (user_id, _role) = Self::extract_user_context(state)?;
        
        // Perform KV scan (hot-only) and convert to batch
        let kvs = self.scan_with_version_resolution_to_kvs(&user_id, filter)?;
        log::debug!(
            "[StreamProvider] scan_rows: table={} rows={} user={} ttl={:?}",
            format!("{}.{}", self.table_id.namespace_id().as_str(), self.table_id.table_name().as_str()),
            kvs.len(),
            user_id.as_str(),
            self.ttl_seconds
        );

        let schema = self.schema_ref();
        let mut rows: Vec<JsonValue> = Vec::with_capacity(kvs.len());
        let has_seq = schema.field_with_name("_seq").is_ok();
        let has_deleted = schema.field_with_name("_deleted").is_ok(); // likely false for streams
        let has_user = schema.field_with_name("user_id").is_ok();

        for (_key, row) in kvs.into_iter() {
            let mut obj = row
                .fields
                .as_object()
                .cloned()
                .unwrap_or_default();
            if has_seq { obj.insert("_seq".to_string(), json!(row._seq.as_i64())); }
            if has_deleted { obj.insert("_deleted".to_string(), json!(false)); }
            if has_user { obj.insert("user_id".to_string(), json!(row.user_id.as_str())); }
            rows.push(JsonValue::Object(obj));
        }

        let batch = json_rows_to_arrow_batch(&schema, rows)
            .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to build Arrow batch: {}", e)))?;
        Ok(batch)
    }
    
    fn scan_with_version_resolution_to_kvs(
        &self,
    user_id: &UserId,
    _filter: Option<&Expr>,
    ) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, KalamDbError> {
        // 1) Scan ONLY RocksDB (hot storage) with user_id prefix filter
        let user_bytes = user_id.as_str().as_bytes();
        let len = (user_bytes.len().min(255)) as u8;
        let mut prefix = Vec::with_capacity(1 + len as usize);
        prefix.push(len);
        prefix.extend_from_slice(&user_bytes[..len as usize]);

        let ttl_ms = self.ttl_seconds.map(|s| s * 1000);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| KalamDbError::InvalidOperation(format!("System time error: {}", e)))?
            .as_millis() as u64;
        log::debug!(
            "[StreamProvider] prefix scan: table={}.{} user={} prefix_len={} ttl_ms={:?} now_ms={}",
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str(),
            user_id.as_str(),
            prefix.len(),
            ttl_ms,
            now_ms
        );

        let raw = self
            .store
            .scan_limited_with_prefix_bytes(Some(&prefix), 100_000)
            .map_err(|e| KalamDbError::InvalidOperation(format!(
                "Failed to scan stream table hot storage: {}",
                e
            )))?;
        log::debug!(
            "[StreamProvider] raw scan results: table={} user={} count={}",
            format!("{}.{}", self.table_id.namespace_id().as_str(), self.table_id.table_name().as_str()),
            user_id.as_str(),
            raw.len()
        );

        // 2) TTL filtering (if configured)

        let mut results: Vec<(StreamTableRowId, StreamTableRow)> = Vec::new();
        for (key_bytes, row) in raw.into_iter() {
            // Parse typed key from bytes
            let maybe_key = kalamdb_commons::ids::StreamTableRowId::from_bytes(&key_bytes);
            let key = match maybe_key {
                Ok(k) => k,
                Err(err) => {
                    log::warn!("Skipping invalid StreamTableRowId key bytes: {}", err);
                    continue;
                }
            };

            // TTL check: keep if no TTL or not expired
            let keep = match ttl_ms {
                None => true,
                Some(ttl) => {
                    let ts = row._seq.timestamp_millis();
                    ts + ttl > now_ms
                }
            };

            if keep {
                results.push((key, row));
            }
        }

        log::debug!(
            "[StreamProvider] ttl-filtered results: table={}.{} user={} kept={}",
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str(),
            user_id.as_str(),
            results.len()
        );

        // TODO(phase 13.6): Apply filter expression for simple predicates if provided
        Ok(results)
    }
    
    fn extract_fields(row: &StreamTableRow) -> Option<&JsonValue> {
        Some(&row.fields)
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for StreamTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamTableProvider")
            .field("table_id", &self.table_id)
            .field("table_type", &self.table_type)
            .field("ttl_seconds", &self.ttl_seconds)
            .field("primary_key_field_name", &self.primary_key_field_name)
            .finish()
    }
}

// Implement DataFusion TableProvider trait
#[async_trait]
impl TableProvider for StreamTableProvider {
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
        log::debug!(
            "[StreamProvider] scan start: table={}.{} projection={:?} filters={} limit={:?}",
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str(),
            projection,
            filters.len(),
            limit
        );
        let batch = self
            .scan_rows(state, None)
            .map_err(|e| DataFusionError::Execution(format!("scan_rows failed: {}", e)))?;

        let mem = MemTable::try_new(self.schema_ref(), vec![vec![batch]])?;
        let plan = mem.scan(state, projection, filters, limit).await?;
        log::debug!(
            "[StreamProvider] scan planned: table={}.{}",
            self.table_id.namespace_id().as_str(),
            self.table_id.table_name().as_str()
        );
        Ok(plan)
    }
}
