//! Typed handler for FLUSH TABLE statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::jobs::executors::flush::FlushParams;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::TableId;
use kalamdb_commons::{JobId, JobType};
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

        // Create FlushParams with typed parameters
        let params = FlushParams {
            table_id: table_id.clone(),
            table_type: table_def.table_type,
            flush_threshold: None, // Use default from config
        };

        // Create a flush job via JobsManager (async execution handled in background loop)
        let job_manager = self.app_context.job_manager();
        let idempotency_key = format!(
            "flush-{}-{}",
            statement.namespace.as_str(),
            statement.table_name.as_str()
        );
        let job_id: JobId = job_manager
            .create_job_typed(
                JobType::Flush,
                statement.namespace.clone(),
                params,
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
        use kalamdb_commons::Role;
        // Allow Service, DBA, and System roles to flush tables
        if !matches!(context.user_role(), Role::Service | Role::Dba | Role::System) {
            return Err(KalamDbError::Unauthorized(
                "FLUSH TABLE requires Service, DBA, or System role".to_string(),
            ));
        }
        Ok(())
    }
}
