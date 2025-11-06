/// Result type for SQL execution
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// DDL operation completed successfully with a message
    Success(String),
    /// Query result as Arrow RecordBatch
    RecordBatch(arrow::array::RecordBatch),
    /// Multiple record batches (for streaming results)
    RecordBatches(Vec<arrow::array::RecordBatch>),
    /// Subscription metadata (for SUBSCRIBE TO commands)
    Subscription(serde_json::Value),
}
