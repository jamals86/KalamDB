//! System table provider extension trait
//!
//! This trait provides common methods for all system table providers.

use crate::error::KalamDbError;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;

/// Extension trait for system table providers
///
/// All system table providers should implement this trait to provide:
/// - Table name identification
/// - Schema access
/// - Data loading for scans
pub trait SystemTableProviderExt: Send + Sync {
    /// Get the table name
    fn table_name(&self) -> &'static str;

    /// Get the schema reference
    fn schema_ref(&self) -> SchemaRef;

    /// Load all data as a RecordBatch
    ///
    /// This method is used to scan all rows in the system table.
    fn load_batch(&self) -> Result<RecordBatch, KalamDbError>;
}
