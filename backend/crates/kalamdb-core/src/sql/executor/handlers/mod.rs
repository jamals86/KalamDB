//! SQL Execution Handlers
//!
//! This module provides modular handlers for different types of SQL operations:
//! - **models**: Core types (ExecutionContext, ScalarValue, ExecutionResult, ExecutionMetadata)
//! - **authorization**: Authorization gateway (COMPLETE - Phase 9.3)
//! - **transaction**: Transaction handling (COMPLETE - Phase 9.4)
//! - **ddl**: DDL operations (future)
//! - **dml**: DML operations (future)
//! - **query**: Query execution (future)
//! - **flush**: Flush operations (future)
//! - **subscription**: Live query subscriptions (future)
//! - **user_management**: User CRUD operations (future)
//! - **table_registry**: Table registration (REMOVED - deprecated REGISTER/UNREGISTER)
//! - **system_commands**: VACUUM, OPTIMIZE, ANALYZE (future)
//! - **helpers**: Shared helper functions (future)
//! - **audit**: Audit logging (future)

use crate::error::KalamDbError;
use datafusion::execution::context::SessionContext;
use kalamdb_sql::statement_classifier::SqlStatement;

// Core types relocated to executor/models in v3
pub mod authorization;
pub mod ddl;

// Phase 7 (US3): New handlers
pub mod dml;
pub mod query;
pub mod flush;
pub mod subscription;
pub mod user_management;
// pub mod table_registry; // removed
pub mod system_commands;

// Re-export core types from executor/models for convenience
pub use crate::sql::executor::models::{ExecutionContext, ExecutionMetadata, ExecutionResult, ScalarValue};
pub use authorization::AuthorizationHandler;
pub use ddl::DDLHandler;

// Phase 7 (US3): Re-export new handlers
pub use dml::DMLHandler;
pub use query::QueryHandler;
pub use flush::FlushHandler;
pub use subscription::SubscriptionHandler;
pub use user_management::UserManagementHandler;
// pub use table_registry::TableRegistryHandler; // removed
pub use system_commands::SystemCommandsHandler;

/// Common trait for SQL statement handlers
///
/// All statement handlers should implement this trait to provide a consistent
/// interface for executing SQL operations.
///
/// **Phase 2 Task T016**: Unified handler interface for all SQL statement types
///
/// # Example
///
/// ```ignore
/// use kalamdb_core::sql::executor::handlers::{StatementHandler, ExecutionContext, ExecutionResult};
/// use async_trait::async_trait;
///
/// pub struct MyHandler;
///
/// #[async_trait]
/// impl StatementHandler for MyHandler {
///     async fn execute(
///         &self,
///         session: &SessionContext,
///         statement: SqlStatement,
///         params: Vec<ScalarValue>,
///         context: &ExecutionContext,
///     ) -> Result<ExecutionResult, KalamDbError> {
///         // Handler implementation
///         Ok(ExecutionResult::Success("Completed".to_string()))
///     }
///
///     async fn check_authorization(
///         &self,
///         statement: &SqlStatement,
///         context: &ExecutionContext,
///     ) -> Result<(), KalamDbError> {
///         // Authorization checks
///         Ok(())
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait StatementHandler: Send + Sync {
    /// Execute a SQL statement with full context
    ///
    /// # Arguments
    /// * `session` - DataFusion session context for query execution
    /// * `statement` - Parsed SQL statement (from kalamdb_sql)
    /// * `params` - Parameter values for prepared statements (? placeholders)
    /// * `context` - Execution context (user, role, namespace, audit info)
    ///
    /// # Returns
    /// * `Ok(ExecutionResult)` - Successful execution result
    /// * `Err(KalamDbError)` - Execution error
    async fn execute(
        &self,
        session: &SessionContext,
        statement: SqlStatement,
        params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError>;

    /// Validate authorization before execution
    ///
    /// Called by the authorization gateway before routing to the handler.
    /// Handlers can implement statement-specific authorization logic here.
    ///
    /// # Arguments
    /// * `statement` - SQL statement to authorize
    /// * `context` - Execution context with user/role information
    ///
    /// # Returns
    /// * `Ok(())` - Authorization passed
    /// * `Err(KalamDbError::PermissionDenied)` - Authorization failed
    async fn check_authorization(
        &self,
        statement: &SqlStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Default implementation: delegate to AuthorizationHandler
        AuthorizationHandler::check_authorization(context, statement)
    }
}
