//! SQL executor orchestrator (Phase 9 foundations)
//!
//! Minimal dispatcher that keeps the public `SqlExecutor` API compiling
//! while provider-based handlers are migrated. Behaviour is intentionally
//! limited for now; most statements return a structured error until
//! handler implementations are in place.

pub mod handlers;

use crate::error::KalamDbError;
use crate::sql::executor::handlers::{
    AuthorizationHandler, ExecutionContext, ExecutionMetadata, ExecutionResult,
};
use datafusion::prelude::SessionContext;
use kalamdb_commons::models::UserId;
use kalamdb_sql::statement_classifier::SqlStatement;
use std::sync::Arc;

// Re-export handler types so external callers keep working without changes.
pub use handlers::{ExecutionContext as ExecutorContextAlias, ExecutionMetadata as ExecutorMetadataAlias, ExecutionResult as ExecutorResultAlias, ParamValue};

/// Public facade for SQL execution routing.
pub struct SqlExecutor {
    app_context: Arc<crate::app_context::AppContext>,
    enforce_password_complexity: bool,
}

impl SqlExecutor {
    /// Construct a new executor hooked into the shared `AppContext`.
    pub fn new(
        app_context: Arc<crate::app_context::AppContext>,
        enforce_password_complexity: bool,
    ) -> Self {
        Self {
            app_context,
            enforce_password_complexity,
        }
    }

    /// Builder toggle that keeps the legacy API intact.
    pub fn with_password_complexity(mut self, enforce: bool) -> Self {
        self.enforce_password_complexity = enforce;
        self
    }

    /// Execute a statement without request metadata.
    pub async fn execute(
        &self,
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.execute_with_metadata(session, sql, exec_ctx, None).await
    }

    /// Execute a statement with optional metadata.
    pub async fn execute_with_metadata(
        &self,
        _session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        _metadata: Option<&ExecutionMetadata>,
    ) -> Result<ExecutionResult, KalamDbError> {
        let classified = SqlStatement::classify(sql);
        AuthorizationHandler::check_authorization(exec_ctx, &classified)?;

        Err(KalamDbError::InvalidOperation(format!(
            "Statement '{}' not yet supported in provider-only executor",
            classified.name()
        )))
    }

    /// Legacy bootstrap hook (currently a no-op until handler wiring lands).
    pub async fn load_existing_tables(
        &self,
        _default_user_id: UserId,
    ) -> Result<(), KalamDbError> {
        Ok(())
    }

    /// Expose the shared `AppContext` for upcoming migrations.
    pub fn app_context(&self) -> &Arc<crate::app_context::AppContext> {
        &self.app_context
    }
}
