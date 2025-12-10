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
use crate::sql::plan_cache::PlanCache;
pub use datafusion::scalar::ScalarValue;
use kalamdb_commons::NamespaceId;
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
    /// Construct a new executor hooked into the shared `AppContext`.
    pub fn new(
        app_context: Arc<crate::app_context::AppContext>,
        enforce_password_complexity: bool,
    ) -> Self {
        let handler_registry = Arc::new(HandlerRegistry::new(
            app_context.clone(),
            enforce_password_complexity,
        ));
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

    /// Execute a statement without request metadata.
    pub async fn execute(
        &self,
        sql: &str,
        exec_ctx: &ExecutionContext,
        params: Vec<ScalarValue>,
    ) -> Result<ExecutionResult, KalamDbError> {
        self.execute_with_metadata(sql, exec_ctx, None, params)
            .await
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
            exec_ctx.user_role,
        )
        .map_err(|e| match e {
            kalamdb_sql::classifier::StatementClassificationError::Unauthorized(msg) => {
                KalamDbError::Unauthorized(msg)
            }
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
        use crate::sql::executor::default_ordering::apply_default_order_by;
        use crate::sql::executor::parameter_binding::{replace_placeholders_in_plan, validate_params};

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
        let df = if params.is_empty() {
            if let Some(plan) = self.plan_cache.get(sql) {
                // Cache hit: Create DataFrame directly from plan
                // This skips parsing, logical planning, and optimization (~1-5ms)
                // The cached plan already has default ORDER BY applied
                match session.execute_logical_plan(plan).await {
                    Ok(df) => df,
                    Err(e) => {
                        log::warn!("Failed to create DataFrame from cached plan: {}", e);
                        // Fallback to full planning if cache fails
                        match session.sql(sql).await {
                            Ok(df) => {
                                // Apply default ORDER BY for consistency
                                let plan = df.logical_plan().clone();
                                let ordered_plan = apply_default_order_by(plan, &self.app_context)?;
                                session.execute_logical_plan(ordered_plan).await.map_err(|e| {
                                    KalamDbError::ExecutionError(e.to_string())
                                })?
                            }
                            Err(e) => return Err(KalamDbError::ExecutionError(e.to_string())),
                        }
                    }
                }
            } else {
                // Cache miss: Parse SQL and get DataFrame (with detailed logging on failure)
                match session.sql(sql).await {
                    Ok(df) => {
                        // Apply default ORDER BY by primary key columns (or _seq as fallback)
                        // This ensures consistent ordering between hot (RocksDB) and cold (Parquet) storage
                        let plan = df.logical_plan().clone();
                        let ordered_plan = apply_default_order_by(plan, &self.app_context)?;

                        // Cache the ordered plan for future use
                        self.plan_cache.insert(sql.to_string(), ordered_plan.clone());

                        // Execute the ordered plan
                        session.execute_logical_plan(ordered_plan).await.map_err(|e| {
                            KalamDbError::ExecutionError(e.to_string())
                        })?
                    }
                    Err(e) => {
                        return Err(self.log_sql_error(sql, exec_ctx, e));
                    }
                }
            }
        } else {
            // Parameterized query: Parse, bind parameters, then execute
            // Don't cache parameterized queries (cache key would need to include params)
            let df = match session.sql(sql).await {
                Ok(df) => df,
                Err(e) => {
                    return Err(self.log_sql_error(sql, exec_ctx, e));
                }
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
                }
            }
        };

        // Check permissions on the logical plan
        self.check_select_permissions(df.logical_plan(), exec_ctx)?;

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
                    exec_ctx.user_id.as_str(),
                    exec_ctx.user_role,
                    e
                );
                return Err(KalamDbError::Other(format!("Error executing query: {}", e)));
            }
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
                exec_ctx.user_id.as_str(),
                exec_ctx.user_role,
                e
            );
        } else {
            log::error!(
                target: "sql::plan",
                "❌ SQL planning failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                sql,
                exec_ctx.user_id.as_str(),
                exec_ctx.user_role,
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
        use crate::sql::executor::helpers::table_registration::{
            register_shared_table_provider, register_stream_table_provider,
            register_user_table_provider,
        };
        use kalamdb_commons::schemas::TableType;

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
                    log::error!(
                        "Failed to convert table definition to Arrow schema for {}.{}: {}",
                        table_def.namespace_id.as_str(),
                        table_def.table_name.as_str(),
                        e
                    );
                    continue;
                }
            };

            // Register provider based on type
            match table_def.table_type {
                TableType::User => {
                    register_user_table_provider(app_context, &table_id, arrow_schema)?;
                    user_count += 1;
                }
                TableType::Shared => {
                    register_shared_table_provider(app_context, &table_id, arrow_schema)?;
                    shared_count += 1;
                }
                TableType::Stream => {
                    // Extract TTL from table_options
                    let ttl_seconds =
                        if let kalamdb_commons::schemas::TableOptions::Stream(stream_opts) =
                            &table_def.table_options
                        {
                            Some(stream_opts.ttl_seconds)
                        } else {
                            None
                        };
                    register_stream_table_provider(
                        app_context,
                        &table_id,
                        arrow_schema,
                        ttl_seconds,
                    )?;
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

    fn check_select_permissions(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        exec_ctx: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        self.check_plan_permissions(plan, exec_ctx)
    }

    fn check_plan_permissions(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        exec_ctx: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use datafusion::logical_expr::LogicalPlan;
        use kalamdb_auth::rbac::can_access_shared_table;
        use kalamdb_commons::models::{NamespaceId, TableId, TableName};
        use kalamdb_commons::schemas::TableOptions;
        use kalamdb_commons::schemas::TableType;
        use kalamdb_commons::TableAccess;

        match plan {
            LogicalPlan::TableScan(scan) => {
                // Resolve table reference to TableId
                let (ns, tbl) = match &scan.table_name {
                    datafusion::common::TableReference::Bare { table } => (
                        NamespaceId::new("default"),
                        TableName::new(table.to_string()),
                    ),
                    datafusion::common::TableReference::Partial { schema, table } => (
                        NamespaceId::new(schema.to_string()),
                        TableName::new(table.to_string()),
                    ),
                    datafusion::common::TableReference::Full { schema, table, .. } => (
                        NamespaceId::new(schema.to_string()),
                        TableName::new(table.to_string()),
                    ),
                };

                let table_id = TableId::new(ns.clone(), tbl.clone());

                // Get table definition
                let schema_registry = self.app_context.schema_registry();
                if let Ok(Some(def)) = schema_registry.get_table_definition(&table_id) {
                    if matches!(def.table_type, TableType::Shared) {
                        let access_level = if let TableOptions::Shared(opts) = &def.table_options {
                            opts.access_level.unwrap_or(TableAccess::Private)
                        } else {
                            TableAccess::Private
                        };

                        if !can_access_shared_table(access_level, false, exec_ctx.user_role)
                        {
                            return Err(KalamDbError::Unauthorized(format!(
                                "Insufficient privileges to read shared table '{}.{}' (Access Level: {:?})",
                                ns.as_str(),
                                tbl.as_str(),
                                access_level
                            )));
                        }
                    } else if matches!(def.table_type, TableType::User) {
                        // Enforce namespace isolation: User tables can only be accessed by their owner
                        // (unless user is admin/system)
                        // TODO: Re-enable this check once we have proper namespace permissions or clarify USER table semantics
                        /*
                        let is_admin = matches!(
                            exec_ctx.user_role,
                            kalamdb_commons::Role::System | kalamdb_commons::Role::Dba
                        );
                        if !is_admin && ns.as_str() != exec_ctx.user_id.as_str() {
                            return Err(KalamDbError::Unauthorized(format!(
                                "Insufficient privileges to access user table '{}.{}'",
                                ns.as_str(),
                                tbl.as_str()
                            )));
                        }
                        */
                    } else if matches!(def.table_type, TableType::System) {
                        // Enforce system table restrictions
                        // Only admins can access system tables directly
                        let is_admin = matches!(
                            exec_ctx.user_role,
                            kalamdb_commons::Role::System | kalamdb_commons::Role::Dba
                        );
                        if !is_admin {
                            return Err(KalamDbError::Unauthorized(format!(
                                "Insufficient privileges to access system table '{}.{}'",
                                ns.as_str(),
                                tbl.as_str()
                            )));
                        }
                    }
                }
            }
            _ => {
                for input in plan.inputs() {
                    self.check_plan_permissions(input, exec_ctx)?;
                }
            }
        }
        Ok(())
    }
}
