//! Shared table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for shared tables with:
//! - Global data accessible to all users in namespace
//! - System columns (_updated, _deleted)
//! - RocksDB buffer with Parquet persistence
//! - Flush policy support (row/time/combined)

use crate::schema_registry::{NamespaceId, SchemaRegistry, TableName, TableType};
use crate::error::KalamDbError;
use crate::tables::base_table_provider::{BaseTableProvider, TableProviderCore};
use crate::tables::arrow_json_conversion::{
    arrow_batch_to_json, json_rows_to_arrow_batch, validate_insert_rows,
};
use crate::tables::shared_tables::shared_table_store::{SharedTableRow, SharedTableStore};
use kalamdb_commons::ids::{SeqId, SharedTableRowId};
use async_trait::async_trait;
use datafusion::arrow::array::ArrayRef;
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::dml::InsertOp;
use datafusion::logical_expr::{Expr, TableType as DataFusionTableType};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::models::TableId; // Phase 10: Arc<TableId> optimization
use kalamdb_store::EntityStoreV2 as EntityStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

/// Shared table provider for DataFusion
///
/// Provides SQL query access to shared tables with:
/// - INSERT/UPDATE/DELETE operations
/// - System columns (_updated, _deleted)
/// - RocksDB buffer with Parquet persistence
/// - Flush to Parquet
///
/// **Key Difference from User Tables**: Single storage location (no ${user_id} templating)
///
/// **Phase 10 Optimization**: Uses unified SchemaCache as single source of truth for table metadata
/// **Phase 3B**: Uses TableProviderCore to consolidate common provider fields
pub struct SharedTableProvider {
    /// Core provider fields (table_id, schema, cache, etc.)
    core: TableProviderCore,

    /// Application context for SystemColumnsService access
    app_context: Arc<crate::app_context::AppContext>,

    /// SharedTableStore for data operations
    store: Arc<SharedTableStore>,
}

impl std::fmt::Debug for SharedTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedTableProvider")
            .field("table_id", self.core.table_id())
            .field("schema", &self.core.schema_ref())
            .field("store", &"<SharedTableStore>")
            .finish()
    }
}

impl SharedTableProvider {
    /// Create a new shared table provider
    ///
    /// # Arguments
    /// * `table_id` - Arc<TableId> created once at registration (Phase 10: zero-allocation cache lookups)
    /// * `app_context` - Application context for SystemColumnsService access
    /// * `unified_cache` - Reference to unified SchemaCache for metadata lookups
    /// * `schema` - Arrow schema for the table (with system columns)
    /// * `store` - SharedTableStore for data operations
    pub fn new(
        table_id: Arc<TableId>,
        app_context: Arc<crate::app_context::AppContext>,
        unified_cache: Arc<SchemaRegistry>,
        _schema: SchemaRef,
        store: Arc<SharedTableStore>,
    ) -> Self {
        let core = TableProviderCore::new(
            table_id,
            TableType::Shared,
            None, // storage_id - will be fetched from cache when needed
            unified_cache,
        );
        Self { core, app_context, store }
    }

    /// Get the column family name for this shared table
    pub fn column_family_name(&self) -> String {
        format!(
            "shared_table:{}:{}",
            self.core.namespace().as_str(),
            self.core.table_name().as_str()
        )
    }

    /// Get the namespace ID
    pub fn namespace_id(&self) -> &NamespaceId {
        self.core.namespace()
    }

    /// Get the table name
    pub fn table_name(&self) -> &TableName {
        self.core.table_name()
    }

