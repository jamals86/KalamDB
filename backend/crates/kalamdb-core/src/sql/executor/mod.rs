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
use datafusion::prelude::SessionContext;
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

    /// Execute SELECT/INSERT/DELETE via DataFusion with per-user session
    async fn execute_via_datafusion(
        &self,
        sql: &str,
        params: Vec<ScalarValue>,
        exec_ctx: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // TODO: Implement parameter binding once we have the full query handler
        // DataFusion supports params via LogicalPlan manipulation, not DataFrame.with_params()
        if !params.is_empty() {
            return Err(KalamDbError::InvalidOperation(
                "Parameter binding not yet implemented (will be added with query handler)".to_string()
            ));
        }

        // Create per-request SessionContext with user_id injected
        // Tables are already registered in base session (done once on CREATE TABLE or server startup)
        // The user_id injection allows UserTableProvider::scan() to filter by current user
        let session = exec_ctx.create_session_with_user();

        // Parse SQL and get DataFrame (with detailed logging on failure)
        let df = match session.sql(sql).await {
            Ok(df) => df,
            Err(e) => {
                // Log planning failure with rich context
                log::error!(
                    target: "sql::plan",
                    "❌ SQL planning failed | sql='{}' | user='{}' | role='{:?}' | error='{}'",
                    sql,
                    exec_ctx.user_id.as_str(),
                    exec_ctx.user_role,
                    e
                );
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

        // Return batches with row count
        Ok(ExecutionResult::Rows { batches, row_count })
    }

    /// Load existing tables from system.tables and register providers
    ///
    /// Called during server startup to restore table access after restart.
    /// Scans system.tables and creates/registers:
    /// - UserTableShared instances for USER tables
    /// - SharedTableProvider instances for SHARED tables  
    /// - StreamTableProvider instances for STREAM tables
    ///
    /// # Returns
    /// Ok on success, error if table loading fails
    pub async fn load_existing_tables(
        &self
    ) -> Result<(), KalamDbError> {
        use crate::tables::base_table_provider::UserTableShared;
        use crate::tables::user_tables::new_user_table_store;
        use kalamdb_commons::schemas::TableType;

        let app_context = &self.app_context;
        let tables_provider = app_context.system_tables().tables();
        let schema_registry = app_context.schema_registry();

        // Scan all tables from system.tables
        let all_tables_batch = tables_provider.scan_all_tables()?;
        
        if all_tables_batch.num_rows() == 0 {
            log::info!("No existing tables to load");
            return Ok(());
        }

        // Extract table IDs and types from batch
        use datafusion::arrow::array::{Array, StringArray};

        // system.tables schema now uses 'namespace_id' consistently
        let namespace_array = all_tables_batch
            .column_by_name("namespace_id")
            .ok_or_else(|| KalamDbError::Other("Missing namespace_id column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("namespace_id is not StringArray".to_string()))?;

        let table_name_array = all_tables_batch
            .column_by_name("table_name")
            .ok_or_else(|| KalamDbError::Other("Missing table_name column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("table_name is not StringArray".to_string()))?;

        let table_type_array = all_tables_batch
            .column_by_name("table_type")
            .ok_or_else(|| KalamDbError::Other("Missing table_type column".to_string()))?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| KalamDbError::Other("table_type is not StringArray".to_string()))?;

        let mut user_count = 0;
        let mut shared_count = 0;
        let mut stream_count = 0;
        let mut system_count = 0;

        // Iterate through all tables and register providers
        for i in 0..all_tables_batch.num_rows() {
            let namespace_id = namespace_array.value(i);
            let table_name = table_name_array.value(i);
            let table_type_str = table_type_array.value(i);

            let table_id = kalamdb_commons::models::TableId::from_strings(namespace_id, table_name);
            
            // Parse table type
            let table_type = match table_type_str.to_uppercase().as_str() {
                "USER" => TableType::User,
                "SHARED" => TableType::Shared,
                "STREAM" => TableType::Stream,
                "SYSTEM" => {
                    system_count += 1;
                    continue; // System tables already registered in AppContext
                }
                _ => {
                    log::warn!("Unknown table type '{}' for {}.{}", table_type_str, namespace_id, table_name);
                    continue;
                }
            };

            // Get table definition with schema
            let table_def = match schema_registry.get_table_definition(&table_id)? {
                Some(def) => def,
                None => {
                    log::warn!("Table definition not found for {}.{}", namespace_id, table_name);
                    continue;
                }
            };

            // Convert to Arrow schema
            let arrow_schema = match table_def.to_arrow_schema() {
                Ok(schema) => schema,
                Err(e) => {
                    log::error!("Failed to convert table definition to Arrow schema for {}.{}: {}", 
                               namespace_id, table_name, e);
                    continue;
                }
            };

            // Ensure CachedTableData exists in unified schema cache (may have been evicted or not yet inserted on legacy tables)
            if schema_registry.get(&table_id).is_none() {
                use crate::schema_registry::CachedTableData;
                use kalamdb_commons::models::StorageId;
                // Determine storage id (default to local)
                let storage_id = StorageId::local();
                // Resolve path template for this table type
                let template = match table_type {
                    TableType::User => schema_registry
                        .resolve_storage_path_template(&table_id.namespace_id(), &table_id.table_name(), TableType::User, &storage_id)
                        .unwrap_or_default(),
                    TableType::Shared => schema_registry
                        .resolve_storage_path_template(&table_id.namespace_id(), &table_id.table_name(), TableType::Shared, &storage_id)
                        .unwrap_or_default(),
                    TableType::Stream => String::new(),
                    TableType::System => String::new(),
                };
                let mut data = CachedTableData::new(Arc::clone(&table_def));
                data.storage_id = Some(storage_id);
                data.storage_path_template = template;
                schema_registry.insert(table_id.clone(), Arc::new(data));
                log::debug!("Primed schema cache for {}.{} (version {}), template set",
                    namespace_id, table_name, table_def.schema_version);
            }

            // Register provider based on type
            match table_type {
                TableType::User => {
                    // Create user table store
                    let user_table_store = Arc::new(new_user_table_store(
                        app_context.storage_backend(),
                        &table_id.namespace_id(),
                        &table_id.table_name(),
                    ));

                    // Create and register UserTableShared
                    let mut shared = UserTableShared::new(
                        Arc::new(table_id.clone()),
                        app_context.schema_registry(),
                        arrow_schema.clone(),
                        user_table_store,
                    );
                    // Attach LiveQueryManager so changes are published after restart
                    if let Some(shared_ref) = Arc::get_mut(&mut shared) {
                        shared_ref.attach_live_query_manager(app_context.live_query_manager());
                    }

                    // Create UserTableProvider and register in unified provider cache
                    let provider = crate::tables::user_tables::UserTableProvider::new(shared);
                    schema_registry.insert_provider(table_id.clone(), Arc::new(provider));
                    user_count += 1;
                }
                TableType::Shared => {
                    use crate::tables::shared_tables::{shared_table_store::new_shared_table_store, SharedTableProvider};
                    let shared_store = Arc::new(new_shared_table_store(
                        app_context.storage_backend(),
                        &table_id.namespace_id(),
                        &table_id.table_name(),
                    ));
                    let provider = SharedTableProvider::new(
                        Arc::new(table_id.clone()),
                        app_context.schema_registry(),
                        arrow_schema.clone(),
                        shared_store,
                    );
                    schema_registry.insert_provider(table_id.clone(), Arc::new(provider));
                    shared_count += 1;
                }
                TableType::Stream => {
                    use crate::tables::stream_tables::{stream_table_store::new_stream_table_store, StreamTableProvider};
                    let stream_store = Arc::new(new_stream_table_store(
                        &table_id.namespace_id(),
                        &table_id.table_name(),
                    ));
                    
                    // Extract TTL from table options
                    let retention_seconds = if let kalamdb_commons::schemas::TableOptions::Stream(stream_opts) = &table_def.table_options {
                        Some(stream_opts.ttl_seconds as u32)
                    } else {
                        None
                    };
                    
                    let provider = StreamTableProvider::new(
                        Arc::new(table_id.clone()),
                        app_context.schema_registry(),
                        stream_store,
                        retention_seconds,
                        false, // ephemeral default
                        None,  // max_buffer default
                    );
                    schema_registry.insert_provider(table_id.clone(), Arc::new(provider));
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

    /// Expose the shared `AppContext` for upcoming migrations.
    pub fn app_context(&self) -> &Arc<crate::app_context::AppContext> {
        &self.app_context
    }
}
