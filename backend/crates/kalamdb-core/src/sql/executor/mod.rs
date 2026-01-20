//! SQL executor orchestrator (Phase 9 foundations)
//!
//! Minimal dispatcher that keeps the public `SqlExecutor` API compiling
//! while provider-based handlers are migrated. Behaviour is intentionally
//! limited for now; most statements return a structured error until
//! handler implementations are in place.

pub mod default_evaluator;
pub mod default_ordering;
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
use crate::sql::plan_cache::{PlanCache, PlanCacheKey};
pub use datafusion::scalar::ScalarValue;
use kalamdb_sql::statement_classifier::SqlStatement;
use std::sync::Arc;

// Re-export model types so external callers keep working without changes.
pub use models::{
    ExecutionContext as ExecutorContextAlias, ExecutionMetadata as ExecutorMetadataAlias,
    ExecutionResult as ExecutorResultAlias,
};

/// Public facade for SQL execution routing.
pub struct SqlExecutor {
    app_context: Arc<crate::app_context::AppContext>,
    handler_registry: Arc<HandlerRegistry>,
    plan_cache: Arc<PlanCache>,
}

impl SqlExecutor {
    fn is_table_not_found_error(e: &datafusion::error::DataFusionError) -> bool {
        let msg = e.to_string().to_lowercase();
        (msg.contains("table") && msg.contains("not found"))
            || (msg.contains("relation") && msg.contains("does not exist"))
            || msg.contains("unknown table")
    }

    /// Construct a new executor hooked into the shared `AppContext`.
    pub fn new(
        app_context: Arc<crate::app_context::AppContext>,
        enforce_password_complexity: bool,
    ) -> Self {
        let handler_registry =
            Arc::new(HandlerRegistry::new(app_context.clone(), enforce_password_complexity));
        let plan_cache = Arc::new(PlanCache::new());
        Self {
            app_context,
            handler_registry,
            plan_cache,
        }
    }

    /// Clear the plan cache (e.g., after DDL operations)
    pub fn clear_plan_cache(&self) {
        self.plan_cache.clear();
    }

    /// Get current plan cache size (diagnostics/testing)
    pub fn plan_cache_len(&self) -> usize {
        self.plan_cache.len()
    }

    /// Execute a statement without request metadata.
    pub async fn execute(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.execute_with_metadata(sql, exec_ctx, None, params).await
    }

    /// Execute a statement with optional metadata.
    pub async fn execute_with_metadata(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        _metadata: Option<&ExecutionMetadata>,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Step 0: Check SQL query length to prevent DoS attacks
        // Most legitimate queries are under 10KB, we allow up to 1MB
        if sql.len() > kalamdb_commons::constants::MAX_SQL_QUERY_LENGTH {
            log::warn!(
                "❌ SQL query rejected: length {} bytes exceeds maximum {} bytes",
                sql.len(),
                kalamdb_commons::constants::MAX_SQL_QUERY_LENGTH
            );
            return Err(KalamDbError::InvalidSql(format!(
                "SQL query too long: {} bytes (maximum {} bytes)",
                sql.len(),
                kalamdb_commons::constants::MAX_SQL_QUERY_LENGTH
            )));
        }

        // Step 1: Classify, authorize, and parse statement in one pass
        // Prioritize SELECT/DML checks as they represent 99% of queries
        // Authorization happens before parsing for fail-fast behavior
        let classified = SqlStatement::classify_and_parse(
            sql,
            &exec_ctx.default_namespace(),
            exec_ctx.user_role(),
        )
        .map_err(|e| match e {
            kalamdb_sql::classifier::StatementClassificationError::Unauthorized(msg) => {
                KalamDbError::Unauthorized(msg)
            },
            kalamdb_sql::classifier::StatementClassificationError::InvalidSql {
                sql: _,
                message,
            } => KalamDbError::InvalidSql(message),
        })?;

        // Step 2: Route based on statement type
        use kalamdb_sql::statement_classifier::SqlStatementKind;
        match classified.kind() {
            // Hot path: SELECT queries use DataFusion
            // Tables are already registered in base session, we just inject user_id
            SqlStatementKind::Select => self.execute_via_datafusion(sql, params, exec_ctx).await,

            // DataFusion meta commands (EXPLAIN, SET, SHOW, etc.) - admin only
            // No caching needed - these are diagnostic/config commands
            // Authorization already checked in classifier
            SqlStatementKind::DataFusionMetaCommand => {
                self.execute_meta_command(sql, exec_ctx).await
            },

            // DDL operations that modify table/view structure require plan cache invalidation
            // This prevents stale cached plans from referencing dropped/altered tables
            SqlStatementKind::CreateTable(_)
            | SqlStatementKind::DropTable(_)
            | SqlStatementKind::AlterTable(_)
            | SqlStatementKind::CreateView(_)
            | SqlStatementKind::CreateNamespace(_)
            | SqlStatementKind::DropNamespace(_) => {
                let result = self.handler_registry.handle(classified, params, exec_ctx).await;
                // Clear plan cache after DDL to invalidate any cached plans
                // that may reference the modified schema
                if result.is_ok() {
                    self.plan_cache.clear();
                    log::debug!("Plan cache cleared after DDL operation");
                }
                result
            },

            // All other statements: Delegate to handler registry (no cache invalidation needed)
            _ => self.handler_registry.handle(classified, params, exec_ctx).await,
        }
    }

