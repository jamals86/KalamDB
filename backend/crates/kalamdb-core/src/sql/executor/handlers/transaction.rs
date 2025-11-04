//! Transaction control handlers
//!
//! BEGIN, COMMIT, ROLLBACK transaction statements.

// These functions will be moved from executor/mod.rs during migration

use crate::error::KalamDbError;
use crate::sql::executor::SqlExecutor;

use super::types::ExecutionResult;

/// Execute BEGIN TRANSACTION
///
/// Will be migrated from executor/mod.rs::execute_begin_transaction()
pub async fn begin(_executor: &SqlExecutor) -> Result<ExecutionResult, KalamDbError> {
    // TODO: Migrate from executor/mod.rs
    Ok(ExecutionResult::Success(
        "Transaction started (BEGIN)".to_string(),
    ))
}

/// Execute COMMIT TRANSACTION
///
/// Will be migrated from executor/mod.rs::execute_commit_transaction()
pub async fn commit(_executor: &SqlExecutor) -> Result<ExecutionResult, KalamDbError> {
    // TODO: Migrate from executor/mod.rs
    Ok(ExecutionResult::Success(
        "Transaction committed (no-op)".to_string(),
    ))
}

/// Execute ROLLBACK TRANSACTION
///
/// Will be migrated from executor/mod.rs::execute_rollback_transaction()
pub async fn rollback(_executor: &SqlExecutor) -> Result<ExecutionResult, KalamDbError> {
    // TODO: Migrate from executor/mod.rs
    Ok(ExecutionResult::Success(
        "Transaction rollback (not supported - no changes applied)".to_string(),
    ))
}
