//! SQL executor orchestrator (Phase 9 foundations)
//!
//! Minimal dispatcher that keeps the public `SqlExecutor` API compiling
//! while provider-based handlers are migrated. Behaviour is intentionally
//! limited for now; most statements return a structured error until
//! handler implementations are in place.

pub mod handlers;
pub mod models;

use crate::error::KalamDbError;
use crate::sql::executor::handlers::AuthorizationHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionMetadata, ExecutionResult};
use datafusion::prelude::SessionContext;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::UserId;
use kalamdb_sql::statement_classifier::SqlStatement;
use std::sync::Arc;

// Re-export model types so external callers keep working without changes.
pub use models::{ExecutionContext as ExecutorContextAlias, ExecutionMetadata as ExecutorMetadataAlias, ExecutionResult as ExecutorResultAlias};

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
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.execute_with_metadata(session, sql, exec_ctx, None, params).await
    }

    /// Execute a statement with optional metadata.
    pub async fn execute_with_metadata(
        &self,
        session: &SessionContext,
        sql: &str,
        exec_ctx: &ExecutionContext,
        _metadata: Option<&ExecutionMetadata>,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        let classified = SqlStatement::classify(sql);
        AuthorizationHandler::check_authorization(exec_ctx, &classified)?;

        // Route based on statement type
        match classified {
            SqlStatement::Select | SqlStatement::Insert | SqlStatement::Delete => {
                // Let DataFusion handle these statements with parameter binding
                self.execute_via_datafusion(session, sql, params).await
            }
            SqlStatement::Update => {
                // UPDATE needs custom handling (not yet implemented)
                Err(KalamDbError::InvalidOperation(
                    "UPDATE statement not yet supported".to_string()
                ))
            }
            _ => {
                // All other statements (DDL, system commands, etc.)
                Err(KalamDbError::InvalidOperation(format!(
                    "Statement '{}' not yet supported in provider-only executor",
                    classified.name()
                )))
            }
        }
    }

    /// Execute SELECT/INSERT/DELETE via DataFusion with parameter binding
    async fn execute_via_datafusion(
        &self,
        session: &SessionContext,
        sql: &str,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement parameter binding once we have the full query handler
        // DataFusion supports params via LogicalPlan manipulation, not DataFrame.with_params()
        if !params.is_empty() {
            return Err(KalamDbError::InvalidOperation(
                "Parameter binding not yet implemented (will be added with query handler)".to_string()
            ));
        }

        // Parse SQL and get DataFrame
        let df = session
            .sql(sql)
            .await
            .map_err(|e| KalamDbError::Other(format!("Error planning query: {}", e)))?;

        // Execute and collect results
        let batches = df
            .collect()
            .await
            .map_err(|e| KalamDbError::Other(format!("Error executing query: {}", e)))?;

        // Return batches
        if batches.is_empty() {
            Ok(ExecutionResult::RecordBatches(vec![]))
        } else if batches.len() == 1 {
            Ok(ExecutionResult::RecordBatch(batches[0].clone()))
        } else {
            Ok(ExecutionResult::RecordBatches(batches))
        }
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
