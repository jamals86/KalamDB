//! Typed statement handler trait over parsed AST statements

use crate::error::KalamDbError;
use super::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::DdlAst;

#[async_trait::async_trait]
pub trait TypedStatementHandler<T: DdlAst>: Send + Sync {
    /// Execute a typed parsed statement with full context
    ///
    /// # Parameters
    /// * `session` - DataFusion session context
    /// * `statement` - Parsed statement AST
    /// * `params` - Query parameters ($1, $2, etc.)
    /// * `context` - Execution context (user, role, namespace, etc.)
    async fn execute(
        &self,
    session: &SessionContext,
    statement: T,
    params: Vec<ScalarValue>,
    context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>;

    /// Authorization hook for typed statements (optional override)
    async fn check_authorization(
        &self,
        _statement: &T,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        Ok(())
    }
}
