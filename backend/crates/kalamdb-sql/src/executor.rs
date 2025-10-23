//! SQL executor for system table operations
//!
//! Executes parsed SQL statements against RocksDB-backed system tables.
//!
//! Note: This module is currently a stub for future implementation.
//! System table queries are handled directly by the adapter layer.

use crate::adapter::RocksDbAdapter;
use crate::parser::SystemStatement;
use anyhow::{Result, bail};

/// SQL executor
pub struct SqlExecutor {
    adapter: RocksDbAdapter,
}

impl SqlExecutor {
    pub fn new(adapter: RocksDbAdapter) -> Self {
        Self { adapter }
    }

    /// Execute a parsed SQL statement
    ///
    /// Currently returns empty results. Full implementation pending.
    /// For now, use the adapter methods directly for system table operations.
    pub fn execute(&self, statement: SystemStatement) -> Result<Vec<serde_json::Value>> {
        match statement {
            SystemStatement::Select { table, .. } => {
                bail!("Direct SELECT execution not yet implemented. Use adapter scan methods for {:?}", table)
            }
            SystemStatement::Insert { table, .. } => {
                bail!("Direct INSERT execution not yet implemented. Use adapter insert methods for {:?}", table)
            }
            SystemStatement::Update { table, .. } => {
                bail!("Direct UPDATE execution not yet implemented. Use adapter update methods for {:?}", table)
            }
            SystemStatement::Delete { table, .. } => {
                bail!("Direct DELETE execution not yet implemented. Use adapter delete methods for {:?}", table)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_executor_stub() {
        // This module is currently a stub
        // Full implementation will be added when needed
    }
}
