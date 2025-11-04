//! SQL Execution Handlers
//!
//! This module provides modular handlers for different types of SQL operations:
//! - **types**: Core types (ExecutionContext, ParamValue, ExecutionResult, ExecutionMetadata)
//! - **authorization**: Authorization gateway (COMPLETE - Phase 9.3)
//! - **transaction**: Transaction handling (COMPLETE - Phase 9.4)
//! - **ddl**: DDL operations (future)
//! - **dml**: DML operations (future)
//! - **query**: Query execution (future)
//! - **flush**: Flush operations (future)
//! - **subscription**: Live query subscriptions (future)
//! - **user_management**: User CRUD operations (future)
//! - **table_registry**: Table registration (future)
//! - **system_commands**: VACUUM, OPTIMIZE, ANALYZE (future)
//! - **helpers**: Shared helper functions (future)
//! - **audit**: Audit logging (future)

pub mod types;
pub mod authorization;
pub mod ddl;
pub mod transaction;

#[cfg(test)]
mod tests;

// Re-export core types for convenience
pub use types::{ExecutionContext, ExecutionMetadata, ExecutionResult, ParamValue};
pub use authorization::AuthorizationHandler;
pub use ddl::DDLHandler;
pub use transaction::TransactionHandler;
