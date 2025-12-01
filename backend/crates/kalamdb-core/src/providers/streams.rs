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

use super::base::{extract_seq_bounds_from_filter, BaseTableProvider, TableProviderCore};
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
use datafusion::physical_plan::ExecutionPlan;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::ids::StreamTableRowId;
use kalamdb_commons::models::UserId;
use kalamdb_commons::{Role, StorageKey, TableId};
use kalamdb_store::entity_store::EntityStore;
use kalamdb_tables::{StreamTableRow, StreamTableStore};
use std::any::Any;
use std::sync::Arc;

// Arrow <-> JSON helpers
use crate::live_query::ChangeNotification;
use kalamdb_commons::models::Row;

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

    /// StreamTableStore for DML operations (hot-only, in-memory)
    store: Arc<StreamTableStore>,

    /// TTL in seconds (optional)
    ttl_seconds: Option<u64>,

    /// Cached primary key field name
    primary_key_field_name: String,

    /// Cached Arrow schema (prevents panics if table is dropped while provider is in use)
    schema: SchemaRef,
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
        store: Arc<StreamTableStore>,
        ttl_seconds: Option<u64>,
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
            ttl_seconds,
            primary_key_field_name,
            schema,
        }
    }

    /// Expose the underlying store (used by maintenance jobs such as stream eviction)
    pub fn store_arc(&self) -> Arc<StreamTableStore> {
        self.store.clone()
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

        Ok((user_ctx.user_id.clone(), user_ctx.role))
    }
}

impl BaseTableProvider<StreamTableRowId, StreamTableRow> for StreamTableProvider {
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

