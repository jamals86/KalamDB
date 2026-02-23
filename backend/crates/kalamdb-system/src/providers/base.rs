//! Base traits and utilities for system table providers
//!
//! This module provides common functionality for all system table providers,
//! reducing code duplication and ensuring consistent behavior.
//!
//! ## Key Components
//!
//! - [`SystemTableScan`]: Trait for system tables with unified scan logic
//! - [`FilterExtractor`]: Utility for extracting prefix/start_key from filters
//! - Streaming iterator support for memory-efficient scanning

use std::sync::Arc;

use arrow::array::RecordBatch;
use datafusion::catalog::Session;
use datafusion::common::DataFusionError;
use datafusion::datasource::{MemTable, TableProvider};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::conversions::json_rows_to_arrow_batch;
use kalamdb_commons::models::rows::{Row, SystemTableRow};
use kalamdb_commons::{KSerializable, StorageKey};
use kalamdb_store::{EntityStore, IndexedEntityStore};
use tracing::Instrument;

use crate::error::SystemError;

/// Result type for DataFusion operations
pub type DataFusionResult<T> = Result<T, DataFusionError>;

/// Shared static metadata for indexed system table providers.
///
/// Providers build this once and reuse it across:
/// - `SystemTableScan` implementation (table_name, schema, key parsing)
/// - `TableProvider` implementation (schema)
#[derive(Clone, Copy)]
pub struct IndexedProviderDefinition<K> {
    pub table_name: &'static str,
    pub primary_key_column: &'static str,
    pub schema: fn() -> arrow::datatypes::SchemaRef,
    pub parse_key: fn(&str) -> Option<K>,
}

/// Shared static metadata for non-indexed/simple system table providers.
#[derive(Clone, Copy)]
pub struct SimpleProviderDefinition {
    pub table_name: &'static str,
    pub schema: fn() -> arrow::datatypes::SchemaRef,
}

/// Default pushdown strategy for providers that do not implement pushdown.
pub fn unsupported_filter_pushdown(
    filters: &[&Expr],
) -> DataFusionResult<Vec<TableProviderFilterPushDown>> {
    Ok(vec![TableProviderFilterPushDown::Unsupported; filters.len()])
}

/// Shared conversion path for system table scan serialization.
///
/// Converts `SystemTableRow` values into Arrow using the same ScalarValue->Arrow
/// coercion path used by shared/user tables.
pub fn system_rows_to_batch(
    schema: &arrow::datatypes::SchemaRef,
    rows: Vec<SystemTableRow>,
) -> Result<RecordBatch, SystemError> {
    let rows: Vec<Row> = rows.into_iter().map(|row| row.fields).collect();
    json_rows_to_arrow_batch(schema, rows)
        .map_err(|e| SystemError::SerializationError(format!("system rows to batch failed: {e}")))
}

