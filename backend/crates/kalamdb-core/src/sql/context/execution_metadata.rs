/// Metadata about query execution
#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    pub rows_affected: usize,
    pub execution_time_ms: u64,
    pub statement_type: String,
}

impl ExecutionMetadata {
    pub fn new(rows_affected: usize, execution_time_ms: u64, statement_type: String) -> Self {
        Self {
            rows_affected,
            execution_time_ms,
            statement_type,
        }
    }
}
