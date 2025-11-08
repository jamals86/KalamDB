//! SQL executor orchestrator (Phase 9 foundations)
//!
//! Minimal dispatcher that keeps the public `SqlExecutor` API compiling
//! while provider-based handlers are migrated. Behaviour is intentionally
//! limited for now; most statements return a structured error until
//! handler implementations are in place.

pub mod handler_adapter;
pub mod handler_registry;
pub mod handlers;
pub mod helpers;
pub mod models;
pub mod parameter_binding;
pub mod parameter_validation;

use crate::error::KalamDbError;
use crate::sql::executor::handler_registry::HandlerRegistry;
use crate::sql::executor::models::{ExecutionContext, ExecutionMetadata, ExecutionResult};
use datafusion::prelude::SessionContext;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::NamespaceId;
use kalamdb_commons::models::UserId;
use kalamdb_sql::statement_classifier::SqlStatement;
use std::sync::Arc;

// Re-export model types so external callers keep working without changes.
pub use models::{ExecutionContext as ExecutorContextAlias, ExecutionMetadata as ExecutorMetadataAlias, ExecutionResult as ExecutorResultAlias};

/// Public facade for SQL execution routing.
pub struct SqlExecutor {
    app_context: Arc<crate::app_context::AppContext>,
    handler_registry: Arc<HandlerRegistry>,
    enforce_password_complexity: bool,
}

impl SqlExecutor {
    /// Construct a new executor hooked into the shared `AppContext`.
    pub fn new(
        app_context: Arc<crate::app_context::AppContext>,
        enforce_password_complexity: bool,
    ) -> Self {
        let handler_registry = Arc::new(HandlerRegistry::new(app_context.clone()));
        Self {
            app_context,
            handler_registry,
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
        // Step 1: Classify, authorize, and parse statement in one pass
        // Prioritize SELECT/DML checks as they represent 99% of queries
        // Authorization happens before parsing for fail-fast behavior
        // TODO: Pass namespace context from ExecutionContext
        let classified = SqlStatement::classify_and_parse(
            sql,
            &NamespaceId::new("default"),
            exec_ctx.user_role.clone(),
        )
        .map_err(|msg| KalamDbError::Unauthorized(msg))?;

        // Step 2: Route based on statement type
        use kalamdb_sql::statement_classifier::SqlStatementKind;
        match classified.kind() {
            // Hot path: SELECT queries use DataFusion
            SqlStatementKind::Select => {
                self.execute_via_datafusion(session, sql, params).await
            }
            
            // // Phase 7: INSERT/DELETE/UPDATE use native handlers with TypedStatementHandler
            // SqlStatementKind::Insert(_) | SqlStatementKind::Delete(_) | SqlStatementKind::Update(_) => {
            //     self.handler_registry
            //         .handle(session, classified, sql.to_string(), params, exec_ctx)
            //         .await
            // }
            
            // All other statements: Delegate to handler registry
            _ => {
                self.handler_registry
                    .handle(session, classified, params, exec_ctx)
                    .await
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

        // Calculate total row count
        let row_count: usize = batches.iter().map(|b| b.num_rows()).sum();

        // Return batches with row count
        Ok(ExecutionResult::Rows { batches, row_count })
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
