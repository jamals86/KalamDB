//! DDL Executor - CREATE/ALTER/DROP TABLE operations
//!
//! This is the SINGLE place where table mutations happen.

use std::sync::Arc;

use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableType;

use crate::app_context::AppContext;
use crate::applier::ApplierError;
use crate::schema_registry::CachedTableData;
use crate::sql::executor::helpers::table_registration;

/// Executor for DDL (Data Definition Language) operations
pub struct DdlExecutor {
    app_context: Arc<AppContext>,
}

impl DdlExecutor {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }

    fn clear_plan_cache_if_available(&self) {
        if let Some(sql_executor) = self.app_context.try_sql_executor() {
            // Important for cluster followers: DDL is applied via Raft applier,
            // bypassing SqlExecutor's normal post-DDL cache invalidation.
            sql_executor.clear_plan_cache();
            log::debug!("DdlExecutor: Cleared SQL plan cache after DDL");
        }
    }

    /// Execute CREATE TABLE
    ///
    /// This performs ALL the steps for table creation:
    /// 1. Persist to system.tables
    /// 2. Prime schema cache
    /// 3. Register DataFusion provider
    /// 4. Emit TableCreated event
    pub async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Creating {} table {}", table_type, table_id.full_name());

        // 1. Persist to system.tables
        self.app_context
            .system_tables()
            .tables()
            .create_table(table_id, table_def)
            .map_err(|e| ApplierError::Execution(format!("Failed to create table: {}", e)))?;

        // 2. Prime the schema cache
        self.prime_schema_cache(table_id, table_type, table_def)?;

        // 3. Register the DataFusion provider
        self.register_table_provider(table_id, table_type, table_def)?;

        // 4. Invalidate cached plans across this node
        self.clear_plan_cache_if_available();

        Ok(format!(
            "{} table {}.{} created successfully",
            table_type,
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        ))
    }

    /// Execute ALTER TABLE
    pub async fn alter_table(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
        old_version: u32,
    ) -> Result<String, ApplierError> {
        if log::log_enabled!(log::Level::Debug) {
            log::debug!(
                "CommandExecutorImpl: Altering table {} (version {} -> {}). New columns: {:?}",
                table_id.full_name(),
                old_version,
                table_def.schema_version,
                table_def.columns.iter().map(|c| c.column_name.as_str()).collect::<Vec<_>>()
            );
        }

        // 1. Update in system.tables
        self.app_context
            .system_tables()
            .tables()
            .update_table(table_id, table_def)
            .map_err(|e| ApplierError::Execution(format!("Failed to alter table: {}", e)))?;
        log::debug!("CommandExecutorImpl: Updated system.tables for {}", table_id.full_name());

        // 2. Update schema cache
        let table_type = table_def.table_type;
        self.prime_schema_cache(table_id, table_type, table_def)?;
        log::debug!("CommandExecutorImpl: Primed schema cache for {}", table_id.full_name());

        // 3. Re-register provider with new schema
        self.register_table_provider(table_id, table_type, table_def)?;
        log::debug!("CommandExecutorImpl: Re-registered provider for {}", table_id.full_name());

        // 4. Invalidate cached plans across this node
        self.clear_plan_cache_if_available();

        // 4. Verify the schema was updated correctly
        if let Some(cached) = self.app_context.schema_registry().get(table_id) {
            if let Ok(schema) = cached.arrow_schema() {
                log::debug!(
                    "CommandExecutorImpl: ALTER TABLE {} complete - Arrow schema now has {} fields: {:?}",
                    table_id.full_name(),
                    schema.fields().len(),
                    schema.fields().iter().map(|f| f.name()).collect::<Vec<_>>()
                );
            }
        }

        Ok(format!(
            "Table {}.{} altered successfully (version {} -> {})",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            old_version,
            table_def.schema_version
        ))
    }

    /// Execute DROP TABLE
    pub async fn drop_table(&self, table_id: &TableId) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Dropping table {}", table_id.full_name());

        // 1. Remove from system.tables
        self.app_context
            .system_tables()
            .tables()
            .delete_table(table_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to drop table: {}", e)))?;

        // 2. Remove from schema cache
        self.app_context.schema_registry().invalidate_all_versions(table_id);

        // 3. Deregister DataFusion provider
        self.app_context.schema_registry().remove_provider(table_id).map_err(|e| {
            ApplierError::Execution(format!("Failed to deregister provider: {}", e))
        })?;

        // 4. Invalidate cached plans across this node
        self.clear_plan_cache_if_available();

        Ok(format!(
            "Table {}.{} dropped successfully",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        ))
    }

    /// Prime the schema cache with table definition
    fn prime_schema_cache(
        &self,
        table_id: &TableId,
        _table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<(), ApplierError> {
        use kalamdb_commons::models::schemas::TableOptions;

        let table_def_arc = Arc::new(table_def.clone());
        let schema_registry = self.app_context.schema_registry();

        let storage_id_opt = match &table_def_arc.table_options {
            TableOptions::User(opts) => Some(opts.storage_id.clone()),
            TableOptions::Shared(opts) => Some(opts.storage_id.clone()),
            TableOptions::Stream(_) => None,
            TableOptions::System(_) => return Ok(()),
        };

        let data = CachedTableData::with_storage_config(table_def_arc, storage_id_opt);
        schema_registry.insert(table_id.clone(), Arc::new(data));

        Ok(())
    }

    /// Register table provider with DataFusion
    fn register_table_provider(
        &self,
        table_id: &TableId,
        table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<(), ApplierError> {
        let arrow_schema = self
            .app_context
            .schema_registry()
            .get_arrow_schema(self.app_context.as_ref(), table_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to get Arrow schema: {}", e)))?;

        match table_type {
            TableType::User => {
                table_registration::register_user_table_provider(
                    &self.app_context,
                    table_id,
                    arrow_schema,
                )
                .map_err(|e| {
                    ApplierError::Execution(format!("Failed to register user table: {}", e))
                })?;
            },
            TableType::Shared => {
                table_registration::register_shared_table_provider(
                    &self.app_context,
                    table_id,
                    arrow_schema,
                )
                .map_err(|e| {
                    ApplierError::Execution(format!("Failed to register shared table: {}", e))
                })?;
            },
            TableType::Stream => {
                use kalamdb_commons::models::schemas::TableOptions;
                let ttl = match &table_def.table_options {
                    TableOptions::Stream(opts) => Some(opts.ttl_seconds),
                    _ => Some(10),
                };
                table_registration::register_stream_table_provider(
                    &self.app_context,
                    table_id,
                    arrow_schema,
                    ttl,
                )
                .map_err(|e| {
                    ApplierError::Execution(format!("Failed to register stream table: {}", e))
                })?;
            },
            TableType::System => {
                // System tables are registered differently
            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::DdlExecutor;
    use crate::sql::executor::models::ExecutionContext;
    use crate::sql::executor::SqlExecutor;
    use crate::test_helpers::test_app_context_simple;
    use kalamdb_commons::models::datatypes::KalamDataType;
    use kalamdb_commons::models::schemas::{ColumnDefinition, TableDefinition, TableOptions};
    use kalamdb_commons::models::{NamespaceId, TableId, TableName};
    use kalamdb_commons::schemas::{ColumnDefault, TableType};
    use kalamdb_commons::{Role, UserId};
    use std::sync::Arc;

    #[tokio::test]
    async fn ddl_applied_via_applier_clears_plan_cache() {
        let app_ctx = test_app_context_simple();

        // Register SqlExecutor into AppContext so DdlExecutor can clear plan cache.
        let sql_executor = Arc::new(SqlExecutor::new(app_ctx.clone(), false));
        app_ctx.set_sql_executor(sql_executor.clone());

        // Build a minimal STREAM table definition (avoids storage registry requirements).
        let namespace = NamespaceId::new("test_ns");
        let table_name = TableName::new("test_table");
        let table_id = TableId::new(namespace.clone(), table_name.clone());

        let id_col = ColumnDefinition::new(
            1,
            "id".to_string(),
            1,
            KalamDataType::BigInt,
            false,
            true,
            false,
            ColumnDefault::None,
            None,
        );

        let mut table_def = TableDefinition::new(
            namespace.clone(),
            table_name.clone(),
            TableType::Stream,
            vec![id_col],
            TableOptions::stream(3600),
            None,
        )
        .expect("Failed to create TableDefinition");

        // Inject system columns (_seq, _deleted) to match runtime tables.
        app_ctx
            .system_columns_service()
            .add_system_columns(&mut table_def)
            .expect("Failed to add system columns");

        // Create table via DDL executor (registers provider).
        let ddl = DdlExecutor::new(app_ctx.clone());
        ddl.create_table(&table_id, TableType::Stream, &table_def)
            .await
            .expect("CREATE TABLE failed");

        // Execute a SELECT to populate the plan cache on this node.
        let exec_ctx = ExecutionContext::new(
            UserId::from("test_user"),
            Role::User,
            app_ctx.base_session_context(),
        );

        let _ = sql_executor
            .execute("SELECT * FROM test_ns.test_table", &exec_ctx, vec![])
            .await
            .expect("SELECT failed");

        assert!(sql_executor.plan_cache_len() > 0, "Expected plan cache to be populated");

        // Now apply an ALTER TABLE via the applier path (simulates follower replication).
        let mut altered = table_def.clone();
        let new_col = ColumnDefinition::new(
            999,
            "col1".to_string(),
            (altered.columns.len() + 1) as u32,
            KalamDataType::Text,
            true,
            false,
            false,
            ColumnDefault::None,
            None,
        );
        altered.columns.push(new_col);
        altered.increment_version();

        ddl.alter_table(&table_id, &altered, 1).await.expect("ALTER TABLE failed");

        assert_eq!(
            sql_executor.plan_cache_len(),
            0,
            "Expected plan cache to be cleared after ALTER TABLE applied via applier"
        );
    }
}
