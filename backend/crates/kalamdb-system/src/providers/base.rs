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
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_commons::StorageKey;
use kalamdb_store::entity_store::KSerializable;
use kalamdb_store::{EntityStore, IndexedEntityStore};

use crate::error::SystemError;

/// Result type for DataFusion operations
pub type DataFusionResult<T> = Result<T, DataFusionError>;

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
///     fn schema(&self) -> SchemaRef { UsersTableSchema::schema() }
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
                                    }
                                    Operator::Gt | Operator::GtEq => {
                                        start_key = self.parse_key(s);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        let schema = self.arrow_schema();
        let store = self.store();

        // Prefer secondary index scans when possible
        let pairs: Vec<(K, V)> = if let Some((index_idx, index_prefix)) =
            store.find_best_index_for_filters(filters)
        {
            log::trace!(
                "[{}] Using secondary index {} for filters: {:?}",
                self.table_name(),
                index_idx,
                filters
            );
            store.scan_by_index(index_idx, Some(&index_prefix), limit).map_err(|e| {
                DataFusionError::Execution(format!(
                    "Failed to scan {} by index: {}",
                    self.table_name(),
                    e
                ))
            })?
        } else {
            log::trace!(
                "[{}] Full table scan (no index match) for filters: {:?}",
                self.table_name(),
                filters
            );
            store
                .scan_all_typed(limit, prefix.as_ref(), start_key.as_ref())
                .map_err(|e| {
                    DataFusionError::Execution(format!("Failed to scan {}: {}", self.table_name(), e))
                })?
        };

        let batch = self.create_batch_from_pairs(pairs).map_err(|e| {
            DataFusionError::Execution(format!("Failed to build {} batch: {}", self.table_name(), e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
        })?;

        // Pass through projection and filters to MemTable
        table.scan(state, projection, filters, limit).await
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
                                    }
                                    Operator::Gt | Operator::GtEq => {
                                        start_key = self.parse_key(s);
                                    }
                                    _ => {}
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
            DataFusionError::Execution(format!("Failed to create iterator for {}: {}", self.table_name(), e))
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
                }
                Err(e) => {
                    log::warn!("Error during streaming scan of {}: {}", self.table_name(), e);
                    continue;
                }
            }
        }

        let batch = self.create_batch_from_pairs(pairs).map_err(|e| {
            DataFusionError::Execution(format!("Failed to build {} batch: {}", self.table_name(), e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
        })?;

        table.scan(state, projection, filters, limit).await
    }
}

/// Trait for simple system tables without secondary indexes
///
/// For tables like namespaces, storages that don't have IndexedEntityStore
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

    /// Scan all entries and return as RecordBatch
    fn scan_all_to_batch(&self) -> Result<RecordBatch, SystemError>;

    /// Default scan implementation for simple tables
    async fn base_simple_scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let schema = self.arrow_schema();
        let batch = self.scan_all_to_batch().map_err(|e| {
            DataFusionError::Execution(format!("Failed to build {} batch: {}", self.table_name(), e))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions).map_err(|e| {
            DataFusionError::Execution(format!("Failed to create MemTable: {}", e))
        })?;

        table.scan(state, projection, &[], limit).await
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
pub fn extract_range_filters(filters: &[Expr], column_name: &str) -> (Option<String>, Option<String>) {
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
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    (start, end)
}
