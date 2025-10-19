//! SQL executor for DDL and DML statements
//!
//! This module provides execution logic for SQL statements,
//! coordinating between parsers, services, and DataFusion.

use crate::catalog::Namespace;
use crate::error::KalamDbError;
use crate::services::NamespaceService;
use crate::sql::ddl::{
    AlterNamespaceStatement, CreateNamespaceStatement, DropNamespaceStatement,
    ShowNamespacesStatement,
};
use datafusion::arrow::array::{ArrayRef, StringArray, UInt32Array};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::prelude::SessionContext;
use std::sync::Arc;

/// SQL execution result
#[derive(Debug)]
pub enum ExecutionResult {
    /// DDL operation completed successfully with a message
    Success(String),
    
    /// Query result as Arrow RecordBatch
    RecordBatch(RecordBatch),
    
    /// Multiple record batches (for streaming results)
    RecordBatches(Vec<RecordBatch>),
}

/// SQL executor
///
/// Executes SQL statements by dispatching to appropriate handlers
pub struct SqlExecutor {
    namespace_service: Arc<NamespaceService>,
    #[allow(dead_code)]
    session_context: Arc<SessionContext>,
}

impl SqlExecutor {
    /// Create a new SQL executor
    pub fn new(
        namespace_service: Arc<NamespaceService>,
        session_context: Arc<SessionContext>,
    ) -> Self {
        Self {
            namespace_service,
            session_context,
        }
    }

    /// Execute a SQL statement
    pub async fn execute(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let sql_upper = sql.trim().to_uppercase();

        // Try to parse as DDL first
        if sql_upper.starts_with("CREATE NAMESPACE") {
            return self.execute_create_namespace(sql).await;
        } else if sql_upper.starts_with("SHOW NAMESPACES") {
            return self.execute_show_namespaces(sql).await;
        } else if sql_upper.starts_with("ALTER NAMESPACE") {
            return self.execute_alter_namespace(sql).await;
        } else if sql_upper.starts_with("DROP NAMESPACE") {
            return self.execute_drop_namespace(sql).await;
        }

        // Otherwise, delegate to DataFusion
        Err(KalamDbError::InvalidSql(
            "Unsupported SQL statement. Only namespace DDL is currently supported.".to_string(),
        ))
    }

    /// Execute CREATE NAMESPACE
    async fn execute_create_namespace(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = CreateNamespaceStatement::parse(sql)?;
        let created = self
            .namespace_service
            .create(stmt.name.clone(), stmt.if_not_exists)?;

        let message = if created {
            format!("Namespace '{}' created successfully", stmt.name.as_str())
        } else {
            format!("Namespace '{}' already exists", stmt.name.as_str())
        };

        Ok(ExecutionResult::Success(message))
    }

    /// Execute SHOW NAMESPACES
    async fn execute_show_namespaces(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let _stmt = ShowNamespacesStatement::parse(sql)?;
        let namespaces = self.namespace_service.list()?;

        // Convert to RecordBatch
        let batch = Self::namespaces_to_record_batch(namespaces)?;

        Ok(ExecutionResult::RecordBatch(batch))
    }

    /// Execute ALTER NAMESPACE
    async fn execute_alter_namespace(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = AlterNamespaceStatement::parse(sql)?;
        self.namespace_service
            .update_options(stmt.name.clone(), stmt.options)?;

        let message = format!("Namespace '{}' updated successfully", stmt.name.as_str());

        Ok(ExecutionResult::Success(message))
    }

    /// Execute DROP NAMESPACE
    async fn execute_drop_namespace(&self, sql: &str) -> Result<ExecutionResult, KalamDbError> {
        let stmt = DropNamespaceStatement::parse(sql)?;
        let deleted = self
            .namespace_service
            .delete(stmt.name.clone(), stmt.if_exists)?;

        let message = if deleted {
            format!("Namespace '{}' dropped successfully", stmt.name.as_str())
        } else {
            format!("Namespace '{}' does not exist", stmt.name.as_str())
        };

        Ok(ExecutionResult::Success(message))
    }

    /// Convert namespaces list to Arrow RecordBatch
    fn namespaces_to_record_batch(
        namespaces: Vec<Namespace>,
    ) -> Result<RecordBatch, KalamDbError> {
        let schema = Arc::new(Schema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("table_count", DataType::UInt32, false),
            Field::new("created_at", DataType::Utf8, false),
        ]));

        let names: ArrayRef = Arc::new(StringArray::from(
            namespaces
                .iter()
                .map(|ns| ns.name.as_str())
                .collect::<Vec<_>>(),
        ));

        let table_counts: ArrayRef = Arc::new(UInt32Array::from(
            namespaces.iter().map(|ns| ns.table_count).collect::<Vec<_>>(),
        ));

        let created_at: ArrayRef = Arc::new(StringArray::from(
            namespaces
                .iter()
                .map(|ns| ns.created_at.to_rfc3339())
                .collect::<Vec<_>>(),
        ));

        RecordBatch::try_new(schema, vec![names, table_counts, created_at]).map_err(|e| {
            KalamDbError::Other(format!("Failed to create RecordBatch: {}", e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::prelude::SessionContext;
    use kalamdb_sql::KalamSql;
    use kalamdb_store::test_utils::TestDb;

    fn setup_test_executor() -> SqlExecutor {
        let test_db = TestDb::new(&[
            "system_namespaces",
            "system_tables",
            "system_table_schemas"
        ]).unwrap();

        let kalam_sql = Arc::new(KalamSql::new(test_db.db.clone()).unwrap());
        let namespace_service = Arc::new(NamespaceService::new(kalam_sql));
        let session_context = Arc::new(SessionContext::new());

        SqlExecutor::new(namespace_service, session_context)
    }

    #[tokio::test]
    async fn test_execute_create_namespace() {
        let executor = setup_test_executor();

        let result = executor
            .execute("CREATE NAMESPACE app")
            .await
            .unwrap();

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("created successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_execute_show_namespaces() {
        let executor = setup_test_executor();

        // Create some namespaces
        executor.execute("CREATE NAMESPACE app1").await.unwrap();
        executor.execute("CREATE NAMESPACE app2").await.unwrap();

        let result = executor.execute("SHOW NAMESPACES").await.unwrap();

        match result {
            ExecutionResult::RecordBatch(batch) => {
                assert_eq!(batch.num_rows(), 2);
                assert_eq!(batch.num_columns(), 3);
            }
            _ => panic!("Expected RecordBatch result"),
        }
    }

    #[tokio::test]
    async fn test_execute_alter_namespace() {
        let executor = setup_test_executor();

        executor.execute("CREATE NAMESPACE app").await.unwrap();

        let result = executor
            .execute("ALTER NAMESPACE app SET OPTIONS (enabled = true)")
            .await
            .unwrap();

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("updated successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_execute_drop_namespace() {
        let executor = setup_test_executor();

        executor.execute("CREATE NAMESPACE app").await.unwrap();

        let result = executor.execute("DROP NAMESPACE app").await.unwrap();

        match result {
            ExecutionResult::Success(msg) => {
                assert!(msg.contains("dropped successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_execute_drop_namespace_with_tables() {
        let executor = setup_test_executor();

        executor.execute("CREATE NAMESPACE app").await.unwrap();

        // Simulate adding a table
        executor
            .namespace_service
            .increment_table_count("app")
            .unwrap();

        let result = executor.execute("DROP NAMESPACE app").await;
        assert!(result.is_err());
    }
}
