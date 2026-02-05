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
//! - Commit log-backed storage (append-only, no Parquet)
//! - TTL-based eviction in scan operations

use crate::utils::base::{extract_seq_bounds_from_filter, BaseTableProvider, TableProviderCore};
use crate::utils::row_utils::extract_user_context;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use kalamdb_commons::schemas::TableType;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::catalog::Session;
use datafusion::datasource::TableProvider;
use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::constants::SystemColumnNames;
use kalamdb_commons::ids::{SeqId, StreamTableRowId};
use kalamdb_commons::models::UserId;
use kalamdb_commons::TableId;
use crate::stream_tables::{StreamTableRow, StreamTableStore};
use kalamdb_system::SchemaRegistry as SchemaRegistryTrait;
use std::any::Any;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// Arrow <-> JSON helpers
use kalamdb_commons::websocket::ChangeNotification;
use kalamdb_commons::models::rows::Row;

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

    /// StreamTableStore for DML operations (commit log-backed)
    store: Arc<StreamTableStore>,

    /// TTL in seconds (optional)
    ttl_seconds: Option<u64>,

    /// Cached primary key field name
    primary_key_field_name: String,

    /// Cached Arrow schema (prevents panics if table is dropped while provider is in use)
    schema: SchemaRef,

    /// Whether the Arrow schema includes a user_id column
    has_user_column: bool,

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
            .schema_registry
            .get_arrow_schema(core.table_id())
            .expect("Failed to get Arrow schema from registry during provider creation");
        let has_user_column = schema.field_with_name("user_id").is_ok();

        Self {
            core,
            store,
            ttl_seconds,
            primary_key_field_name,
            schema,
            has_user_column,
        }
    }

    /// Build a complete Row from StreamTableRow including system column (_seq)
    ///
    /// This ensures live query notifications include all columns, not just user-defined fields.
    /// Stream tables don't have _deleted column.
    fn build_notification_row(entity: &StreamTableRow) -> Row {
        let mut values = entity.fields.values.clone();
        values.insert(
            SystemColumnNames::SEQ.to_string(),
            ScalarValue::Int64(Some(entity._seq.as_i64())),
        );
        Row::new(values)
    }

    fn now_millis() -> Result<u64, KalamDbError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .into_invalid_operation("System time error")
            .map(|d| d.as_millis() as u64)
    }

    /// Expose the underlying store (used by maintenance jobs such as stream eviction)
    pub fn store_arc(&self) -> Arc<StreamTableStore> {
        self.store.clone()
    }
}

