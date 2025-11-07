//! Typed DDL handler for DESCRIBE TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::DescribeTableStatement;
use std::sync::Arc;

/// Typed handler for DESCRIBE TABLE statements
pub struct DescribeTableHandler {
    app_context: Arc<AppContext>,
}

impl DescribeTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DescribeTableStatement> for DescribeTableHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        _statement: DescribeTableStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "DESCRIBE TABLE not yet implemented".to_string(),
        ))
    }

    async fn check_authorization(
        &self,
        _statement: &DescribeTableStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // DESCRIBE TABLE allowed for all authenticated users who can access the table
        Ok(())
    }
}