/// Trait for system table providers with unified scan logic.
///
/// This trait provides a common implementation for the `scan()` method
/// used by all system table providers, reducing code duplication.
///
/// ## Features
/// - Automatic filter extraction for prefix/start_key scans
/// - Secondary index lookup via `find_best_index_for_filters`
/// - Streaming iterator support for memory-efficient scanning
/// - Consistent error handling and logging
///
/// ## Example Implementation
/// ```rust,ignore
/// impl SystemTableScan<UserId, User> for UsersTableProvider {
///     fn store(&self) -> &IndexedEntityStore<UserId, User> { &self.store }
///     fn table_name(&self) -> &str { "system.users" }
///     fn primary_key_column(&self) -> &str { "user_id" }
///     fn schema(&self) -> SchemaRef { Self::schema() }
///     fn create_batch_from_iter(&self, iter: impl Iterator<Item = (UserId, User)>) -> Result<RecordBatch, SystemError> { ... }
/// }
/// ```
#[async_trait::async_trait]
pub trait SystemTableScan<K, V>: Send + Sync
where
    K: StorageKey + Clone + Send + Sync + 'static,
    V: KSerializable + Clone + Send + Sync + 'static,
{
    /// Returns a reference to the indexed entity store
    fn store(&self) -> &IndexedEntityStore<K, V>;

    /// Returns the table name for logging (e.g., "system.users")
    fn table_name(&self) -> &str;

    /// Returns the primary key column name for filter extraction (e.g., "user_id")
    fn primary_key_column(&self) -> &str;

    /// Returns the Arrow schema for this table
    fn arrow_schema(&self) -> arrow::datatypes::SchemaRef;

    /// Parse a string value into the key type (for filter extraction)
    fn parse_key(&self, value: &str) -> Option<K>;

    /// Create a RecordBatch from key-value pairs
    ///
    /// Implementations should convert their entity type to Arrow arrays.
    fn create_batch_from_pairs(&self, pairs: Vec<(K, V)>) -> Result<RecordBatch, SystemError>;

    /// Default scan implementation with filter extraction and index usage
    ///
    /// This method:
    /// 1. Extracts prefix/start_key from filters based on primary key column
    /// 2. Checks for secondary index matches
    /// 3. Uses either index scan or full table scan
    /// 4. Creates a RecordBatch and delegates to MemTable
    async fn base_system_scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let span = tracing::info_span!(
            "system.base_system_scan",
            table_id = self.table_name(),
            filter_count = filters.len(),
            projection_count = projection.map_or(0, Vec::len),
            has_limit = limit.is_some(),
            limit = limit.unwrap_or(0)
        );

        async move {
            use datafusion::logical_expr::Operator;
            use datafusion::scalar::ScalarValue;

            let mut start_key: Option<K> = None;
            let mut prefix: Option<K> = None;

            // Extract start_key/prefix from filters
            let pk_column = self.primary_key_column();
            for expr in filters {
                if let Expr::BinaryExpr(binary) = expr {
                    if let Expr::Column(col) = binary.left.as_ref() {
                        if let Expr::Literal(val, _) = binary.right.as_ref() {
                            if col.name == pk_column {
                                if let ScalarValue::Utf8(Some(s)) = val {
                                    match binary.op {
                                        Operator::Eq => {
                                            prefix = self.parse_key(s);
                                        },
                                        Operator::Gt | Operator::GtEq => {
                                            start_key = self.parse_key(s);
                                        },
                                        _ => {},
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let schema = self.arrow_schema();
            let store = self.store();

            // Prefer secondary index scans when possible (iterator-based to avoid large allocations)
            let mut pairs: Vec<(K, V)> = Vec::new();
            if let Some((index_idx, index_prefix)) = store.find_best_index_for_filters(filters) {
                log::trace!(
                    "[{}] Using secondary index {} for filters: {:?}",
                    self.table_name(),
                    index_idx,
                    filters
                );
                let iter = store
                    .scan_by_index_iter(index_idx, Some(&index_prefix), limit)
                    .map_err(|e| {
                        DataFusionError::Execution(format!(
                            "Failed to scan {} by index: {}",
                            self.table_name(),
                            e
                        ))
                    })?;

                let effective_limit = limit.unwrap_or(100_000);
                for result in iter {
                    let (key, value) = result.map_err(|e| {
                        DataFusionError::Execution(format!(
                            "Failed to scan {} by index: {}",
                            self.table_name(),
                            e
                        ))
                    })?;
                    pairs.push((key, value));
                    if pairs.len() >= effective_limit {
                        break;
                    }
                }
            } else {
                log::trace!(
                    "[{}] Full table scan (no index match) for filters: {:?}",
                    self.table_name(),
                    filters
                );
                let iter =
                    store.scan_iterator(prefix.as_ref(), start_key.as_ref()).map_err(|e| {
                        DataFusionError::Execution(format!(
                            "Failed to create iterator for {}: {}",
                            self.table_name(),
                            e
                        ))
                    })?;

                let effective_limit = limit.unwrap_or(100_000);
                for result in iter {
                    match result {
                        Ok((key, value)) => {
                            pairs.push((key, value));
                            if pairs.len() >= effective_limit {
                                break;
                            }
                        },
                        Err(e) => {
                            log::warn!("Error during scan of {}: {}", self.table_name(), e);
                            continue;
                        },
                    }
                }
            }

            tracing::debug!(row_count = pairs.len(), "base_system_scan collected rows");
            let batch = self.create_batch_from_pairs(pairs).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to build {} batch: {}",
                    self.table_name(),
                    e
                ))
            })?;

            let partitions = vec![vec![batch]];
            let table = MemTable::try_new(schema, partitions).map_err(|e| {
                DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
            })?;

            // Pass through projection and filters to MemTable
            table.scan(state, projection, filters, limit).await
        }
        .instrument(span)
        .await
    }

    /// Streaming scan using EntityIterator (memory-efficient)
    ///
    /// This method uses an iterator instead of loading all rows into memory,
    /// which is useful for large tables or when only a few rows are needed.
    ///
    /// ## Performance Benefits
    /// - Early termination when limit is reached
    /// - Constant memory usage regardless of table size
    /// - Works well with LIMIT clauses
    async fn streaming_scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let span = tracing::info_span!(
            "system.streaming_scan",
            table_id = self.table_name(),
            filter_count = filters.len(),
            projection_count = projection.map_or(0, Vec::len),
            has_limit = limit.is_some(),
            limit = limit.unwrap_or(0)
        );

        async move {
            use datafusion::logical_expr::Operator;
            use datafusion::scalar::ScalarValue;

            let mut start_key: Option<K> = None;
            let mut prefix: Option<K> = None;

            // Extract start_key/prefix from filters
            let pk_column = self.primary_key_column();
            for expr in filters {
                if let Expr::BinaryExpr(binary) = expr {
                    if let Expr::Column(col) = binary.left.as_ref() {
                        if let Expr::Literal(val, _) = binary.right.as_ref() {
                            if col.name == pk_column {
                                if let ScalarValue::Utf8(Some(s)) = val {
                                    match binary.op {
                                        Operator::Eq => {
                                            prefix = self.parse_key(s);
                                        },
                                        Operator::Gt | Operator::GtEq => {
                                            start_key = self.parse_key(s);
                                        },
                                        _ => {},
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let schema = self.arrow_schema();
            let store = self.store();

            // Use iterator for memory-efficient scanning
            let iter = store.scan_iterator(prefix.as_ref(), start_key.as_ref()).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to create iterator for {}: {}",
                    self.table_name(),
                    e
                ))
            })?;

            // Collect with limit - only takes what we need
            let effective_limit = limit.unwrap_or(100_000);
            let mut pairs = Vec::with_capacity(effective_limit.min(1000));

            for result in iter {
                match result {
                    Ok((key, value)) => {
                        pairs.push((key, value));
                        if pairs.len() >= effective_limit {
                            break;
                        }
                    },
                    Err(e) => {
                        log::warn!("Error during streaming scan of {}: {}", self.table_name(), e);
                        continue;
                    },
                }
            }
            tracing::debug!(row_count = pairs.len(), "streaming_scan collected rows");

            let batch = self.create_batch_from_pairs(pairs).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to build {} batch: {}",
                    self.table_name(),
                    e
                ))
            })?;

            let partitions = vec![vec![batch]];
            let table = MemTable::try_new(schema, partitions).map_err(|e| {
                DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
            })?;

            table.scan(state, projection, filters, limit).await
        }
        .instrument(span)
        .await
    }
}

