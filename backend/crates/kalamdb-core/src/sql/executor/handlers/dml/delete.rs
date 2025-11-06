//! DELETE handler (modular)
use crate::error::KalamDbError;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

pub struct DeleteHandler;
impl DeleteHandler { pub fn new() -> Self { Self } }

#[async_trait]
impl StatementHandler for DeleteHandler {
    async fn execute(&self, _session:&SessionContext, statement:SqlStatement, _params:Vec<ScalarValue>, _ctx:&ExecutionContext) -> Result<ExecutionResult,KalamDbError> {
        if !matches!(statement, SqlStatement::Delete) { return Err(KalamDbError::InvalidOperation("DeleteHandler received non-DELETE".into())); }
        Err(KalamDbError::InvalidOperation("DELETE not yet implemented".into()))
    }
}
