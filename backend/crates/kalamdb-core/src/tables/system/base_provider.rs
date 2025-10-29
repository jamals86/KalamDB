//! Shared helper trait for system table providers.
//!
//! This module centralizes the common `TableProvider::scan` boilerplate used by
//! the various system tables. Each provider simply implements
//! [`SystemTableProviderExt`] to expose its schema and record loading logic, and
//! the trait takes care of building an in-memory execution plan with consistent
//! error handling.

use crate::error::KalamDbError;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::{MemTable, TableProvider};
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::physical_plan::ExecutionPlan;
use std::sync::Arc;

/// Shared behaviour for memory-backed system table providers.
pub trait SystemTableProviderExt: Send + Sync {
    /// Display name used in error messages.
    fn table_name(&self) -> &'static str;

    /// Arrow schema for the system table.
    fn schema_ref(&self) -> SchemaRef;

    /// Load all rows for the table into a single record batch.
    fn load_batch(&self) -> Result<RecordBatch, KalamDbError>;

    /// Load the table into one or more record batches.
    ///
    /// Providers that naturally return multiple batches can override this for a
    /// more efficient implementation. By default we wrap [`load_batch`] in a
    /// single-partition vector.
    fn load_batches(&self) -> Result<Vec<RecordBatch>, KalamDbError> {
        Ok(vec![self.load_batch()?])
    }

    /// Build an in-memory scan plan with consistent error handling.
    async fn into_memory_exec(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let schema = self.schema_ref();
        let batches = self.load_batches().map_err(|err| {
            DataFusionError::Execution(format!(
                "Failed to load {} records: {}",
                self.table_name(),
                err
            ))
        })?;

        let partitions = if batches.is_empty() {
            vec![vec![RecordBatch::new_empty(schema.clone())]]
        } else {
            vec![batches]
        };

        let table = MemTable::try_new(schema.clone(), partitions).map_err(|err| {
            DataFusionError::Execution(format!(
                "Failed to build MemTable for {}: {}",
                self.table_name(),
                err
            ))
        })?;

        table.scan(state, projection, &[], limit).await
    }
}
