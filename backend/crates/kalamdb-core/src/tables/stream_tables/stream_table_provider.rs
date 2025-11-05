//! Stream table provider for DataFusion integration
//!
//! This module provides a DataFusion TableProvider implementation for stream tables with:
//! - Ephemeral event storage (memory/RocksDB only, NO Parquet)
//! - Timestamp-prefixed keys for efficient TTL eviction
//! - NO system columns (_updated, _deleted)
//! - Optional ephemeral mode (only store if subscribers exist)
//! - Real-time event delivery to subscribers

use crate::catalog::{NamespaceId, SchemaCache, TableName, TableType};
use crate::error::KalamDbError;
use crate::tables::base_table_provider::{BaseTableProvider, TableProviderCore};
use crate::live_query::manager::{ChangeNotification, LiveQueryManager};
use crate::stores::system_table::SharedTableStoreExt;
use crate::tables::arrow_json_conversion::{arrow_batch_to_json, json_rows_to_arrow_batch};
use crate::tables::system::LiveQueriesTableProvider;
use crate::tables::{StreamTableRow, StreamTableRowId, StreamTableStore};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
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

/// Stream table provider for DataFusion
///
/// Provides SQL query access to stream tables with:
/// - Event insertion with automatic timestamps
/// - Ephemeral storage (memory/RocksDB only)
/// - TTL-based eviction support
/// - Real-time event delivery
///
/// **Important**: Stream tables do NOT have system columns (_updated, _deleted)
///
/// **Phase 10 Optimization**: Uses unified SchemaCache as single source of truth for table metadata
/// **Phase 3B**: Uses TableProviderCore to consolidate common provider fields
pub struct StreamTableProvider {
    /// Core provider fields (table_id, schema, cache, etc.)
    core: TableProviderCore,

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

impl std::fmt::Debug for StreamTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamTableProvider")
            .field("table_id", self.core.table_id())
            .field("retention_seconds", &self.retention_seconds)
            .field("ephemeral", &self.ephemeral)
            .field("max_buffer", &self.max_buffer)
            .finish()
    }
}

