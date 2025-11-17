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
pub mod default_evaluator;

use crate::error::KalamDbError;
use crate::sql::executor::handler_registry::HandlerRegistry;
use crate::sql::executor::models::{ExecutionContext, ExecutionMetadata, ExecutionResult};
use datafusion::scalar::ScalarValue;
use kalamdb_commons::NamespaceId;
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
        let handler_registry = Arc::new(HandlerRegistry::new(app_context.clone(), enforce_password_complexity));
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
            // Tables are already registered in base session, we just inject user_id
            SqlStatementKind::Select => {
                self.execute_via_datafusion(sql, params, exec_ctx).await
            }
            
            // All other statements: Delegate to handler registry
            _ => {
                self.handler_registry
                    .handle(classified, params, exec_ctx)
                    .await
            }
        }
    }

    /// Execute SELECT via DataFusion with per-user session
    async fn execute_via_datafusion(
        &self,
        sql: &str,
        params: Vec<ScalarValue>,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use crate::sql::executor::parameter_binding::validate_params;

        // Validate parameters if present (binding will be added in future iteration)
        if !params.is_empty() {
            validate_params(&params)?;
            // TODO: Implement actual parameter binding via LogicalPlan traversal
            // For now, reject queries with parameters
            return Err(KalamDbError::NotImplemented {
                feature: "Parameter binding".to_string(),
                message: "Parameter validation is implemented, binding will be added in next iteration".to_string(),
            });
        }

        // Create per-request SessionContext with user_id injected
        // Tables are already registered in base session (done once on CREATE TABLE or server startup)
        // The user_id injection allows UserTableProvider::scan() to filter by current user
        let session = exec_ctx.create_session_with_user();

        // Parse SQL and get DataFrame (with detailed logging on failure)
        let df = match session.sql(sql).await {
            Ok(df) => df,
            Err(e) => {
                // Check if this is a table not found error (likely user typo)
                let error_msg = e.to_string().to_lowercase();
                let is_table_not_found = error_msg.contains("table") && error_msg.contains("not found")
                    || error_msg.contains("relation") && error_msg.contains("does not exist")
                    || error_msg.contains("unknown table");
                
                if is_table_not_found {
                    // Log as warning for table not found (likely user typo)
                    log::warn!(
                        target: "sql::plan",
                        "⚠️  Table not found | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                        sql,
                        exec_ctx.user_id.as_str(),
                        exec_ctx.user_role,
                        e
                    );
                } else {
                    // Log planning failure with rich context
                    log::error!(
                        target: "sql::plan",
                        "❌ SQL planning failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                        sql,
                        exec_ctx.user_id.as_str(),
                        exec_ctx.user_role,
                        e
                    );
                }
                return Err(KalamDbError::Other(format!("Error planning query: {}", e)));
            }
        };

        // Execute and collect results (log execution errors)
        let batches = match df.collect().await {
            Ok(batches) => batches,
            Err(e) => {
                log::error!(
                    target: "sql::exec",
                    "❌ SQL execution failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                    sql,
                    exec_ctx.user_id.as_str(),
                    exec_ctx.user_role,
                    e
                );
                return Err(KalamDbError::Other(format!("Error executing query: {}", e)));
            }
        };

        // Calculate total row count
        let row_count: usize = batches.iter().map(|b| b.num_rows()).sum();

        // Informational log for debugging result sizes in tests
        log::info!(
            target: "sql::exec",
            "✅ SQL executed | sql='{}' | user='{}' | role='{:?}' | rows={}",
            sql,
            exec_ctx.user_id.as_str(),
            exec_ctx.user_role,
            row_count
        );

        // Return batches with row count
        Ok(ExecutionResult::Rows { batches, row_count })
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
    pub async fn load_existing_tables(
        &self
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::schemas::TableType;
        use crate::sql::executor::helpers::table_registration::{
            register_user_table_provider,
            register_shared_table_provider,
            register_stream_table_provider,
        };

        let app_context = &self.app_context;
        let schema_registry = app_context.schema_registry();

        // Load all table definitions from the store (much cleaner than scanning Arrow batches!)
        let all_table_defs = schema_registry.scan_all_table_definitions()?;
        
        if all_table_defs.is_empty() {
            log::info!("No existing tables to load");
            return Ok(());
        }

        let mut user_count = 0;
        let mut shared_count = 0;
        let mut stream_count = 0;
        let mut system_count = 0;

        // Iterate through all table definitions and register providers
        for table_def in all_table_defs {
            let table_id = kalamdb_commons::models::TableId::new(
                table_def.namespace_id.clone(),
                table_def.table_name.clone(),
            );
            
            // Skip system tables (already registered in AppContext)
            if matches!(table_def.table_type, TableType::System) {
                system_count += 1;
                continue;
            }

            // Convert to Arrow schema
            let arrow_schema = match table_def.to_arrow_schema() {
                Ok(schema) => schema,
                Err(e) => {
                    log::error!("Failed to convert table definition to Arrow schema for {}.{}: {}", 
                               table_def.namespace_id.as_str(), table_def.table_name.as_str(), e);
                    continue;
                }
            };

            // Register provider based on type
            match table_def.table_type {
                TableType::User => {
                    register_user_table_provider(&app_context, &table_id, arrow_schema)?;
                    user_count += 1;
                }
                TableType::Shared => {
                    register_shared_table_provider(&app_context, &table_id, arrow_schema)?;
                    shared_count += 1;
                }
                TableType::Stream => {
                    // Extract TTL from table_options
                    let ttl_seconds = if let kalamdb_commons::schemas::TableOptions::Stream(stream_opts) = &table_def.table_options {
                        Some(stream_opts.ttl_seconds)
                    } else {
                        None
                    };
                    register_stream_table_provider(&app_context, &table_id, arrow_schema, ttl_seconds)?;
                    stream_count += 1;
                }
                TableType::System => {
                    // Already handled above
                    unreachable!()
                }
            }
        }

        log::info!(
            "Loaded {} tables: {} user, {} shared, {} stream ({} system already registered)",
            user_count + shared_count + stream_count,
            user_count,
            shared_count,
            stream_count,
            system_count
        );

        Ok(())
    }

    // /// Register a table with DataFusion's catalog system
    // ///
    // /// Creates the namespace schema if it doesn't exist, then registers the provider.
    // #[allow(dead_code)]
    // pub(crate) fn register_table_with_datafusion(
    //     base_session: &Arc<SessionContext>,
    //     namespace_id: &NamespaceId,
    //     table_name: &kalamdb_commons::models::TableName,
    //     provider: Arc<dyn datafusion::datasource::TableProvider>,
    // ) -> Result<(), KalamDbError> {
    //     let catalog_name = base_session
    //         .catalog_names()
    //         .first()
    //         .ok_or_else(|| KalamDbError::InvalidOperation("No catalogs available".to_string()))?
    //         .clone();
        
    //     let catalog = base_session
    //         .catalog(&catalog_name)
    //         .ok_or_else(|| KalamDbError::InvalidOperation(format!("Catalog '{}' not found", catalog_name)))?;
        
    //     // Get or create namespace schema
    //     let schema = catalog
    //         .schema(namespace_id.as_str())
    //         .unwrap_or_else(|| {
    //             // Create namespace schema if it doesn't exist
    //             let new_schema = Arc::new(datafusion::catalog::memory::MemorySchemaProvider::new());
    //             catalog.register_schema(namespace_id.as_str(), new_schema.clone())
    //                 .expect("Failed to register namespace schema");
    //             new_schema
    //         });
        
    //     // Register table
    //     schema
    //         .register_table(table_name.as_str().to_string(), provider)
    //         .map_err(|e| KalamDbError::InvalidOperation(format!("Failed to register table with DataFusion: {}", e)))?;
        
    //     Ok(())
    // }

    /// Expose the shared `AppContext` for upcoming migrations.
    pub fn app_context(&self) -> &Arc<crate::app_context::AppContext> {
        &self.app_context
    }
}