/// Trait for simple system tables without secondary indexes
///
/// For tables like namespaces, storages that don't have IndexedEntityStore.
///
/// ## Performance
///
/// Override `scan_to_batch()` to enable filter/limit-aware scanning.
/// The default falls back to `scan_all_to_batch()` (full table scan).
///
/// When `scan_to_batch()` is overridden, providers can:
/// - Use primary key equality filters for O(1) point lookups
/// - Respect `LIMIT` at the iterator level (early termination)
/// - Avoid materializing the entire table for simple queries
#[async_trait::async_trait]
pub trait SimpleSystemTableScan<K, V>: Send + Sync
where
    K: StorageKey + Clone + Send + Sync + 'static,
    V: KSerializable + Clone + Send + Sync + 'static,
{
    /// Returns the table name for logging
    fn table_name(&self) -> &str;

    /// Returns the Arrow schema for this table
    fn arrow_schema(&self) -> arrow::datatypes::SchemaRef;

    /// Scan all entries and return as RecordBatch (full table scan).
    ///
    /// Used as fallback when `scan_to_batch` is not overridden.
    fn scan_all_to_batch(&self) -> Result<RecordBatch, SystemError>;

    /// Scan entries with optional filter and limit support.
    ///
    /// Override this method to enable optimized scanning:
    /// - Primary key equality filter → point lookup via `EntityStore::get()`
    /// - Limit → iterator with early termination
    ///
    /// Default falls back to `scan_all_to_batch()` (full table scan).
    fn scan_to_batch(
        &self,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> Result<RecordBatch, SystemError> {
        self.scan_all_to_batch()
    }

    /// Default scan implementation for simple tables.
    ///
    /// Uses `scan_to_batch()` for optimized scanning with filter/limit support,
    /// then wraps in a MemTable for DataFusion execution.
    async fn base_simple_scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let span = tracing::info_span!(
            "system.base_simple_scan",
            table_id = self.table_name(),
            filter_count = filters.len(),
            has_limit = limit.is_some(),
            limit = limit.unwrap_or(0),
        );

        async move {
            let schema = self.arrow_schema();
            let batch = self.scan_to_batch(filters, limit).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to build {} batch: {}",
                    self.table_name(),
                    e
                ))
            })?;

            tracing::debug!(row_count = batch.num_rows(), "base_simple_scan collected rows");

            let partitions = vec![vec![batch]];
            let table = MemTable::try_new(schema, partitions).map_err(|e| {
                DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
            })?;

            table.scan(state, projection, filters, limit).await
        }
        .instrument(span)
        .await
    }
}