impl StreamTableProvider {
    /// Create a new stream table provider
    ///
    /// # Arguments
    /// * `table_id` - Arc<TableId> created once at registration (Phase 10: zero-allocation cache lookups)
    /// * `unified_cache` - Reference to unified SchemaCache for metadata lookups
    /// * `schema` - Arrow schema for the table (NO system columns)
    /// * `store` - StreamTableStore for event operations
    /// * `retention_seconds` - Optional TTL in seconds
    /// * `ephemeral` - If true, only store events if subscribers exist
    /// * `max_buffer` - Optional maximum buffer size
    pub fn new(
        table_id: Arc<TableId>,
        unified_cache: Arc<SchemaCache>,
        schema: SchemaRef,
        store: Arc<StreamTableStore>,
        retention_seconds: Option<u32>,
        ephemeral: bool,
        max_buffer: Option<usize>,
    ) -> Self {
        let core = TableProviderCore::new(
            table_id,
            TableType::Stream,
            schema,
            None, // storage_id - stream tables don't use Parquet
            unified_cache,
        );
        Self {
            core,
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
    pub fn insert_event(&self, row_id: &str, row_data: JsonValue) -> Result<(), KalamDbError> {
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
                    return Ok(());
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
        let timestamp_str = chrono::Utc::now().to_rfc3339();

        // Create StreamTableRow with event data
        let row = StreamTableRow {
            row_id: row_id.to_string(),
            fields: row_data.clone(),
            inserted_at: timestamp_str.clone(),
            _updated: timestamp_str,
            _deleted: false,
            ttl_seconds: self.retention_seconds.map(|s| s as u64),
        };

        // Insert event into StreamTableStore
        EntityStore::put(self.store.as_ref(), &StreamTableRowId::new(row_id), &row)
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        // T154: Notify live query manager with change notification
        if let Some(live_query_manager) = &self.live_query_manager {
            let notification = ChangeNotification::insert(
                self.table_name().as_str().to_string(),
                row_data.clone(),
            );

            // Deliver notification asynchronously (spawn task to avoid blocking)
            let manager = Arc::clone(live_query_manager);
            // Use fully qualified table name (namespace.table)
            let table_name = format!(
                "{}.{}",
                self.namespace_id().as_str(),
                self.table_name().as_str()
            );
            log::info!(
                "📤 StreamTable INSERT: Notifying subscribers for table '{}'",
                table_name
            );
            tokio::spawn(async move {
                if let Err(e) = manager.notify_table_change(&table_name, notification).await {
                    log::error!(
                        "Failed to notify subscribers for table '{}': {}",
                        table_name,
                        e
                    );
                } else {
                    log::info!(
                        "📤 Successfully notified subscribers for table '{}'",
                        table_name
                    );
                }
            });
        }

        Ok(())
    }

    /// Insert multiple events into the stream table
    ///
    /// # Arguments
    /// * `events` - Vector of (row_id, row_data) tuples
    ///
    /// # Returns
    /// Ok(()) on success
    pub fn insert_batch(&self, events: Vec<(String, JsonValue)>) -> Result<(), KalamDbError> {
        for (row_id, row_data) in events {
            self.insert_event(&row_id, row_data)?;
        }

        Ok(())
    }

    /// Get a specific event from the stream table
    ///
    /// # Arguments
    /// * `row_id` - Event identifier
    ///
    /// # Returns
    /// Event data if found, None otherwise
    pub fn get_event(&self, row_id: &str) -> Result<Option<JsonValue>, KalamDbError> {
        EntityStore::get(self.store.as_ref(), &StreamTableRowId::new(row_id))
            .map(|opt| opt.map(|row| row.fields))
            .map_err(|e| KalamDbError::Other(e.to_string()))
    }

    /// Scan all events in the stream table
    ///
    /// # Returns
    /// Vector of (row_id, row_data) tuples
    pub fn scan_events(&self) -> Result<Vec<(JsonValue, String)>, KalamDbError> {
        let rows = self
            .store
            .scan(self.namespace_id().as_str(), self.table_name().as_str())
            .map_err(|e| KalamDbError::Other(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(row_id, row)| (row.fields, row_id))
            .collect())
    }

    /// Count events in the stream table
    ///
    /// Used for max_buffer eviction checks
    pub fn count_events(&self) -> Result<usize, KalamDbError> {
        let events = self.scan_events()?;
        Ok(events.len())
    }

    /// Evict expired events based on TTL
    ///
    /// # Returns
    /// Number of events deleted
    pub fn evict_expired(&self) -> Result<usize, KalamDbError> {
        self.store
            .cleanup_expired_rows(self.namespace_id().as_str(), self.table_name().as_str())
            .map_err(|e| KalamDbError::Other(e.to_string()))
    }
}

impl BaseTableProvider for StreamTableProvider {
    fn table_id(&self) -> &kalamdb_commons::models::TableId {
        self.core.table_id()
    }

    fn schema_ref(&self) -> SchemaRef {
        self.core.schema_ref()
    }

    fn table_type(&self) -> crate::catalog::TableType {
        self.core.table_type()
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
        self.core.schema_ref()
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
        _state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::arrow::array::{ArrayRef, RecordBatch};

        // Scan all events from the store
        let events = self
            .store
            .scan(
                self.core.namespace().as_str(),
                self.core.table_name().as_str(),
            )
            .map_err(|e| {
                DataFusionError::Execution(format!("Failed to scan stream table: {}", e))
            })?;

        // Apply limit if specified
        let events_to_process = if let Some(limit_val) = limit {
            events.into_iter().take(limit_val).collect()
        } else {
            events
        };

        // Convert events to Arrow RecordBatch (extract .fields from StreamTableRow)
        let row_values: Vec<JsonValue> = events_to_process
            .into_iter()
            .map(|(_id, row)| row.fields)
            .collect();
        let batch = json_rows_to_arrow_batch(&self.core.schema_ref(), row_values).map_err(|e| {
            DataFusionError::Execution(format!("Failed to convert events to Arrow: {}", e))
        })?;

        // Apply projection if specified
        let (final_batch, final_schema) = if let Some(proj_indices) = projection {
            let projected_columns: Vec<ArrayRef> = proj_indices
                .iter()
                .map(|&i| batch.column(i).clone())
                .collect();

            let projected_schema = Arc::new(self.core.schema_ref().project(proj_indices).map_err(|e| {
                DataFusionError::Execution(format!("Failed to project schema: {}", e))
            })?);

            let projected_batch = RecordBatch::try_new(projected_schema.clone(), projected_columns)
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to create projected batch: {}", e))
                })?;

            (projected_batch, projected_schema)
        } else {
            (batch, self.core.schema_ref())
        };

        // Return a MemTable scan with the result
        use datafusion::datasource::MemTable;
        let partitions = vec![vec![final_batch]];
        let table = MemTable::try_new(final_schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;

        // Don't pass projection to MemTable.scan() - we already projected
        table.scan(_state, None, &[], limit).await
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

        let mut total_inserted = 0;

        // Process each batch
        for batch in batches {
            // Convert Arrow RecordBatch to JSON rows
            let json_rows = arrow_batch_to_json(&batch, false).map_err(|e| {
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

        // Return a MemTable scan with the result
        use datafusion::datasource::MemTable;
        let partitions = vec![vec![result_batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;
        table.scan(_state, None, &[], None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CachedTableData, SchemaCache, TableType};
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use kalamdb_store::test_utils::TestDb;
    use serde_json::json;
    use kalamdb_commons::models::schemas::TableDefinition;

    /// Phase 10: Create Arc<TableId> for test providers (avoids allocation on every cache lookup)
    fn create_test_table_id() -> Arc<TableId> {
        Arc::new(TableId::new(
            NamespaceId::new("app"),
            TableName::new("events"),
        ))
    }

    fn create_test_provider() -> (StreamTableProvider, TestDb) {
        let test_db = TestDb::new(&["stream_app:events"]).unwrap();

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
            Field::new("data", DataType::Utf8, true),
        ]));
        // Build unified cache with CachedTableData for tests
        let unified_cache = Arc::new(SchemaCache::new(0, None));
        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("events"));
        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("app"),
                TableName::new("events"),
                TableType::Stream,
                vec![], // Empty columns for test
                None,
            ).unwrap()
        );

        let data = CachedTableData::new(
            table_id.clone(),
            TableType::Stream,
            chrono::Utc::now(),
            None, // stream tables don't use Parquet storage
            crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            "/data/{namespace}/{tableName}/".to_string(),
            1,
            None,
            td,
        );

        unified_cache.insert(table_id.clone(), Arc::new(data));
        let store = Arc::new(StreamTableStore::new(
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone())),
            "stream_app:events",
        ));