    fn insert(&self, user_id: &UserId, row_data: Row) -> Result<StreamTableRowId, KalamDbError> {
        let table_id = self.core.table_id();
        // Lazy TTL cleanup: delete expired rows BEFORE inserting new one
        // This ensures storage doesn't grow unbounded without background jobs
        if let Some(ttl_seconds) = self.ttl_seconds {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let ttl_ms = ttl_seconds * 1000;
            let cutoff_ms = now_ms.saturating_sub(ttl_ms);

            // Scan for expired rows (limit to 100 per insert to avoid blocking)
            let all_rows = self.store.scan_all(Some(100), None, None).map_err(|e| {
                KalamDbError::InvalidOperation(format!("Failed to scan for TTL cleanup: {}", e))
            })?;

            let mut deleted_count = 0;
            for (key_bytes, row) in all_rows.iter() {
                let row_ts = row._seq.timestamp_millis();
                if row_ts < cutoff_ms {
                    // Parse key from bytes
                    if let Ok(row_key) = StreamTableRowId::from_bytes(key_bytes) {
                        if let Err(e) = self.store.delete(&row_key) {
                            log::warn!("Failed to delete expired row during TTL cleanup: {}", e);
                        } else {
                            deleted_count += 1;
                        }
                    }
                }
            }

            if deleted_count > 0 {
                log::debug!(
                    "[StreamProvider] TTL cleanup: table={}.{} deleted={} expired rows",
                    table_id.namespace_id().as_str(),
                    table_id.table_name().as_str(),
                    deleted_count
                );
            }
        }

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
            KalamDbError::InvalidOperation(format!("Failed to insert stream event: {}", e))
        })?;

        log::debug!(
            "[StreamProvider] Inserted event: table={}.{} seq={} user={}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            seq_id.as_i64(),
            user_id.as_str()
        );

        // Fire live query notification (INSERT)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();
            let table_name = format!(
                "{}.{}",
                table_id.namespace_id().as_str(),
                table_id.table_name().as_str()
            );

            // Flatten event fields (user_id injected by manager)
            let obj = entity.fields.values.clone();
            let row = Row::new(obj);

            let notification = ChangeNotification::insert(table_id.clone(), row);
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

    fn update(
        &self,
        user_id: &UserId,
        _key: &StreamTableRowId,
        updates: Row,
    ) -> Result<StreamTableRowId, KalamDbError> {
        // TODO: Implement full UPDATE logic for stream tables
        // 1. Scan ONLY RocksDB (hot storage, no Parquet)
        // 2. Find row by key (user-scoped)
        // 3. Merge updates
        // 4. Append new version

        // Placeholder: Just append as new version (incomplete implementation)
        self.insert(user_id, updates)
    }

    fn update_by_pk_value(
        &self,
        user_id: &UserId,
        _pk_value: &str,
        updates: Row,
    ) -> Result<StreamTableRowId, KalamDbError> {
        // TODO: Implement full UPDATE logic for stream tables
        // Stream tables are typically append-only, so UPDATE just inserts a new event
        self.insert(user_id, updates)
    }

    fn delete(&self, user_id: &UserId, key: &StreamTableRowId) -> Result<(), KalamDbError> {
        // TODO: Implement DELETE logic for stream tables
        // Stream tables may use hard delete or tombstone depending on requirements

        // Placeholder: Delete from hot storage directly
        self.store.delete(key).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to delete stream event: {}", e))
        })?;

        // Fire live query notification (DELETE hard)
        if let Some(manager) = &self.core.live_query_manager {
            let table_id = self.core.table_id().clone();

            let row_id_str = format!("{}:{}", key.user_id().as_str(), key.seq().as_i64());
            let notification = ChangeNotification::delete_hard(table_id.clone(), row_id_str);
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
        // Extract user_id from SessionState for RLS
        let (user_id, _role) = Self::extract_user_context(state)?;

        // Extract sequence bounds from filter to optimize RocksDB scan
        let (since_seq, _until_seq) = if let Some(expr) = filter {
            extract_seq_bounds_from_filter(expr)
        } else {
            (None, None)
        };

        // Perform KV scan (hot-only) and convert to batch
        let keep_deleted = false; // Stream tables don't support soft delete yet
        let kvs = self.scan_with_version_resolution_to_kvs(
            &user_id,
            filter,
            since_seq,
            limit,
            keep_deleted,
        )?;
        let table_id = self.core.table_id();
        log::debug!(
            "[StreamProvider] scan_rows: table={}.{} rows={} user={} ttl={:?}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            kvs.len(),
            user_id.as_str(),
            self.ttl_seconds
        );

        let schema = self.schema_ref();
        let has_user = schema.field_with_name("user_id").is_ok();
        crate::providers::base::rows_to_arrow_batch(&schema, kvs, projection, |row_values, row| {
            if has_user {
                row_values.values.insert(
                    "user_id".to_string(),
                    ScalarValue::Utf8(Some(row.user_id.as_str().to_string())),
                );
            }
        })
    }

    fn scan_with_version_resolution_to_kvs(
        &self,
        user_id: &UserId,
        _filter: Option<&Expr>,
        since_seq: Option<kalamdb_commons::ids::SeqId>,
        limit: Option<usize>,
        _keep_deleted: bool,
    ) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, KalamDbError> {
        let table_id = self.core.table_id();
        // 1) Scan ONLY RocksDB (hot storage) with user_id prefix filter
        let user_bytes = user_id.as_str().as_bytes();
        let len = (user_bytes.len().min(255)) as u8;
        let mut prefix = Vec::with_capacity(1 + len as usize);
        prefix.push(len);
        prefix.extend_from_slice(&user_bytes[..len as usize]);

        // Construct start_key if since_seq is provided
        let start_key_bytes = if let Some(seq) = since_seq {
            // since_seq is exclusive, so start at seq + 1
            let start_seq = kalamdb_commons::ids::SeqId::from(seq.as_i64() + 1);
            let key = StreamTableRowId::new(user_id.clone(), start_seq);
            Some(key.storage_key())
        } else {
            None
        };

        let ttl_ms = self.ttl_seconds.map(|s| s * 1000);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| KalamDbError::InvalidOperation(format!("System time error: {}", e)))?
            .as_millis() as u64;
        log::debug!(
            "[StreamProvider] prefix scan: table={}.{} user={} prefix_len={} ttl_ms={:?} now_ms={}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            user_id.as_str(),
            prefix.len(),
            ttl_ms,
            now_ms
        );

        // Use limit if provided, otherwise default to 100,000
        let scan_limit = limit.map(|l| std::cmp::max(l * 2, 1000)).unwrap_or(100_000);

        let raw = self
            .store
            .scan_limited_with_prefix_and_start(
                Some(&prefix),
                start_key_bytes.as_deref(),
                scan_limit,
            )
            .map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to scan stream table hot storage: {}",
                    e
                ))
            })?;
        log::debug!(
            "[StreamProvider] raw scan results: table={}.{} user={} count={}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
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

        // Apply limit after TTL filtering
        if let Some(l) = limit {
            if results.len() > l {
                results.truncate(l);
            }
        }

        log::debug!(
            "[StreamProvider] ttl-filtered results: table={}.{} user={} kept={}",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            user_id.as_str(),
            results.len()
        );

        // TODO(phase 13.6): Apply filter expression for simple predicates if provided
        Ok(results)
    }

    fn extract_row(row: &StreamTableRow) -> &Row {
        &row.fields
    }
}

// Manual Debug to satisfy DataFusion's TableProvider: Debug bound
impl std::fmt::Debug for StreamTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let table_id = self.core.table_id_arc();
        f.debug_struct("StreamTableProvider")
            .field("table_id", &table_id)
            .field("table_type", &self.core.table_type())
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