    /// Execute SELECT via DataFusion with per-user session
    async fn execute_via_datafusion(
        &self,
        sql: &str,
        params: Vec<ScalarValue>,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use crate::sql::executor::default_ordering::apply_default_order_by;
        use crate::sql::executor::parameter_binding::{
            replace_placeholders_in_plan, validate_params,
        };

        // Validate parameters if present
        if !params.is_empty() {
            validate_params(&params)?;
        }

        // Create per-request SessionContext with user_id injected
        // Tables are already registered in base session (done once on CREATE TABLE or server startup)
        // The user_id injection allows UserTableProvider::scan() to filter by current user
        let session = exec_ctx.create_session_with_user();

        // Try to get cached plan first (only if no params - parameterized queries can't use cached plans)
        // Note: Cached plans already have default ORDER BY applied
        // Key excludes user_id because LogicalPlan is user-agnostic - filtering happens at scan time
        let cache_key =
            PlanCacheKey::new(exec_ctx.default_namespace().clone(), exec_ctx.user_role(), sql);

        let df = if params.is_empty() {
            if let Some(plan) = self.plan_cache.get(&cache_key) {
                // Cache hit: Create DataFrame directly from plan
                // This skips parsing, logical planning, and optimization (~1-5ms)
                // The cached plan already has default ORDER BY applied
                // Clone the Arc'd plan for execution (cheap reference count bump)
                match session.execute_logical_plan((*plan).clone()).await {
                    Ok(df) => df,
                    Err(e) => {
                        log::warn!("Failed to create DataFrame from cached plan: {}", e);
                        // Fallback to full planning if cache fails
                        match session.sql(sql).await {
                            Ok(df) => {
                                // Apply default ORDER BY for consistency
                                let plan = df.logical_plan().clone();
                                let ordered_plan = apply_default_order_by(plan, &self.app_context)?;
                                session
                                    .execute_logical_plan(ordered_plan)
                                    .await
                                    .map_err(|e| KalamDbError::ExecutionError(e.to_string()))?
                            },
                            Err(e) => {
                                if Self::is_table_not_found_error(&e) {
                                    log::warn!(
                                        target: "sql::plan",
                                        "⚠️  Table not found during planning; reloading table providers and retrying once | sql='{}'",
                                        sql
                                    );
                                    //let _ = self.load_existing_tables().await;
                                    let retry_session = exec_ctx.create_session_with_user();
                                    match retry_session.sql(sql).await {
                                        Ok(df) => {
                                            let plan = df.logical_plan().clone();
                                            let ordered_plan =
                                                apply_default_order_by(plan, &self.app_context)?;
                                            retry_session
                                                .execute_logical_plan(ordered_plan)
                                                .await
                                                .map_err(|e| {
                                                KalamDbError::ExecutionError(e.to_string())
                                            })?
                                        },
                                        Err(e2) => {
                                            return Err(self.log_sql_error(sql, exec_ctx, e2));
                                        },
                                    }
                                } else {
                                    return Err(self.log_sql_error(sql, exec_ctx, e));
                                }
                            },
                        }
                    },
                }
            } else {
                // Cache miss: Parse SQL and get DataFrame (with detailed logging on failure)
                match session.sql(sql).await {
                    Ok(df) => {
                        // Apply default ORDER BY by primary key columns (or _seq as fallback)
                        // This ensures consistent ordering between hot (RocksDB) and cold (Parquet) storage
                        let plan = df.logical_plan().clone();
                        let ordered_plan = apply_default_order_by(plan, &self.app_context)?;

                        // Cache the ordered plan for future use (scoped by namespace+role)
                        self.plan_cache.insert(cache_key, ordered_plan.clone());

                        // Execute the ordered plan
                        session
                            .execute_logical_plan(ordered_plan)
                            .await
                            .map_err(|e| KalamDbError::ExecutionError(e.to_string()))?
                    },
                    Err(e) => {
                        if Self::is_table_not_found_error(&e) {
                            log::warn!(
                                target: "sql::plan",
                                "⚠️  Table not found during planning; reloading table providers and retrying once | sql='{}'",
                                sql
                            );
                            //let _ = self.load_existing_tables().await;
                            let retry_session = exec_ctx.create_session_with_user();
                            match retry_session.sql(sql).await {
                                Ok(df) => {
                                    let plan = df.logical_plan().clone();
                                    let ordered_plan =
                                        apply_default_order_by(plan, &self.app_context)?;

                                    self.plan_cache.insert(cache_key, ordered_plan.clone());

                                    retry_session
                                        .execute_logical_plan(ordered_plan)
                                        .await
                                        .map_err(|e| KalamDbError::ExecutionError(e.to_string()))?
                                },
                                Err(e2) => {
                                    return Err(self.log_sql_error(sql, exec_ctx, e2));
                                },
                            }
                        } else {
                            return Err(self.log_sql_error(sql, exec_ctx, e));
                        }
                    },
                }
            }
        } else {
            // Parameterized query: Parse, bind parameters, then execute
            // Don't cache parameterized queries (cache key would need to include params)
            let df = match session.sql(sql).await {
                Ok(df) => df,
                Err(e) => {
                    if Self::is_table_not_found_error(&e) {
                        log::warn!(
                            target: "sql::plan",
                            "⚠️  Table not found during planning; reloading table providers and retrying once | sql='{}'",
                            sql
                        );
                        //let _ = self.load_existing_tables().await;
                        let retry_session = exec_ctx.create_session_with_user();
                        match retry_session.sql(sql).await {
                            Ok(df) => df,
                            Err(e2) => {
                                return Err(self.log_sql_error(sql, exec_ctx, e2));
                            },
                        }
                    } else {
                        return Err(self.log_sql_error(sql, exec_ctx, e));
                    }
                },
            };

            // Get the logical plan and replace placeholders with parameter values
            let plan = df.logical_plan().clone();
            let bound_plan = replace_placeholders_in_plan(plan, &params)?;

            // Apply default ORDER BY to the bound plan
            let ordered_plan = apply_default_order_by(bound_plan, &self.app_context)?;

            // Execute the ordered plan
            match session.execute_logical_plan(ordered_plan).await {
                Ok(df) => df,
                Err(e) => {
                    log::error!(
                        target: "sql::exec",
                        "❌ Parameter binding execution failed | sql='{}' | params={} | error='{}'",
                        sql,
                        params.len(),
                        e
                    );
                    return Err(KalamDbError::ExecutionError(e.to_string()));
                },
            }
        };

        // Check permissions on the logical plan
        //FIXME: Check do we still need this?? now we have a permission check in each tableprovider
        //self.check_select_permissions(df.logical_plan(), exec_ctx)?;

        // Capture schema before collecting (needed for 0 row results)
        // DFSchema -> Arrow Schema via inner() method
        let schema: arrow::datatypes::SchemaRef =
            std::sync::Arc::new(df.schema().as_arrow().clone());

        // Execute and collect results (log execution errors)
        let batches = match df.collect().await {
            Ok(batches) => batches,
            Err(e) => {
                log::error!(
                    target: "sql::exec",
                    "❌ SQL execution failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                    sql,
                    exec_ctx.user_id().as_str(),
                    exec_ctx.user_role(),
                    e
                );
                return Err(KalamDbError::Other(format!("Error executing query: {}", e)));
            },
        };

        // Calculate total row count
        let row_count: usize = batches.iter().map(|b| b.num_rows()).sum();

        // Return batches with row count and schema (schema is needed when batches is empty)
        Ok(ExecutionResult::Rows {
            batches,
            row_count,
            schema: Some(schema),
        })
    }