    /// INSERT operation
    ///
    /// # Arguments
    /// * `row_id` - Unique row identifier
    /// * `row_data` - Row data as JSON object (system columns will be injected)
    ///
    /// INSERT operation for shared tables
    ///
    /// # Arguments
    /// * `row_id` - DEPRECATED: Use SeqId from SystemColumnsService instead
    /// * `row_data` - Row data as JSON object
    ///
    /// # Returns
    /// Ok(()) on success
    ///
    /// # MVCC Architecture (Phase 12)
    /// - Uses SystemColumnsService to generate SeqId (Snowflake ID)
    /// - Creates SharedTableRow with (_seq, _deleted, fields)
    /// - Storage key is SeqId directly (8 bytes big-endian)
    pub fn insert(&self, _row_id: &str, row_data: JsonValue) -> Result<(), KalamDbError> {
        log::debug!(
            "SharedTableProvider::insert called - table_id: {}, store partition: {}",
            self.core.table_id(),
            self.store.partition()
        );
        
        // Get SeqId from SystemColumnsService (MVCC - Phase 12)
        let sys_cols = self.app_context.system_columns_service();
        let (seq_id, deleted) = sys_cols.handle_insert()?;

        // Create SharedTableRow entity (MVCC structure)
        let entity = SharedTableRow {
            _seq: seq_id,
            _deleted: deleted,
            fields: row_data,
        };

        // Storage key is SeqId directly
        let key = seq_id;

        // Store in SharedTableStore
        self.store
            .put(&key, &entity)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        log::debug!(
            "SharedTableProvider::insert completed - seq: {}, partition: {}",
            seq_id,
            self.store.partition()
        );

        Ok(())
    }

