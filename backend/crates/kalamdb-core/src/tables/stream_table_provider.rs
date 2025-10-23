//! Stream table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for stream tables with:
//! - Ephemeral event storage (memory/RocksDB only, NO Parquet)
//! - Timestamp-prefixed keys for efficient TTL eviction
//! - NO system columns (_updated, _deleted)
//! - Optional ephemeral mode (only store if subscribers exist)
//! - Real-time event delivery to subscribers

use crate::catalog::{NamespaceId, TableMetadata, TableName};
use crate::error::KalamDbError;
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::tables::system::LiveQueriesTableProvider;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::datasource::TableProvider;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::{Expr, TableType as DataFusionTableType};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_store::StreamTableStore;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::sync::Arc;

/// Stream table provider for DataFusion
///
/// Provides SQL query access to stream tables with:
/// - Event insertion with automatic timestamps
/// - Ephemeral storage (memory/RocksDB only)
/// - TTL-based eviction support
/// - Real-time event delivery
///
/// **Important**: Stream tables do NOT have system columns (_updated, _deleted)
pub struct StreamTableProvider {
    /// Table metadata (namespace, table name, type, etc.)
    table_metadata: TableMetadata,

    /// Arrow schema for the table (user-defined only, no system columns)
    schema: SchemaRef,

    /// StreamTableStore for event operations
    store: Arc<StreamTableStore>,

    /// Retention period in seconds (for TTL eviction)
    retention_seconds: Option<u32>,

    /// Ephemeral mode - only store if subscribers exist
    ephemeral: bool,

    /// Maximum buffer size (number of events to keep)
    max_buffer: Option<usize>,

    /// Optional live queries provider for ephemeral mode checks
    live_queries: Option<Arc<LiveQueriesTableProvider>>,

    /// Optional live query manager for real-time notifications (T154)
    live_query_manager: Option<Arc<LiveQueryManager>>,

    /// Counter for discarded events (ephemeral mode, no subscribers)
    discarded_count: std::sync::atomic::AtomicU64,
}

