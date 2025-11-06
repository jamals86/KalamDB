//! INSERT handler (modular)
use crate::error::KalamDbError;
use crate::sql::executor::handlers::StatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

pub struct InsertHandler;
impl InsertHandler { pub fn new() -> Self { Self } }

#[async_trait]
impl StatementHandler for InsertHandler {
    async fn execute(&self, _session:&SessionContext, statement:SqlStatement, _params:Vec<ScalarValue>, _ctx:&ExecutionContext) -> Result<ExecutionResult,KalamDbError> {
        if !matches!(statement, SqlStatement::Insert) { return Err(KalamDbError::InvalidOperation("InsertHandler received non-INSERT".into())); }
        Err(KalamDbError::InvalidOperation("INSERT not yet implemented".into()))
    }
}