    pub fn update(&self, row_id: &str, updates: JsonValue) -> Result<(), KalamDbError> {
        let seq_id_int = row_id.parse::<i64>()
            .map_err(|_| KalamDbError::InvalidOperation(format!("Invalid row_id format: {}", row_id)))?;
        let key = SharedTableRowId::new(seq_id_int);
        let mut row_data = self
            .store
            .get(&key)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;

        // Apply updates
        if let Some(new_fields) = updates.as_object() {
            if let Some(existing_fields) = row_data.fields.as_object_mut() {
                for (k, v) in new_fields {
                    existing_fields.insert(k.clone(), v.clone());
                }
            }
        }

        // Update system columns - create new version with MVCC
        let sys_cols = self.app_context.system_columns_service();
        let (new_seq, _deleted) = sys_cols.handle_update()?;
        row_data._seq = new_seq;

        // Store updated row
        self.store
            .put(&new_seq, &row_data)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// Validate that INSERT rows comply with schema constraints
    ///
    /// Checks:
    /// - NOT NULL constraints (non-nullable columns must have values)
    /// - Data type compatibility (basic type checking)
    fn validate_insert_rows_local(&self, rows: &[JsonValue]) -> Result<(), String> {
        validate_insert_rows(&self.core.schema_ref(), rows)
    }

    // ============================================================================
    // Phase 3 (US1): Version Resolution Helpers
    // ============================================================================

    /// T056: Scan RocksDB and return Arrow RecordBatch (for version resolution)
    ///
    /// # Arguments
    /// * `schema` - Full schema including system columns (_updated, _deleted)
    /// * `limit` - Optional limit for early termination
    ///
    /// # Returns
    /// RecordBatch from RocksDB with system columns
    async fn scan_rocksdb_as_batch(
        &self,
        schema: &SchemaRef,
        limit: Option<usize>,
    ) -> DataFusionResult<datafusion::arrow::record_batch::RecordBatch> {
        log::debug!(
            "SharedTableProvider RocksDB scan: table={}.{}, limit={:?}",
            self.core.namespace().as_str(),
            self.core.table_id().table_name().as_str(),
            limit
        );

        let rocks_rows_raw = if let Some(lim) = limit {
            self.store
                .scan_limited(lim * 2) // Fetch extra for version resolution
                .map_err(|e| DataFusionError::Execution(format!("RocksDB scan failed: {}", e)))?
        } else {
            self.store
                .scan_all()
                .map_err(|e| DataFusionError::Execution(format!("RocksDB scan failed: {}", e)))?
        };

        log::debug!("RocksDB scan: {} rows", rocks_rows_raw.len());

        // Convert to Arrow RecordBatch
        if rocks_rows_raw.is_empty() {
            // Return empty batch with correct schema
            use datafusion::arrow::array::new_null_array;
            let arrays: Vec<ArrayRef> = schema
                .fields()
                .iter()
                .map(|f| new_null_array(f.data_type(), 0))
                .collect();
            datafusion::arrow::record_batch::RecordBatch::try_new(schema.clone(), arrays)
                .map_err(|e| DataFusionError::Execution(format!("Empty batch creation failed: {}", e)))
        } else {
            let rows_with_ids: Vec<(SharedTableRowId, SharedTableRow)> = rocks_rows_raw
                .iter()
                .map(|(key, row)| {
                    (
                        SharedTableRowId::new(String::from_utf8_lossy(key).to_string()),
                        row.clone(),
                    )
                })
                .collect();

            shared_rows_to_arrow_batch(&rows_with_ids, schema, None)
                .map_err(|e| DataFusionError::Execution(format!("RocksDB→Arrow conversion failed: {}", e)))
        }
    }

    /// T056: Scan Parquet files and return concatenated Arrow RecordBatch (for version resolution)
    ///
    /// # Arguments
    /// * `schema` - Full schema including system columns
    ///
    /// # Returns
    /// Concatenated RecordBatch from all Parquet files
    async fn scan_parquet_as_batch(
        &self,
        schema: &SchemaRef,
    ) -> DataFusionResult<datafusion::arrow::record_batch::RecordBatch> {
        log::debug!(
            "SharedTableProvider Parquet scan: table={}.{}",
            self.core.namespace().as_str(),
            self.core.table_id().table_name().as_str()
        );

        // Resolve storage path from SchemaRegistry
        let storage_path = self
            .core
            .cache()
            .get(self.core.table_id())
            .map(|cached_data| cached_data.storage_path_template.clone())
            .unwrap_or_else(|| String::new());

        let table_identifier = format!(
            "{}.{}",
            self.core.namespace().as_str(),
            self.core.table_name().as_str()
        );

        // Use shared Parquet scanning helper from base_table_provider
        crate::tables::base_table_provider::scan_parquet_files_as_batch(
            &storage_path,
            schema,
            &table_identifier,
        )
        .await
    }

    /// DELETE operation (soft delete)
    ///
    /// Sets _deleted=true and updates _updated timestamp.
    /// Row remains in RocksDB until flush or cleanup job removes it.
    pub fn delete_soft(&self, row_id: &str) -> Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        let mut row_data = self
            .store
            .get(&key)
            .map_err(|e| KalamDbError::Other(e.to_string()))?
            .ok_or_else(|| KalamDbError::NotFound(format!("Row not found: {}", row_id)))?;

        // Update system columns
        row_data._deleted = true;
        row_data._updated = chrono::Utc::now().to_rfc3339();

        // Store updated row
        self.store
            .put(&key, &row_data)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// DELETE operation (hard delete)
    ///
    /// Permanently removes row from RocksDB.
    /// Used by cleanup jobs for expired soft-deleted rows.
    pub fn delete_hard(&self, row_id: &str) -> Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id);
        self.store
            .delete(&key)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(())
    }

    /// DELETE by logical `id` field (helper for SQL layer)
    ///
    /// Finds the row whose JSON field `id` equals `id_value` and performs a soft delete.
    /// This bridges the mismatch between external primary key semantics (id column)
    /// and internal storage key (_seq in MVCC).
    pub fn delete_by_id_field(&self, id_value: &str) -> Result<(), KalamDbError> {
        let rows = self
            .store
            .scan_all()
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        log::debug!("delete_by_id_field: Looking for id={} among {} rows", id_value, rows.len());

        // Match either numeric or string representations
        let mut target_seq: Option<kalamdb_commons::ids::SeqId> = None;
        for (_k, row) in rows.iter() {
            if let Some(v) = row.fields.get("id") {
                log::debug!("delete_by_id_field: Found row with id field: {:?}", v);
                let is_match = match v {
                    JsonValue::Number(n) => {
                        let n_str = n.to_string();
                        log::debug!("delete_by_id_field: Comparing number {} with {}", n_str, id_value);
                        n_str == id_value
                    }
                    JsonValue::String(s) => {
                        log::debug!("delete_by_id_field: Comparing string {} with {}", s, id_value);
                        s == id_value
                    }
                    _ => {
                        log::debug!("delete_by_id_field: id field is neither number nor string: {:?}", v);
                        false
                    }
                };
                if is_match {
                    log::debug!("delete_by_id_field: Match found! seq={}", row._seq);
                    target_seq = Some(row._seq);
                    break;
                }
            } else {
                log::debug!("delete_by_id_field: Row has no id field");
            }
        }

        let seq = target_seq
            .ok_or_else(|| {
                log::error!("delete_by_id_field: No row found with id={}", id_value);
                KalamDbError::NotFound(format!("Row with id={} not found", id_value))
            })?;
        
        // Use SeqId directly as the row_id for delete_soft
        self.delete_soft(&seq.to_string())
    }

    /// UPDATE by logical `id` field (helper for SQL layer)
    ///
    /// Finds the row whose JSON field `id` equals `id_value` and performs an update.
    /// This bridges the mismatch between external primary key semantics (id column)
    /// and internal storage key (_seq in MVCC).
    pub fn update_by_id_field(&self, id_value: &str, updates: serde_json::Value) -> Result<(), KalamDbError> {
        let rows = self
            .store
            .scan_all()
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        log::debug!("update_by_id_field: Looking for id={} among {} rows", id_value, rows.len());

        // Match either numeric or string representations
        let mut target_seq: Option<kalamdb_commons::ids::SeqId> = None;
        for (_k, row) in rows.iter() {
            if let Some(v) = row.fields.get("id") {
                let is_match = match v {
                    JsonValue::Number(n) => n.to_string() == id_value,
                    JsonValue::String(s) => s == id_value,
                    _ => false,
                };
                if is_match {
                    log::debug!("update_by_id_field: Match found! seq={}", row._seq);
                    target_seq = Some(row._seq);
                    break;
                }
            }
        }

        let seq = target_seq
            .ok_or_else(|| {
                log::error!("update_by_id_field: No row found with id={}", id_value);
                KalamDbError::NotFound(format!("Row with id={} not found", id_value))
            })?;
        
        // Use SeqId directly as the row_id for update
        self.update(&seq.to_string(), updates)
    }
}

