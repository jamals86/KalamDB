//! Typed handler for FLUSH TABLE statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::{JobType, JobId};
use kalamdb_commons::models::TableId;
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
        statement: FlushTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Validate table exists via SchemaRegistry and fetch definition for table_type
        let registry = self.app_context.schema_registry();
        let table_id = TableId::new(statement.namespace.clone(), statement.table_name.clone());
        let table_def = registry.get_table_definition(&table_id)?;        
        if table_def.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Table {}.{} not found",
                statement.namespace.as_str(),
                statement.table_name.as_str()
            )));
        }
        let table_def = table_def.unwrap();
        let table_type_str = format!("{}", table_def.table_type); // relies on Display impl

        // Create a flush job via JobsManager (async execution handled in background loop)
        let job_manager = self.app_context.job_manager();
        let params_json = serde_json::json!({
            "namespace_id": statement.namespace.as_str(),
            "table_name": statement.table_name.as_str(),
            "table_type": table_type_str
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
                "Flush started for table '{}.{}'. Job ID: {}",
                statement.namespace.as_str(),
                statement.table_name.as_str(),
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