    /// Execute DataFusion meta commands (EXPLAIN, SET, SHOW, etc.)
    ///
    /// These commands are passed directly to DataFusion without custom parsing.
    /// No plan caching is performed since these are diagnostic/config commands.
    /// Authorization is already checked in the classifier (admin only).
    async fn execute_meta_command(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Create per-request SessionContext with user_id injected
        let session = exec_ctx.create_session_with_user();

        // Execute the command directly via DataFusion
        let df = match session.sql(sql).await {
            Ok(df) => df,
            Err(e) => {
                log::error!(
                    target: "sql::meta",
                    "❌ Meta command failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                    sql,
                    exec_ctx.user_id().as_str(),
                    exec_ctx.user_role(),
                    e
                );
                return Err(KalamDbError::ExecutionError(e.to_string()));
            },
        };

        // Capture schema before collecting
        let schema: arrow::datatypes::SchemaRef =
            std::sync::Arc::new(df.schema().as_arrow().clone());

        // Execute and collect results
        let batches = match df.collect().await {
            Ok(batches) => batches,
            Err(e) => {
                log::error!(
                    target: "sql::meta",
                    "❌ Meta command execution failed | sql='{}' | user='{}' | error='{}'",
                    sql,
                    exec_ctx.user_id().as_str(),
                    e
                );
                return Err(KalamDbError::Other(format!("Error executing meta command: {}", e)));
            },
        };

        let row_count: usize = batches.iter().map(|b| b.num_rows()).sum();

        log::debug!(
            target: "sql::meta",
            "✅ Meta command completed | sql='{}' | rows={}",
            sql,
            row_count
        );

        Ok(ExecutionResult::Rows {
            batches,
            row_count,
            schema: Some(schema),
        })
    }