/// Helper function to extract string equality filter value for a column
pub fn extract_filter_value(filters: &[Expr], column_name: &str) -> Option<String> {
    use datafusion::logical_expr::Operator;
    use datafusion::scalar::ScalarValue;

    for expr in filters {
        if let Expr::BinaryExpr(binary) = expr {
            if let Expr::Column(col) = binary.left.as_ref() {
                if col.name == column_name && binary.op == Operator::Eq {
                    if let Expr::Literal(ScalarValue::Utf8(Some(s)), _) = binary.right.as_ref() {
                        return Some(s.clone());
                    }
                }
            }
        }
    }
    None
}

/// Helper function to extract range filter values for a column
pub fn extract_range_filters(
    filters: &[Expr],
    column_name: &str,
) -> (Option<String>, Option<String>) {
    use datafusion::logical_expr::Operator;
    use datafusion::scalar::ScalarValue;

    let mut start = None;
    let mut end = None;

    for expr in filters {
        if let Expr::BinaryExpr(binary) = expr {
            if let Expr::Column(col) = binary.left.as_ref() {
                if col.name == column_name {
                    if let Expr::Literal(ScalarValue::Utf8(Some(s)), _) = binary.right.as_ref() {
                        match binary.op {
                            Operator::Gt | Operator::GtEq => start = Some(s.clone()),
                            Operator::Lt | Operator::LtEq => end = Some(s.clone()),
                            _ => {},
                        }
                    }
                }
            }
        }
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::arrow::array::{RecordBatch, StringArray};
    use datafusion::arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::context::SessionContext;
    use datafusion::logical_expr::{col, lit};
    use kalamdb_commons::{KSerializable, StorageKey};
    use kalamdb_store::test_utils::InMemoryBackend;
    use kalamdb_store::{IndexedEntityStore, StorageBackend};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct DummyValue {
        value: String,
    }

    impl KSerializable for DummyValue {}

    struct RecordingBackend {
        inner: InMemoryBackend,
        last_scan: Mutex<Option<(Option<Vec<u8>>, Option<Vec<u8>>, Option<usize>)>>,
        scan_calls: AtomicUsize,
    }

    impl RecordingBackend {
        fn new() -> Self {
            Self {
                inner: InMemoryBackend::new(),
                last_scan: Mutex::new(None),
                scan_calls: AtomicUsize::new(0),
            }
        }

        fn last_scan(&self) -> Option<(Option<Vec<u8>>, Option<Vec<u8>>, Option<usize>)> {
            self.last_scan.lock().unwrap().clone()
        }

        fn scan_calls(&self) -> usize {
            self.scan_calls.load(Ordering::SeqCst)
        }
    }

    impl StorageBackend for RecordingBackend {
        fn get(
            &self,
            partition: &kalamdb_commons::storage::Partition,
            key: &[u8],
        ) -> kalamdb_store::storage_trait::Result<Option<Vec<u8>>> {
            self.inner.get(partition, key)
        }

        fn put(
            &self,
            partition: &kalamdb_commons::storage::Partition,
            key: &[u8],
            value: &[u8],
        ) -> kalamdb_store::storage_trait::Result<()> {
            self.inner.put(partition, key, value)
        }

        fn delete(
            &self,
            partition: &kalamdb_commons::storage::Partition,
            key: &[u8],
        ) -> kalamdb_store::storage_trait::Result<()> {
            self.inner.delete(partition, key)
        }

        fn batch(
            &self,
            operations: Vec<kalamdb_store::storage_trait::Operation>,
        ) -> kalamdb_store::storage_trait::Result<()> {
            self.inner.batch(operations)
        }

        fn scan(
            &self,
            partition: &kalamdb_commons::storage::Partition,
            prefix: Option<&[u8]>,
            start_key: Option<&[u8]>,
            limit: Option<usize>,
        ) -> kalamdb_store::storage_trait::Result<kalamdb_commons::storage::KvIterator<'_>>
        {
            *self.last_scan.lock().unwrap() =
                Some((prefix.map(|p| p.to_vec()), start_key.map(|k| k.to_vec()), limit));
            self.scan_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.scan(partition, prefix, start_key, limit)
        }

        fn partition_exists(&self, partition: &kalamdb_commons::storage::Partition) -> bool {
            self.inner.partition_exists(partition)
        }

        fn create_partition(
            &self,
            partition: &kalamdb_commons::storage::Partition,
        ) -> kalamdb_store::storage_trait::Result<()> {
            self.inner.create_partition(partition)
        }

        fn list_partitions(
            &self,
        ) -> kalamdb_store::storage_trait::Result<Vec<kalamdb_commons::storage::Partition>>
        {
            self.inner.list_partitions()
        }

        fn drop_partition(
            &self,
            partition: &kalamdb_commons::storage::Partition,
        ) -> kalamdb_store::storage_trait::Result<()> {
            self.inner.drop_partition(partition)
        }

        fn compact_partition(
            &self,
            partition: &kalamdb_commons::storage::Partition,
        ) -> kalamdb_store::storage_trait::Result<()> {
            self.inner.compact_partition(partition)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    struct DummyProvider {
        store: IndexedEntityStore<kalamdb_commons::models::UserId, DummyValue>,
    }

    impl DummyProvider {
        fn new(backend: Arc<dyn StorageBackend>) -> Self {
            let store = IndexedEntityStore::new(backend, "system_dummy", Vec::new());
            Self { store }
        }
    }

    #[async_trait::async_trait]
    impl SystemTableScan<kalamdb_commons::models::UserId, DummyValue> for DummyProvider {
        fn store(&self) -> &IndexedEntityStore<kalamdb_commons::models::UserId, DummyValue> {
            &self.store
        }

        fn table_name(&self) -> &str {
            "system.dummy"
        }

        fn primary_key_column(&self) -> &str {
            "user_id"
        }

        fn arrow_schema(&self) -> arrow::datatypes::SchemaRef {
            Arc::new(Schema::new(vec![Field::new("user_id", DataType::Utf8, false)]))
        }

        fn parse_key(&self, value: &str) -> Option<kalamdb_commons::models::UserId> {
            Some(kalamdb_commons::models::UserId::new(value))
        }

        fn create_batch_from_pairs(
            &self,
            pairs: Vec<(kalamdb_commons::models::UserId, DummyValue)>,
        ) -> Result<RecordBatch, SystemError> {
            let values: Vec<Option<String>> =
                pairs.into_iter().map(|(id, _)| Some(id.as_str().to_string())).collect();
            let array = StringArray::from(values);
            RecordBatch::try_new(self.arrow_schema(), vec![Arc::new(array)])
                .map_err(|e| SystemError::Other(e.to_string()))
        }
    }

    #[tokio::test]
    async fn test_base_system_scan_uses_prefix_for_pk_filter() {
        let backend = Arc::new(RecordingBackend::new());
        let provider = DummyProvider::new(backend.clone());

        let user_id = kalamdb_commons::models::UserId::new("u1");
        provider
            .store
            .insert(
                &user_id,
                &DummyValue {
                    value: "v".to_string(),
                },
            )
            .unwrap();

        let ctx = SessionContext::new();
        let state = ctx.state();
        let filter = col("user_id").eq(lit("u1"));

        let _plan = provider.base_system_scan(&state, None, &[filter], None).await.unwrap();

        assert_eq!(backend.scan_calls(), 1);
        let last = backend.last_scan().expect("missing scan");
        assert_eq!(last.0, Some(user_id.storage_key()));
        assert_eq!(last.1, None);
    }
}