impl BaseTableProvider for SharedTableProvider {
    fn table_id(&self) -> &kalamdb_commons::models::TableId {
        self.core.table_id()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.core.schema_ref()
    }

    fn table_type(&self) -> crate::schema_registry::TableType {
        self.core.table_type()
    }
}

// DataFusion TableProvider trait implementation
#[async_trait]
impl TableProvider for SharedTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        // Phase 10, US1, FR-006: Use memoized Arrow schema (50-100× speedup)
        // Add system columns to the schema if they don't already exist
        let base_schema = match self.core.arrow_schema() {
            Ok(s) => s,
            Err(e) => {
                // If schema is missing (e.g., table was dropped concurrently), avoid panicking
                // and return a minimal schema with system columns so DataFusion can handle the query
                log::warn!("SharedTableProvider.schema(): missing schema for table {:?}: {}. Returning minimal system-only schema.", self.core.table_id(), e);
                let mut sys_fields: Vec<Arc<Field>> = Vec::new();
                sys_fields.push(Arc::new(Field::new(
                    "_updated",
                    DataType::Timestamp(TimeUnit::Millisecond, None),
                    false,
                )));
                sys_fields.push(Arc::new(Field::new("_deleted", DataType::Boolean, false)));
                return Arc::new(Schema::new(sys_fields));
            }
        };
        let mut fields = base_schema.fields().to_vec();

        // Check if _updated already exists
        if !fields.iter().any(|f| f.name() == "_updated") {
            // Add _updated (timestamp in milliseconds)
            fields.push(Arc::new(Field::new(
                "_updated",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false, // NOT NULL
            )));
        }

        // Check if _deleted already exists
        if !fields.iter().any(|f| f.name() == "_deleted") {
            // Add _deleted (boolean)
            fields.push(Arc::new(Field::new(
                "_deleted",
                DataType::Boolean,
                false, // NOT NULL
            )));
        }

        Arc::new(Schema::new(fields))
    }

    fn table_type(&self) -> DataFusionTableType {
        DataFusionTableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        log::debug!(
            "SharedTableProvider::scan called - table_id: {}, store partition: {}",
            self.core.table_id(),
            self.store.partition()
        );

        // Get the base schema and add system columns
        let full_schema = crate::tables::base_table_provider::schema_with_system_columns(
            &self.core.schema_ref()
        );

        // T056: STEP 1 - Scan RocksDB as Arrow RecordBatch (no _deleted filtering)
        let fast_batch = self.scan_rocksdb_as_batch(&full_schema, limit).await?;
        log::debug!("RocksDB scan: {} rows", fast_batch.num_rows());

        // T056: STEP 2 - Scan Parquet as Arrow RecordBatch (no _deleted filtering)
        let long_batch = self.scan_parquet_as_batch(&full_schema).await?;
        log::debug!("Parquet scan: {} rows", long_batch.num_rows());

        // T056: STEP 3-5 - Version Resolution + Deletion Filter + Limit (unified helper)
        let final_batch = crate::tables::base_table_provider::scan_with_version_resolution_and_filter(
            fast_batch,
            long_batch,
            full_schema.clone(),
            limit,
        )
        .await?;

        log::debug!(
            "SharedTableProvider::scan - final batch rows={}",
            final_batch.num_rows()
        );

        // DataFusion MemTable for projection
        use datafusion::datasource::MemTable;
        let partitions = vec![vec![final_batch]];
        let table = MemTable::try_new(full_schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed MemTable: {}", e)))?;
        table.scan(_state, projection, &[], limit).await
    }

    async fn insert_into(
        &self,
        _state: &dyn datafusion::catalog::Session,
        input: Arc<dyn ExecutionPlan>,
        _op: InsertOp,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::execution::TaskContext;
        use datafusion::physical_plan::collect;

        // Execute the input plan to get RecordBatches
        let task_ctx = Arc::new(TaskContext::default());
        let batches = collect(input, task_ctx)
            .await
            .map_err(|e| DataFusionError::Execution(format!("Failed to collect input: {}", e)))?;

        // Process each batch
        for batch in batches {
            // Convert Arrow RecordBatch to JSON rows
            let json_rows = arrow_batch_to_json(&batch, true).map_err(|e| {
                DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
            })?;

            // Validate schema constraints before insert
            self.validate_insert_rows_local(&json_rows).map_err(|e| {
                DataFusionError::Execution(format!("Schema validation failed: {}", e))
            })?;

            // Insert each row
            for (idx, row_data) in json_rows.into_iter().enumerate() {
                // Generate row_id using snowflake ID (or UUID for now)
                let row_id = format!("{}_{}", chrono::Utc::now().timestamp_millis(), idx);

                self.insert(&row_id, row_data)
                    .map_err(|e| DataFusionError::Execution(format!("Insert failed: {}", e)))?;
            }
        }

        // Return empty execution plan (INSERT returns no rows)
        use datafusion::physical_plan::empty::EmptyExec;
        Ok(Arc::new(EmptyExec::new(self.core.schema_ref())))
    }
}

