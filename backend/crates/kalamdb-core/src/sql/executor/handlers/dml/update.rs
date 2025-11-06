//! UPDATE handler (modular)
use crate::error::KalamDbError;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

pub struct UpdateHandler;
impl UpdateHandler { pub fn new() -> Self { Self } }

#[async_trait]
impl StatementHandler for UpdateHandler {
    async fn execute(&self, _session:&SessionContext, statement:SqlStatement, _params:Vec<ScalarValue>, _ctx:&ExecutionContext) -> Result<ExecutionResult,KalamDbError> {
        if !matches!(statement, SqlStatement::Update) { return Err(KalamDbError::InvalidOperation("UpdateHandler received non-UPDATE".into())); }
        Err(KalamDbError::InvalidOperation("UPDATE not yet implemented".into()))
    }
}