impl StreamTableProvider {
    /// Create a new stream table provider
    ///
    /// # Arguments
    /// * `table_metadata` - Table metadata (namespace, table name, type, etc.)
    /// * `schema` - Arrow schema for the table (NO system columns)
    /// * `store` - StreamTableStore for event operations
    /// * `retention_seconds` - Optional TTL in seconds
    /// * `ephemeral` - If true, only store events if subscribers exist
    /// * `max_buffer` - Optional maximum buffer size
    pub fn new(
        table_metadata: TableMetadata,
        schema: SchemaRef,
        store: Arc<StreamTableStore>,
        retention_seconds: Option<u32>,
        ephemeral: bool,
        max_buffer: Option<usize>,
    ) -> Self {
        Self {
            table_metadata,
            schema,
            store,
            retention_seconds,
            ephemeral,
            max_buffer,
            live_queries: None,
            live_query_manager: None,
            discarded_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Set the live queries provider for ephemeral mode checks
    pub fn with_live_queries(mut self, live_queries: Arc<LiveQueriesTableProvider>) -> Self {
        self.live_queries = Some(live_queries);
        self
    }

    /// Set the live query manager for real-time notifications (T154)
    pub fn with_live_query_manager(mut self, live_query_manager: Arc<LiveQueryManager>) -> Self {
        self.live_query_manager = Some(live_query_manager);
        self
    }

    /// Get the number of events discarded due to ephemeral mode (no subscribers)
    pub fn discarded_count(&self) -> u64 {
        self.discarded_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the column family name for this stream table
    pub fn column_family_name(&self) -> String {
        format!(
            "stream_table:{}:{}",
            self.table_metadata.namespace.as_str(),
            self.table_metadata.table_name.as_str()
        )
    }

    /// Get the namespace ID
    pub fn namespace_id(&self) -> &NamespaceId {
        &self.table_metadata.namespace
    }

    /// Get the table name
    pub fn table_name(&self) -> &TableName {
        &self.table_metadata.table_name
    }

    /// Check if ephemeral mode is enabled
    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    /// Get retention period in seconds
    pub fn retention_seconds(&self) -> Option<u32> {
        self.retention_seconds
    }

    /// Get max buffer size
    pub fn max_buffer(&self) -> Option<usize> {
        self.max_buffer
    }

    /// Insert an event into the stream table
    ///
    /// # Arguments
    /// * `row_id` - Event identifier
    /// * `row_data` - Event data as JSON object (NO system columns)
    ///
    /// # Returns
    /// The timestamp (ms) used for the event key
    ///
    /// # Implementation Notes
    /// - Key format: {timestamp_ms}:{row_id}
    /// - NO system columns added (_updated, _deleted)
    /// - If ephemeral mode: check for subscribers first (T151 ✅)
    /// - After insert: notify live query manager (TODO: T154)
    pub fn insert_event(&self, row_id: &str, row_data: JsonValue) -> Result<i64, KalamDbError> {
        // T151: Ephemeral mode check - only store if subscribers exist
        if self.ephemeral {
            if let Some(live_queries) = &self.live_queries {
                // Query for active subscribers on this table
                let subscribers = live_queries
                    .get_by_table_name(self.table_name().as_str())
                    .map_err(|e| {
                        KalamDbError::Other(format!("Failed to check subscribers: {}", e))
                    })?;

                // If no subscribers, discard event immediately
                if subscribers.is_empty() {
                    self.discarded_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    // Log for monitoring (in production, use proper logging)
                    #[cfg(debug_assertions)]
                    eprintln!(
                        "Stream table {}: Discarded event {} (ephemeral mode, no subscribers)",
                        self.table_name().as_str(),
                        row_id
                    );

                    // Return timestamp but don't persist
                    return Ok(chrono::Utc::now().timestamp_millis());
                }
            } else {
                // Ephemeral mode enabled but no live queries provider
                // Fall through to normal insert (conservative behavior)
                #[cfg(debug_assertions)]
                eprintln!(
                    "Warning: Stream table {} has ephemeral=true but no LiveQueriesProvider set",
                    self.table_name().as_str()
                );
            }
        }

        // Generate timestamp for the event
        let timestamp_ms = chrono::Utc::now().timestamp_millis();

        // Insert event into StreamTableStore
        self.store
            .put(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                timestamp_ms,
                row_id,
                row_data.clone(),
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // T154: Notify live query manager with change notification
        if let Some(live_query_manager) = &self.live_query_manager {
            let notification = ChangeNotification::insert(
                self.table_name().as_str().to_string(),
                row_data.clone(),
            );

            // Deliver notification asynchronously (spawn task to avoid blocking)
            let manager = Arc::clone(live_query_manager);
            let table_name = self.table_name().as_str().to_string();
            tokio::spawn(async move {
                if let Err(e) = manager.notify_table_change(&table_name, notification).await {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to notify subscribers: {}", e);
                }
            });
        }

        Ok(timestamp_ms)
    }

    /// Insert multiple events into the stream table
    ///
    /// # Arguments
    /// * `events` - Vector of (row_id, row_data) tuples
    ///
    /// # Returns
    /// Vector of timestamps used for event keys
    pub fn insert_batch(&self, events: Vec<(String, JsonValue)>) -> Result<Vec<i64>, KalamDbError> {
        let mut timestamps = Vec::with_capacity(events.len());

        for (row_id, row_data) in events {
            let timestamp = self.insert_event(&row_id, row_data)?;
            timestamps.push(timestamp);
        }

        Ok(timestamps)
    }

    /// Get an event by timestamp and row ID
    ///
    /// # Arguments
    /// * `timestamp_ms` - Event timestamp in milliseconds
    /// * `row_id` - Event identifier
    ///
    /// # Returns
    /// Event data if found, None otherwise
    pub fn get_event(
        &self,
        timestamp_ms: i64,
        row_id: &str,
    ) -> Result<Option<JsonValue>, KalamDbError> {
        self.store
            .get(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                timestamp_ms,
                row_id,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))
    }

    /// Scan all events in the stream table
    ///
    /// # Returns
    /// Vector of (timestamp_ms, row_id, row_data) tuples
    pub fn scan_events(&self) -> Result<Vec<(i64, String, JsonValue)>, KalamDbError> {
        self.store
            .scan(self.namespace_id().as_str(), self.table_name().as_str())
            .map_err(|e| KalamDbError::Other(e.to_string()))
    }

    /// Count events in the stream table
    ///
    /// Used for max_buffer eviction checks
    pub fn count_events(&self) -> Result<usize, KalamDbError> {
        let events = self.scan_events()?;
        Ok(events.len())
    }

    /// Evict events older than the retention period
    ///
    /// # Arguments
    /// * `cutoff_timestamp_ms` - Delete all events before this timestamp
    ///
    /// # Returns
    /// Number of events deleted
    pub fn evict_older_than(&self, cutoff_timestamp_ms: i64) -> Result<usize, KalamDbError> {
        self.store
            .evict_older_than(
                self.namespace_id().as_str(),
                self.table_name().as_str(),
                cutoff_timestamp_ms,
            )
            .map_err(|e| KalamDbError::Other(e.to_string()))
    }
}

/// Implement DataFusion TableProvider trait
#[async_trait]
impl TableProvider for StreamTableProvider {
    /// Returns self as Any for downcasting
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Returns the Arrow schema for this table (NO system columns)
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    /// Returns the table type (always Base for stream tables)
    fn table_type(&self) -> DataFusionTableType {
        DataFusionTableType::Base
    }

    /// Create an execution plan for scanning this stream table
    ///
    /// # Implementation Notes
    /// - Stream tables are memory/RocksDB only (NO Parquet scans)
    /// - Returns all events in timestamp order
    /// - Filters are NOT pushed down (simple scan for now)
    async fn scan(
        &self,
        _state: &SessionState,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        // TODO: Implement actual scan execution plan
        // For now, return error indicating not yet implemented
        Err(DataFusionError::NotImplemented(
            "Stream table scan not yet implemented".to_string(),
        ))
    }

    /// Insert data into the stream table
    ///
    /// Implements DataFusion's insert_into trait method for stream tables.
    /// Converts Arrow RecordBatch to JSON events and calls insert_event().
    ///
    /// **Important differences from user tables**:
    /// - NO system columns (_updated, _deleted)
    /// - Uses timestamp-based keys for TTL eviction
    /// - Supports ephemeral mode (discard if no subscribers)
    /// - Triggers real-time notifications to subscribers
    async fn insert_into(
        &self,
        _state: &SessionState,
        input: Arc<dyn ExecutionPlan>,
        _overwrite: bool,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::execution::TaskContext;
        use datafusion::physical_plan::collect;

        // Execute the input plan to get RecordBatches
        let task_ctx = Arc::new(TaskContext::default());
        let batches = collect(input, task_ctx)
            .await
            .map_err(|e| DataFusionError::Execution(format!("Failed to collect input: {}", e)))?;

        let mut total_inserted = 0;

        // Process each batch
        for batch in batches {
            // Convert Arrow RecordBatch to JSON rows
            let json_rows = arrow_batch_to_json(&batch).map_err(|e| {
                DataFusionError::Execution(format!("Arrow to JSON conversion failed: {}", e))
            })?;

            // Insert each event
            for (idx, row_data) in json_rows.into_iter().enumerate() {
                // Generate row_id (use index for now, could use a UUID or snowflake ID)
                let row_id = format!("event_{}", idx);

                // Insert event (handles ephemeral mode and notifications internally)
                self.insert_event(&row_id, row_data)
                    .map_err(|e| DataFusionError::Execution(format!("Insert failed: {}", e)))?;

                total_inserted += 1;
            }
        }

        // Create a result batch with the count
        use datafusion::arrow::array::UInt64Array;
        use datafusion::arrow::datatypes::{DataType, Field, Schema};
        use datafusion::arrow::record_batch::RecordBatch;

        let schema = Arc::new(Schema::new(vec![Field::new(
            "count",
            DataType::UInt64,
            false,
        )]));
        let count_array = UInt64Array::from(vec![total_inserted]);
        let result_batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(count_array)])
            .map_err(|e| {
                DataFusionError::Execution(format!("Failed to create result batch: {}", e))
            })?;

        // Return a MemoryExec with the result
        use datafusion::physical_plan::memory::MemoryExec;
        let partitions = vec![vec![result_batch]];
        let exec = MemoryExec::try_new(&partitions, schema, None).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemoryExec: {}", e))
        })?;

