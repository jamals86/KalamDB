//! Authorization helpers for SQL execution
//!
//! Provides RBAC (Role-Based Access Control) checks for SQL statements.

// These functions will be moved from executor/mod.rs during migration
// For now, this is a placeholder module

use crate::error::KalamDbError;

use super::types::ExecutionContext;

/// Check if the user is authorized to execute the given SQL statement
///
/// This will be migrated from the check_authorization method in executor/mod.rs
/// For now, this is a placeholder.
pub fn check_authorization(_ctx: &ExecutionContext, _sql: &str) -> Result<(), KalamDbError> {
    // TODO: Migrate from executor/mod.rs::check_authorization()
    Ok(())
}
