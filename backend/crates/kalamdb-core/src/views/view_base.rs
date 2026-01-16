//! Base traits and common patterns for virtual views
//!
//! This module provides the VirtualView trait and ViewTableProvider wrapper
//! to standardize how views compute their data dynamically using DataFusion patterns.
//!
//! ## Schema Caching
//! Views memoize their Arrow schema using a `static OnceLock<SchemaRef>`.
//! Each view's schema is computed once and shared across all uses.

use crate::schema_registry::RegistryError;
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::{TableProvider, TableType};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::Expr;
use datafusion::physical_plan::ExecutionPlan;
use kalamdb_system::SystemTable;
use std::any::Any;
use std::sync::Arc;

/// VirtualView trait defines the core behavior for virtual tables (views)
///
/// Views compute their data dynamically on each query rather than storing data.
/// This trait provides a common interface for all view implementations.
///
/// ## Schema Caching
/// Implementations should memoize schemas (e.g., with `OnceLock`) to ensure
/// zero-cost schema lookups after initialization.
pub trait VirtualView: Send + Sync + std::fmt::Debug {
    /// Get the SystemTable variant for this view
    ///
    /// This links the view to the centralized SystemTable enum for consistent
    /// identification and schema caching.
    fn system_table(&self) -> SystemTable;

    /// Get the Arrow schema for this view
    ///
    /// Implementations should memoize the schema (e.g., with `OnceLock`).
    fn schema(&self) -> SchemaRef;

    /// Compute a RecordBatch with the current view data
    ///
    /// This is called on each query to generate fresh results.
    fn compute_batch(&self) -> Result<RecordBatch, RegistryError>;

    /// Get the view name for logging and debugging
    ///
    /// Default implementation uses the SystemTable's table_name().
    fn view_name(&self) -> &str {
        self.system_table().table_name()
    }

    /// Check if this is a view (always true for VirtualView implementations)
    fn is_view(&self) -> bool {
        true
    }
}

/// ViewTableProvider wraps a VirtualView and implements DataFusion's TableProvider
///
/// This generic wrapper eliminates code duplication across all view implementations.
/// Each view only needs to implement the VirtualView trait.
#[derive(Debug, Clone)]
pub struct ViewTableProvider<V: VirtualView> {
    view: Arc<V>,
}

impl<V: VirtualView> ViewTableProvider<V> {
    /// Create a new view table provider
    pub fn new(view: Arc<V>) -> Self {
        Self { view }
    }

    /// Get reference to the underlying view
    pub fn view(&self) -> &Arc<V> {
        &self.view
    }
}

#[async_trait]
impl<V: VirtualView + 'static> TableProvider for ViewTableProvider<V> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.view.schema()
    }

    fn table_type(&self) -> TableType {
        TableType::View
    }

    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        use datafusion::datasource::MemTable;

        let schema = self.view.schema();
        let batch = self.view.compute_batch().map_err(|e| {
            DataFusionError::Execution(format!(
                "Failed to compute batch for view {}: {}",
                self.view.view_name(),
                e
            ))
        })?;

        let partitions = vec![vec![batch]];
        let table = MemTable::try_new(schema, partitions)
            .map_err(|e| DataFusionError::Execution(format!("Failed to create MemTable: {}", e)))?;

        table.scan(state, projection, &[], limit).await
    }
}