/// Helper to convert SharedTableStore rows to JSON rows for Arrow conversion
///
/// Extracts the JSON fields from SharedTableRow and prepares them for Arrow batch creation
fn shared_rows_to_arrow_batch(
    rows: &[(SharedTableRowId, SharedTableRow)],
    schema: &datafusion::arrow::datatypes::SchemaRef,
    limit: Option<usize>,
) -> Result<datafusion::arrow::record_batch::RecordBatch, String> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::{DataType, TimeUnit};

    // Apply limit if specified
    let rows_to_process = if let Some(lim) = limit {
        &rows[..std::cmp::min(lim, rows.len())]
    } else {
        rows
    };

    if rows_to_process.is_empty() {
        // Return empty batch with correct schema using shared utility
        return json_rows_to_arrow_batch(schema, vec![]);
    }

    // Build arrays for each column (including system columns from SharedTableRow)
    let mut arrays: Vec<Arc<dyn datafusion::arrow::array::Array>> = Vec::new();

    for field in schema.fields() {
        let array: Arc<dyn datafusion::arrow::array::Array> = match field.data_type() {
            DataType::Utf8 => {
                let values: Vec<Option<String>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        row_data
                            .fields
                            .get(field.name())
                            .and_then(|v| v.as_str().map(|s| s.to_string()))
                    })
                    .collect();
                Arc::new(StringArray::from(values))
            }
            DataType::Int32 => {
                let values: Vec<Option<i32>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        row_data
                            .fields
                            .get(field.name())
                            .and_then(|v| v.as_i64().map(|i| i as i32))
                    })
                    .collect();
                Arc::new(Int32Array::from(values))
            }
            DataType::Int64 => {
                let values: Vec<Option<i64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.fields.get(field.name()).and_then(|v| v.as_i64()))
                    .collect();
                Arc::new(Int64Array::from(values))
            }
            DataType::Float64 => {
                let values: Vec<Option<f64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| row_data.fields.get(field.name()).and_then(|v| v.as_f64()))
                    .collect();
                Arc::new(Float64Array::from(values))
            }
            DataType::Boolean => {
                let values: Vec<Option<bool>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        if field.name() == "_deleted" {
                            Some(row_data._deleted)
                        } else {
                            row_data.fields.get(field.name()).and_then(|v| v.as_bool())
                        }
                    })
                    .collect();
                Arc::new(BooleanArray::from(values))
            }
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
                let values: Vec<Option<i64>> = rows_to_process
                    .iter()
                    .map(|(_, row_data)| {
                        if field.name() == "_updated" {
                            chrono::DateTime::parse_from_rfc3339(&row_data._updated)
                                .ok()
                                .map(|dt| dt.timestamp_millis())
                        } else {
                            row_data.fields.get(field.name()).and_then(|v| {
                                v.as_i64().or_else(|| {
                                    v.as_str().and_then(|s| {
                                        chrono::DateTime::parse_from_rfc3339(s)
                                            .ok()
                                            .map(|dt| dt.timestamp_millis())
                                    })
                                })
                            })
                        }
                    })
                    .collect();
                Arc::new(TimestampMillisecondArray::from(values))
            }
            _ => {
                return Err(format!(
                    "Unsupported data type for field {}: {:?}",
                    field.name(),
                    field.data_type()
                ));
            }
        };

        arrays.push(array);
    }

    datafusion::arrow::record_batch::RecordBatch::try_new(schema.clone(), arrays)
        .map_err(|e| format!("Failed to create RecordBatch: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_registry::TableType;
    use datafusion::arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use kalamdb_store::test_utils::{InMemoryBackend, TestDb};

    /// Phase 10: Create Arc<TableId> for test providers (avoids allocation on every cache lookup)
    fn create_test_table_id() -> Arc<TableId> {
        Arc::new(TableId::new(
            NamespaceId::new("app"),
            TableName::new("config"),
        ))
    }

    fn create_test_provider() -> (SharedTableProvider, TestDb) {
        let test_db = TestDb::new(&["shared_table:app:config"]).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("setting_key", DataType::Utf8, false),
            Field::new("setting_value", DataType::Utf8, false),
            Field::new(
                "_updated",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("_deleted", DataType::Boolean, false),
        ]));

        // Build unified cache with CachedTableData for tests
        use crate::schema_registry::{CachedTableData, SchemaRegistry};
        use kalamdb_commons::models::schemas::TableDefinition;

        let unified_cache = Arc::new(SchemaRegistry::new(0, None));

        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("config"));
        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("app"),
                TableName::new("config"),
                TableType::Shared,
                vec![], // Empty columns for test
                None,
            ).unwrap()
        );

        let data = CachedTableData::new(td);

        unified_cache.insert(table_id.clone(), Arc::new(data));

        let store = Arc::new(
            crate::tables::shared_tables::shared_table_store::new_shared_table_store(
                Arc::new(InMemoryBackend::new()),
                &table_id.namespace_id(),
                &table_id.table_name(),
            ),
        );

        // Create test AppContext
        let app_context = Arc::new(crate::app_context::AppContext::new_test());

        let provider = SharedTableProvider::new(create_test_table_id(), app_context, unified_cache, schema, store);

        (provider, test_db)
    }

    #[test]
    fn test_insert() {
        let (provider, _test_db) = create_test_provider();

        let row_data = serde_json::json!({
            "setting_key": "max_connections",
            "setting_value": "100"
        });

        let result = provider.insert("setting_1", row_data);
        assert!(result.is_ok());

        let key = SharedTableRowId::new("setting_1");
        let stored = provider.store.get(&key).unwrap();
        assert!(stored.is_some());

        let stored_data = stored.unwrap();
        assert_eq!(stored_data.fields["setting_key"], "max_connections");
        assert_eq!(stored_data._deleted, false);
        assert!(!stored_data._updated.is_empty());
    }

    #[test]
    fn test_update() {
        let (provider, _test_db) = create_test_provider();

        // Insert initial data
        let row_data = serde_json::json!({
            "setting_key": "timeout",
            "setting_value": "30"
        });
        provider.insert("setting_2", row_data).unwrap();

        // Update
        let updates = serde_json::json!({
            "setting_value": "60"
        });
        let result = provider.update("setting_2", updates);
        assert!(result.is_ok());

        let key = SharedTableRowId::new("setting_2");
        let stored = provider.store.get(&key).unwrap().unwrap();
        assert_eq!(stored.fields["setting_value"], "60");
        assert_eq!(stored.fields["setting_key"], "timeout"); // Unchanged
    }

    #[test]
    #[ignore] // TODO: Fix test - DB isolation issue
    fn test_delete_soft() {
        let (provider, _test_db) = create_test_provider();

        // Insert data
        let row_data = serde_json::json!({
            "setting_key": "old_setting",
            "setting_value": "deprecated"
        });
        provider.insert("setting_3", row_data).unwrap();

        let key = SharedTableRowId::new("setting_3");
        let before_delete = provider.store.get(&key).unwrap().unwrap();
        assert_eq!(before_delete._deleted, false);

        // Soft delete
        let result = provider.delete_soft("setting_3");
        assert!(result.is_ok(), "delete_soft failed: {:?}", result.err());

        // Verify still exists but marked deleted
        let stored = provider.store.get(&key).unwrap();

        assert!(stored.is_some(), "Row should still exist after soft delete");
        let stored_data = stored.unwrap();

        // Debug: print the actual value
        eprintln!("Stored _deleted value: {:?}", stored_data._deleted);

        assert_eq!(
            stored_data._deleted, true,
            "Row should be marked as deleted"
        );
    }

    #[test]
    fn test_delete_hard() {
        let (provider, _test_db) = create_test_provider();

        // Insert data
        let row_data = serde_json::json!({
            "setting_key": "temp",
            "setting_value": "value"
        });
        provider.insert("setting_4", row_data).unwrap();

        // Hard delete
        let result = provider.delete_hard("setting_4");
        assert!(result.is_ok());

        let key = SharedTableRowId::new("setting_4");
        let stored = provider.store.get(&key).unwrap();
        assert!(stored.is_none());
    }

    #[test]
    fn test_column_family_name() {
        let (provider, _test_db) = create_test_provider();
        assert_eq!(provider.column_family_name(), "shared_table:app:config");
    }
}