        let provider = StreamTableProvider::new(
            create_test_table_id(),
            unified_cache.clone(),
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

        // Verify event was inserted
        let retrieved = provider.get_event("evt001").unwrap();
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
    fn test_evict_expired() {
        // Create a provider with short TTL (1 second)
        let test_db = TestDb::new(&["stream_app:events"]).unwrap();
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("event_type", DataType::Utf8, false),
        ]));

        let unified_cache = Arc::new(SchemaCache::new(0, None));
        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("events"));
        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("app"),
                TableName::new("events"),
                TableType::Stream,
                vec![], // Empty columns for test
                None,
            ).unwrap()
        );
        let data = CachedTableData::new(
            table_id.clone(),
            TableType::Stream,
            chrono::Utc::now(),
            None,
            crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            "/data/{namespace}/{tableName}/".to_string(),
            1,
            None,
            td,
        );
        unified_cache.insert(table_id.clone(), Arc::new(data));
        let store = Arc::new(StreamTableStore::new(
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone())),
            "stream_app:events",
        ));

        let provider = StreamTableProvider::new(
            create_test_table_id(),
            unified_cache.clone(),
            schema,
            store,
            Some(1), // 1 second retention for this test
            false,
            Some(10000),
        );

        // Insert events with short TTL (1 second)
        provider.insert_event("evt001", json!({"id": 1})).unwrap();
        provider.insert_event("evt002", json!({"id": 2})).unwrap();
        provider.insert_event("evt003", json!({"id": 3})).unwrap();

        // Wait for expiration
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Evict expired events
        let deleted = provider.evict_expired().unwrap();
        assert_eq!(deleted, 3); // All events should be deleted

        // Verify no events remain
        let events = provider.scan_events().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_no_system_columns() {
        let (provider, _test_db) = create_test_provider();

        let event_data = json!({
            "id": 1,
            "event_type": "click"
        });

        provider.insert_event("evt001", event_data).unwrap();

        // Retrieve and verify no system columns
        let retrieved = provider.get_event("evt001").unwrap().unwrap();

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

        let unified_cache = Arc::new(SchemaCache::new(0, None));
        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("ephemeral_events"));
        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("app"),
                TableName::new("events"),
                TableType::Stream,
                vec![], // Empty columns for test
                None,
            ).unwrap()
        );
        let data = CachedTableData::new(
            table_id.clone(),
            TableType::Stream,
            chrono::Utc::now(),
            None,
            crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            "/data/{namespace}/{tableName}/".to_string(),
            1,
            None,
            td,
        );
        unified_cache.insert(table_id.clone(), Arc::new(data));
        let store = Arc::new(StreamTableStore::new(
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone())),
            "stream_app:ephemeral_events",
        ));

        let provider = StreamTableProvider::new(
            create_test_table_id(),
            unified_cache.clone(),
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

        let unified_cache = Arc::new(SchemaCache::new(0, None));
        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("ephemeral_events2"));
        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("app"),
                TableName::new("events"),
                TableType::Stream,
                vec![], // Empty columns for test
                None,
            ).unwrap()
        );
        let data = CachedTableData::new(
            table_id.clone(),
            TableType::Stream,
            chrono::Utc::now(),
            None,
            crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            "/data/{namespace}/{tableName}/".to_string(),
            1,
            None,
            td,
        );
        unified_cache.insert(table_id.clone(), Arc::new(data));
        let store = Arc::new(StreamTableStore::new(
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone())),
            "stream_app:ephemeral_events2",
        ));
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let live_queries = Arc::new(LiveQueriesTableProvider::new(backend));
        let provider = StreamTableProvider::new(
            create_test_table_id(),
            unified_cache.clone(),
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

        let unified_cache = Arc::new(SchemaCache::new(0, None));
        let table_id = TableId::new(NamespaceId::new("app"), TableName::new("persistent_events"));
        let td: Arc<TableDefinition> = Arc::new(
            TableDefinition::new_with_defaults(
                NamespaceId::new("app"),
                TableName::new("events"),
                TableType::Stream,
                vec![], // Empty columns for test
                None,
            ).unwrap()
        );
        let data = CachedTableData::new(
            table_id.clone(),
            TableType::Stream,
            chrono::Utc::now(),
            None,
            crate::flush::FlushPolicy::RowLimit { row_limit: 0 },
            "/data/{namespace}/{tableName}/".to_string(),
            1,
            None,
            td,
        );
        unified_cache.insert(table_id.clone(), Arc::new(data));
        let store = Arc::new(StreamTableStore::new(
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone())),
            "stream_app:persistent_events",
        ));
        let backend: Arc<dyn kalamdb_store::StorageBackend> =
            Arc::new(kalamdb_store::RocksDBBackend::new(test_db.db.clone()));
        let live_queries = Arc::new(LiveQueriesTableProvider::new(backend));

        let provider = StreamTableProvider::new(
            create_test_table_id(),
            unified_cache.clone(),
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
