//! Typed DDL handler for CREATE TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

/// Typed handler for CREATE TABLE statements (all table types: USER, SHARED, STREAM)
pub struct CreateTableHandler {
    app_context: Arc<AppContext>,
}

impl CreateTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateTableStatement> for CreateTableHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: CreateTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "CREATE TABLE not yet implemented in typed handler".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &CreateTableStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Authorization depends on table type (USER tables - any user, SHARED/STREAM - DBA)
        // TODO: Implement proper authorization based on statement.table_type
        Ok(())
    }
}