    /// Log SQL errors with appropriate level (warn for user errors, error for system errors)
    fn log_sql_error(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        e: datafusion::error::DataFusionError,
    ) -> KalamDbError {
        let error_msg = e.to_string().to_lowercase();
        let is_table_not_found = error_msg.contains("table") && error_msg.contains("not found")
            || error_msg.contains("relation") && error_msg.contains("does not exist")
            || error_msg.contains("unknown table");

        if is_table_not_found {
            log::warn!(
                target: "sql::plan",
                "⚠️  Table not found | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                sql,
                exec_ctx.user_id().as_str(),
                exec_ctx.user_role(),
                e
            );
        } else {
            log::error!(
                target: "sql::plan",
                "❌ SQL planning failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                sql,
                exec_ctx.user_id().as_str(),
                exec_ctx.user_role(),
                e
            );
        }
        KalamDbError::ExecutionError(e.to_string())
    }

    /// Load existing tables from system.tables and register providers
    ///
    /// Called during server startup to restore table access after restart.
    /// Loads table definitions from the store and creates/registers:
    /// - UserTableShared instances for USER tables
    /// - SharedTableProvider instances for SHARED tables  
    /// - StreamTableProvider instances for STREAM tables
    ///
    /// # Returns
    /// Ok on success, error if table loading fails
    pub async fn load_existing_tables(&self) -> Result<(), KalamDbError> {
        let app_context = &self.app_context;
        // Delegate to unified SchemaRegistry initialization
        app_context.schema_registry().initialize_tables()
    }

    /// Expose the shared `AppContext` for upcoming migrations.
    /// TODO: Remove this since everyone has appcontext access now from the executor
    pub fn app_context(&self) -> &Arc<crate::app_context::AppContext> {
        &self.app_context
    }

}
