//! Typed handler for FLUSH TABLE statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_commons::{JobType, JobId};
use kalamdb_commons::models::{NamespaceId, TableId, TableName};
use kalamdb_sql::ddl::FlushTableStatement;
use std::sync::Arc;

/// Handler for FLUSH TABLE
pub struct FlushTableHandler {
    app_context: Arc<AppContext>,
}

impl FlushTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<FlushTableStatement> for FlushTableHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        statement: FlushTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Validate table exists via SchemaRegistry
        let registry = self.app_context.schema_registry();
        let table_id = TableId::new(statement.namespace.clone(), statement.table_name.clone());
        if registry.get_table_definition(&table_id)?.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Table {}.{} not found",
                statement.namespace.as_str(),
                statement.table_name.as_str()
            )));
        }

        // Create a flush job via JobsManager (async execution handled in background loop)
        let job_manager = self.app_context.job_manager();
        let params_json = serde_json::json!({
            "table_name": statement.table_name.as_str(),
            "namespace": statement.namespace.as_str()
        });
        let idempotency_key = format!("flush-{}-{}", statement.namespace.as_str(), statement.table_name.as_str());
        let job_id: JobId = job_manager
            .create_job(
                JobType::Flush,
                statement.namespace.clone(),
                params_json,
                Some(idempotency_key),
                None,
            )
            .await?;

        Ok(ExecutionResult::Success {
            message: format!(
                "Flush job created. Job ID: {}",
                job_id.as_str()
            ),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &FlushTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "FLUSH TABLE requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