#[async_trait]
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

    fn cluster_coordinator(&self) -> &Arc<dyn kalamdb_system::ClusterCoordinator> {
        &self.core.cluster_coordinator
    }

    fn schema_registry(&self) -> &Arc<dyn SchemaRegistryTrait<Error = KalamDbError>> {
        &self.core.schema_registry
    }

    fn primary_key_field_name(&self) -> &str {
        &self.primary_key_field_name
    }

    fn core(&self) -> &crate::utils::base::TableProviderCore {
        &self.core
    }

    fn construct_row_from_parquet_data(
        &self,
        user_id: &UserId,
        row_data: &crate::utils::version_resolution::ParquetRowData,
    ) -> Result<Option<(StreamTableRowId, StreamTableRow)>, KalamDbError> {
        let row_key = StreamTableRowId::new(user_id.clone(), row_data.seq_id);
        let row = StreamTableRow {
            user_id: user_id.clone(),
            _seq: row_data.seq_id,
            fields: row_data.fields.clone(),
        };
        Ok(Some((row_key, row)))
    }

    /// Stream tables are append-only and don't support UPDATE/DELETE by PK.
    /// This always returns None - DML operations other than INSERT are not supported.
    fn find_row_key_by_id_field(
        &self,
        _user_id: &UserId,
        _id_value: &str,
    ) -> Result<Option<StreamTableRowId>, KalamDbError> {
        // Stream tables are append-only - no PK-based lookups for DML
        Ok(None)
    }

    fn insert(&self, user_id: &UserId, row_data: Row) -> Result<StreamTableRowId, KalamDbError> {
        let table_id = self.core.table_id();

        // Call SystemColumnsService to generate SeqId
        let sys_cols = self.core.system_columns.clone();
        let seq_id = sys_cols
            .generate_seq_id()
            .map_err(|e| KalamDbError::InvalidOperation(format!("SeqId generation failed: {}", e)))?;

        // Create StreamTableRow (no _deleted field for stream tables)
        let user_id = user_id.clone();
        let entity = StreamTableRow {
            user_id: user_id.clone(),
            _seq: seq_id,
            fields: row_data,
        };

        // Create composite key
        let row_key = StreamTableRowId::new(user_id.clone(), seq_id);

        // Store in commit log (append-only, no Parquet)
        self.store.put(&row_key, &entity).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to insert stream event: {}", e))
        })?;

        log::debug!(
            "[StreamProvider] Inserted event: table={} seq={} user={}",
            table_id,
            seq_id.as_i64(),
            user_id.as_str()
        );

        // Fire live query notification (INSERT)
        let manager = self.core.notification_service.clone();
        let table_id = self.core.table_id().clone();

        if manager.has_subscribers(Some(&user_id), &table_id) {
            let table_name = table_id.full_name();
            
            // Build complete row including system column (_seq)
            let row = Self::build_notification_row(&entity);
            
            let notification = ChangeNotification::insert(table_id.clone(), row);
            log::debug!(
                "[StreamProvider] Notifying change: table={} type=INSERT user={} seq={}",
                table_name,
                user_id.as_str(),
                seq_id.as_i64()
            );
            manager.notify_table_change(Some(user_id.clone()), table_id, notification);
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
        // 1. Scan in-memory hot storage (no Parquet)
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
        let notification_service = self.core.notification_service.clone();
        let table_id = self.core.table_id().clone();

        if notification_service.has_subscribers(Some(&user_id), &table_id) {
            let row_id_str = format!("{}:{}", key.user_id().as_str(), key.seq().as_i64());
            let notification = ChangeNotification::delete_hard(table_id.clone(), row_id_str);
            notification_service.notify_table_change(Some(user_id.clone()), table_id, notification);
        }

        Ok(())
    }

    fn delete_by_pk_value(&self, _user_id: &UserId, _pk_value: &str) -> Result<bool, KalamDbError> {
        // Stream tables are append-only - DELETE by PK is not supported
        // Return false indicating no row was deleted
        Ok(false)
    }

    async fn scan_rows(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filter: Option<&Expr>,
        limit: Option<usize>,
    ) -> Result<RecordBatch, KalamDbError> {
        // Extract user_id from SessionState for RLS
        let (user_id, _role) = extract_user_context(state)?;

        // Extract sequence bounds from filter to optimize scan
        let (since_seq, _until_seq) = if let Some(expr) = filter {
            extract_seq_bounds_from_filter(expr)
        } else {
            (None, None)
        };

        // Perform KV scan (hot-only) and convert to batch
        let keep_deleted = false; // Stream tables don't support soft delete yet
        let kvs = self.scan_with_version_resolution_to_kvs_async(
            user_id,
            filter,
            since_seq,
            limit,
            keep_deleted,
        ).await?;
        let table_id = self.core.table_id();
        log::debug!(
            "[StreamProvider] scan_rows: table={} rows={} user={} ttl={:?}",
            table_id,
            kvs.len(),
            user_id.as_str(),
            self.ttl_seconds
        );

        let schema = self.schema_ref();
        crate::utils::base::rows_to_arrow_batch(&schema, kvs, projection, |row_values, row| {
            if self.has_user_column {
                row_values.values.insert(
                    "user_id".to_string(),
                    ScalarValue::Utf8(Some(row.user_id.as_str().to_string())),
                );
            }
        })
    }

    async fn scan_with_version_resolution_to_kvs_async(
        &self,
        user_id: &UserId,
        _filter: Option<&Expr>,
        since_seq: Option<SeqId>,
        limit: Option<usize>,
        _keep_deleted: bool,
    ) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, KalamDbError> {
        let table_id = self.core.table_id();
        
        // since_seq is exclusive, so start at seq + 1
        let start_seq = since_seq.map(|seq| SeqId::from_i64(seq.as_i64().saturating_add(1)));

        let ttl_ms = self.ttl_seconds.map(|s| s * 1000);
        let now_ms = Self::now_millis()?;
        
        log::debug!(
            "[StreamProvider] streaming scan: table={} user={} ttl_ms={:?} limit={:?}",
            table_id,
            user_id.as_str(),
            ttl_ms,
            limit
        );

        // Use streaming scan with TTL filtering and early termination
        // This is more efficient than the pagination loop for LIMIT queries
        let scan_limit = limit.unwrap_or(100_000);
        
        // Use async version to avoid blocking the runtime
        let results = self.store
            .scan_user_streaming_async(user_id, start_seq, scan_limit, ttl_ms, now_ms)
            .await
            .map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to scan stream table hot storage: {}",
                    e
                ))
            })?;

        log::debug!(
            "[StreamProvider] streaming scan complete: table={} user={} rows={}",
            table_id,
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