        Ok(Arc::new(exec))
    }
}

/// Helper function to convert Arrow RecordBatch to JSON rows
///
/// Converts each row in the batch to a JSON object (HashMap).
/// Similar to user_table_provider but WITHOUT system columns.
fn arrow_batch_to_json(
    batch: &datafusion::arrow::record_batch::RecordBatch,
) -> Result<Vec<JsonValue>, KalamDbError> {
    use datafusion::arrow::array::*;
    use datafusion::arrow::datatypes::DataType;
    use serde_json::Map;

    let schema = batch.schema();
    let num_rows = batch.num_rows();
    let mut rows = Vec::with_capacity(num_rows);

    for row_idx in 0..num_rows {
        let mut row_map = Map::new();

        for (col_idx, field) in schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);
            let col_name = field.name().clone();

            let value = match field.data_type() {
                DataType::Utf8 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .ok_or_else(|| {
                            KalamDbError::Other(format!(
                                "Expected StringArray for column {}",
                                col_name
                            ))
                        })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::String(array.value(row_idx).to_string())
                    }
                }
                DataType::Int32 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Int32Array>()
                        .ok_or_else(|| {
                            KalamDbError::Other(format!(
                                "Expected Int32Array for column {}",
                                col_name
                            ))
                        })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                DataType::Int64 => {
                    let array = column
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .ok_or_else(|| {
                            KalamDbError::Other(format!(
                                "Expected Int64Array for column {}",
                                col_name
                            ))
                        })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Number(array.value(row_idx).into())
                    }
                }
                DataType::Float64 => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<Float64Array>()
                            .ok_or_else(|| {
                                KalamDbError::Other(format!(
                                    "Expected Float64Array for column {}",
                                    col_name
                                ))
                            })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        serde_json::Number::from_f64(array.value(row_idx))
                            .map(JsonValue::Number)
                            .unwrap_or(JsonValue::Null)
                    }
                }
                DataType::Boolean => {
                    let array =
                        column
                            .as_any()
                            .downcast_ref::<BooleanArray>()
                            .ok_or_else(|| {
                                KalamDbError::Other(format!(
                                    "Expected BooleanArray for column {}",
                                    col_name
                                ))
                            })?;
                    if array.is_null(row_idx) {
                        JsonValue::Null
                    } else {
                        JsonValue::Bool(array.value(row_idx))
                    }
                }
                DataType::Timestamp(unit, _) => {
                    use datafusion::arrow::datatypes::TimeUnit;

                    // Handle different timestamp types
                    let value_ms = match unit {
                        TimeUnit::Second => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampSecondArray>()
                                .ok_or_else(|| {
                                    KalamDbError::Other(format!(
                                        "Expected TimestampSecondArray for column {}",
                                        col_name
                                    ))
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number((array.value(row_idx) * 1000).into())
                            }
                        }
                        TimeUnit::Millisecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampMillisecondArray>()
                                .ok_or_else(|| {
                                    KalamDbError::Other(format!(
                                        "Expected TimestampMillisecondArray for column {}",
                                        col_name
                                    ))
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number(array.value(row_idx).into())
                            }
                        }
                        TimeUnit::Microsecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampMicrosecondArray>()
                                .ok_or_else(|| {
                                    KalamDbError::Other(format!(
                                        "Expected TimestampMicrosecondArray for column {}",
                                        col_name
                                    ))
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number((array.value(row_idx) / 1000).into())
                            }
                        }
                        TimeUnit::Nanosecond => {
                            let array = column
                                .as_any()
                                .downcast_ref::<TimestampNanosecondArray>()
                                .ok_or_else(|| {
                                    KalamDbError::Other(format!(
                                        "Expected TimestampNanosecondArray for column {}",
                                        col_name
                                    ))
                                })?;
                            if array.is_null(row_idx) {
                                JsonValue::Null
                            } else {
                                JsonValue::Number((array.value(row_idx) / 1_000_000).into())
                            }
                        }
                    };

                    value_ms
                }
                _ => {
                    return Err(KalamDbError::Other(format!(
                        "Unsupported data type {:?} for column {}",
                        field.data_type(),
                        col_name
                    )));
                }
            };

            row_map.insert(col_name, value);
        }

        rows.push(JsonValue::Object(row_map));
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableType;
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use kalamdb_store::test_utils::TestDb;
    use serde_json::json;

    fn create_test_provider() -> (StreamTableProvider, TestDb) {
        let test_db = TestDb::new(&["stream_app:events"]).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
            Field::new("data", DataType::Utf8, true),
        ]));

        let table_metadata = TableMetadata {
            table_name: TableName::new("events"),
            table_type: TableType::Stream,
            namespace: NamespaceId::new("app"),
            created_at: chrono::Utc::now(),
            storage_location: String::new(), // Stream tables don't use Parquet
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            schema_version: 1,
            deleted_retention_hours: None,
        };

        let store = Arc::new(StreamTableStore::new(test_db.db.clone()).unwrap());

        let provider = StreamTableProvider::new(
            table_metadata,
            schema,
            store,
            Some(300),   // 5 minute retention
            false,       // not ephemeral
            Some(10000), // max 10k events
        );

        (provider, test_db)
    }

    #[test]
    fn test_insert_event() {
        let (provider, _test_db) = create_test_provider();

        let event_data = json!({
            "id": 1,
            "event_type": "click",
            "data": "button_clicked"
        });

        let result = provider.insert_event("evt001", event_data);
        assert!(result.is_ok());

        let timestamp = result.unwrap();
        assert!(timestamp > 0);

        // Verify event was inserted
        let retrieved = provider.get_event(timestamp, "evt001").unwrap();
        assert!(retrieved.is_some());

        let data = retrieved.unwrap();
        assert_eq!(data["event_type"], "click");
    }

    #[test]
    fn test_insert_batch() {
        let (provider, _test_db) = create_test_provider();

        let events = vec![
            (
                "evt001".to_string(),
                json!({"id": 1, "event_type": "click", "data": "button1"}),
            ),
            (
                "evt002".to_string(),
                json!({"id": 2, "event_type": "view", "data": "page1"}),
            ),
            (
                "evt003".to_string(),
                json!({"id": 3, "event_type": "submit", "data": "form1"}),
            ),
        ];

        let result = provider.insert_batch(events);
        assert!(result.is_ok());

        let timestamps = result.unwrap();
        assert_eq!(timestamps.len(), 3);

        // Verify all events were inserted
        let all_events = provider.scan_events().unwrap();
        assert_eq!(all_events.len(), 3);
    }

    #[test]
    fn test_scan_events() {
        let (provider, _test_db) = create_test_provider();

        // Insert test events
        provider
            .insert_event("evt001", json!({"id": 1, "event_type": "click"}))
            .unwrap();
        provider
            .insert_event("evt002", json!({"id": 2, "event_type": "view"}))
            .unwrap();

        let events = provider.scan_events().unwrap();
        assert_eq!(events.len(), 2);

        // Events should be in timestamp order
        assert_eq!(events[0].1, "evt001");
        assert_eq!(events[1].1, "evt002");
    }

    #[test]
    fn test_count_events() {
        let (provider, _test_db) = create_test_provider();

        // Initially empty
        assert_eq!(provider.count_events().unwrap(), 0);

        // Insert events
        provider.insert_event("evt001", json!({"id": 1})).unwrap();
        provider.insert_event("evt002", json!({"id": 2})).unwrap();
        provider.insert_event("evt003", json!({"id": 3})).unwrap();

        assert_eq!(provider.count_events().unwrap(), 3);
    }

    #[test]
    fn test_evict_older_than() {
        let (provider, _test_db) = create_test_provider();

        // Insert events with known timestamps
        let _ts1 = provider.insert_event("evt001", json!({"id": 1})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let ts2 = provider.insert_event("evt002", json!({"id": 2})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        provider.insert_event("evt003", json!({"id": 3})).unwrap();

        // Evict events older than ts2
        let deleted = provider.evict_older_than(ts2).unwrap();
        assert_eq!(deleted, 1); // Only evt001 should be deleted

        // Verify remaining events
        let events = provider.scan_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].1, "evt002");
        assert_eq!(events[1].1, "evt003");
    }

    #[test]
    fn test_no_system_columns() {
        let (provider, _test_db) = create_test_provider();

        let event_data = json!({
            "id": 1,
            "event_type": "click"
        });

        let timestamp = provider.insert_event("evt001", event_data).unwrap();

        // Retrieve and verify no system columns
        let retrieved = provider.get_event(timestamp, "evt001").unwrap().unwrap();

        // Should NOT have _updated or _deleted fields
        assert!(retrieved.get("_updated").is_none());
        assert!(retrieved.get("_deleted").is_none());

        // Should only have user-defined fields
        assert!(retrieved.get("id").is_some());
        assert!(retrieved.get("event_type").is_some());
    }

    #[test]
    fn test_table_metadata() {
        let (provider, _test_db) = create_test_provider();

        assert_eq!(provider.namespace_id().as_str(), "app");
        assert_eq!(provider.table_name().as_str(), "events");
        assert!(!provider.is_ephemeral());
        assert_eq!(provider.retention_seconds(), Some(300));
        assert_eq!(provider.max_buffer(), Some(10000));
    }

    #[test]
    fn test_ephemeral_mode_without_live_queries() {
        // Test ephemeral mode when no LiveQueriesProvider is set
        // Should fall back to normal insert behavior
        let test_db = TestDb::new(&["stream_app:ephemeral_events"]).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
        ]));

        let table_metadata = TableMetadata {
            table_name: TableName::new("ephemeral_events"),
            table_type: TableType::Stream,
            namespace: NamespaceId::new("app"),
            created_at: chrono::Utc::now(),
            storage_location: String::new(),
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            schema_version: 1,
            deleted_retention_hours: None,
        };

        let store = Arc::new(StreamTableStore::new(test_db.db.clone()).unwrap());

        let provider = StreamTableProvider::new(
            table_metadata,
            schema,
            store,
            Some(300),
            true, // ephemeral = true
            Some(10000),
        );

        // Insert should succeed even without LiveQueriesProvider
        // (conservative behavior - doesn't discard)
        let result = provider.insert_event("evt001", json!({"id": 1, "event_type": "test"}));
        assert!(result.is_ok());

        // Event should be stored
        let events = provider.scan_events().unwrap();
        assert_eq!(events.len(), 1);

        // Discarded count should be 0 (no discarding without provider)
        assert_eq!(provider.discarded_count(), 0);
    }

    #[test]
    fn test_ephemeral_mode_with_no_subscribers() {
        // Test ephemeral mode when no subscribers exist
        // Events should be discarded
        let test_db =
            TestDb::new(&["stream_app:ephemeral_events2", "system_live_queries"]).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
        ]));

        let table_metadata = TableMetadata {
            table_name: TableName::new("ephemeral_events2"),
            table_type: TableType::Stream,
            namespace: NamespaceId::new("app"),
            created_at: chrono::Utc::now(),
            storage_location: String::new(),
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            schema_version: 1,
            deleted_retention_hours: None,
        };

        let store = Arc::new(StreamTableStore::new(test_db.db.clone()).unwrap());
        let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(test_db.db.clone()).unwrap());
        let live_queries = Arc::new(LiveQueriesTableProvider::new(kalam_sql));

        let provider = StreamTableProvider::new(
            table_metadata,
            schema,
            store,
            Some(300),
            true, // ephemeral = true
            Some(10000),
        )
        .with_live_queries(live_queries);

        // Insert event - should be discarded (no subscribers)
        let result = provider.insert_event("evt001", json!({"id": 1, "event_type": "test"}));
        if let Err(ref e) = result {
            eprintln!("Insert error: {}", e);
        }
        assert!(result.is_ok());

        // Event should NOT be stored
        let events = provider.scan_events().unwrap();
        assert_eq!(events.len(), 0);

        // Discarded count should be 1
        assert_eq!(provider.discarded_count(), 1);
    }

    #[test]
    fn test_non_ephemeral_mode_always_stores() {
        // Test that non-ephemeral mode always stores events
        let test_db =
            TestDb::new(&["stream_app:persistent_events", "system_live_queries"]).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
        ]));

        let table_metadata = TableMetadata {
            table_name: TableName::new("persistent_events"),
            table_type: TableType::Stream,
            namespace: NamespaceId::new("app"),
            created_at: chrono::Utc::now(),
            storage_location: String::new(),
            flush_policy: crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            schema_version: 1,
            deleted_retention_hours: None,
        };

        let store = Arc::new(StreamTableStore::new(test_db.db.clone()).unwrap());
        let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(test_db.db.clone()).unwrap());
        let live_queries = Arc::new(LiveQueriesTableProvider::new(kalam_sql));

        let provider = StreamTableProvider::new(
            table_metadata,
            schema,
            store,
            Some(300),
            false, // ephemeral = false
            Some(10000),
        )
        .with_live_queries(live_queries);

        // Insert event - should always store regardless of subscribers
        provider
            .insert_event("evt001", json!({"id": 1, "event_type": "test"}))
            .unwrap();
        provider
            .insert_event("evt002", json!({"id": 2, "event_type": "test"}))
            .unwrap();

        // Events should be stored
        let events = provider.scan_events().unwrap();
        assert_eq!(events.len(), 2);

        // Discarded count should be 0 (non-ephemeral never discards)
        assert_eq!(provider.discarded_count(), 0);
    }
}
