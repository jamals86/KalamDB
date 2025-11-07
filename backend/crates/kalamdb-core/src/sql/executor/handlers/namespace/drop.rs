//! Typed DDL handler for DROP NAMESPACE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::NamespaceId;
use kalamdb_sql::ddl::DropNamespaceStatement;
use std::sync::Arc;

/// Typed handler for DROP NAMESPACE statements
pub struct DropNamespaceHandler {
    app_context: Arc<AppContext>,
}

impl DropNamespaceHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropNamespaceStatement> for DropNamespaceHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        statement: DropNamespaceStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let namespaces_provider = self.app_context.system_tables().namespaces();
        let name = statement.name.as_str();
        let namespace_id = NamespaceId::new(name);

        // Check if namespace exists
        let namespace = match namespaces_provider.get_namespace(&namespace_id)? {
            Some(ns) => ns,
            None => {
                if statement.if_exists {
                    let message = format!("Namespace '{}' does not exist", name);
                    return Ok(ExecutionResult::Success { message });
                } else {
                    return Err(KalamDbError::NotFound(format!(
                        "Namespace '{}' not found",
                        name
                    )));
                }
            }
        };

        // Check if namespace has tables
        if !namespace.can_delete() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot drop namespace '{}': namespace contains {} table(s). Drop all tables first.",
                name, namespace.table_count
            )));
        }

        // Delete namespace via provider
        namespaces_provider.delete_namespace(&namespace_id)?;

        let message = format!("Namespace '{}' dropped successfully", name);
        Ok(ExecutionResult::Success { message })
    }

    async fn check_authorization(
        &self,
        _statement: &DropNamespaceStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop namespaces. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}
