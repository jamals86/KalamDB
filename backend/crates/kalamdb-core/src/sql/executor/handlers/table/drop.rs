//! Typed DDL handler for DROP TABLE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableType;
use kalamdb_sql::ddl::DropTableStatement;
use std::sync::Arc;

/// Typed handler for DROP TABLE statements
pub struct DropTableHandler {
    app_context: Arc<AppContext>,
}

impl DropTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropTableStatement> for DropTableHandler {
    async fn execute(
        &self,
        statement: DropTableStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let table_id = TableId::from_strings(
            statement.namespace_id.as_str(),
            statement.table_name.as_str(),
        );

        // RBAC: authorize based on actual table type if exists
        let registry = self.app_context.schema_registry();
        let actual_type = match registry.get_table_definition(&table_id)? {
            Some(def) => def.table_type,
            None => TableType::from(statement.table_type),
        };
        let is_owner = false;
        if !crate::auth::rbac::can_delete_table(context.user_role, actual_type, is_owner) {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop this table".to_string(),
            ));
        }

        // Check existence via system.tables provider (for IF EXISTS behavior)
        let tables = self.app_context.system_tables().tables();
        let exists = tables.get_table_by_id(&table_id)?.is_some();
        if !exists {
            if statement.if_exists {
                return Ok(ExecutionResult::Success { message: format!(
                    "Table {}.{} does not exist (skipped)",
                    statement.namespace_id.as_str(),
                    statement.table_name.as_str()
                )});
            } else {
                return Err(KalamDbError::NotFound(format!(
                    "Table '{}' not found in namespace '{}'",
                    statement.table_name.as_str(),
                    statement.namespace_id.as_str()
                )));
            }
        }

        // TODO: Check active live queries/subscriptions before dropping (Phase 9 integration)

        // Remove definition via SchemaRegistry (delete-through) → invalidates cache
        registry.delete_table_definition(&table_id)?;

        Ok(ExecutionResult::Success {
            message: format!(
                "Table {}.{} dropped successfully",
                statement.namespace_id.as_str(),
                statement.table_name.as_str()
            ),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &DropTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Coarse auth gate (fine-grained check performed in execute using actual table type)
        if context.is_system() || context.is_admin() {
            return Ok(());
        }
        // Allow users to attempt; execute() will enforce per-table RBAC
        Ok(())
    }
}
