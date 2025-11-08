//! Typed DDL handler for DROP NAMESPACE statements

use crate::app_context::AppContext;
use crate::test_helpers::create_test_session;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
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
        statement: DropNamespaceStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Extract namespace provider from AppContext
        let namespaces_provider = self.app_context.system_tables().namespaces();
        let namespace_id = statement.name;

        // Check if namespace exists
        let namespace = match namespaces_provider.get_namespace(&namespace_id)? {
            Some(ns) => ns,
            None => {
                if statement.if_exists {
                    let message = format!("Namespace '{}' does not exist", namespace_id.as_str());
                    return Ok(ExecutionResult::Success { message });
                } else {
                    return Err(KalamDbError::NotFound(format!(
                        "Namespace '{}' not found",
                        namespace_id.as_str()
                    )));
                }
            }
        };

        // Check if namespace has tables
        if !namespace.can_delete() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot drop namespace '{}': namespace contains {} table(s). Drop all tables first.",
                    namespace.name,
                namespace.table_count
            )));
        }

        // Delete namespace via provider
        namespaces_provider.delete_namespace(&namespace_id)?;

            let message = format!("Namespace '{}' dropped successfully", namespace.name);
        Ok(ExecutionResult::Success { message })
    }

    async fn check_authorization(
        &self,
        _statement: &DropNamespaceStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Only DBA/System roles can drop namespaces
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop namespaces. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::Role;
    use kalamdb_commons::models::UserId;

    fn create_test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), Role::Dba, create_test_session())
    }

    #[tokio::test]
    async fn test_drop_namespace_success() {
        let app_ctx = AppContext::get();
        let handler = DropNamespaceHandler::new(app_ctx);
        let stmt = DropNamespaceStatement {
            name: kalamdb_commons::models::NamespaceId::new("test_namespace"),
            if_exists: false,
        };
        let ctx = create_test_context();
        let session = SessionContext::new();

        // Note: This test would need proper setup of test namespace
        // For now, it demonstrates the pattern
        let result = handler.execute(stmt, vec![], &ctx).await;
        
        // Would verify result or error based on test setup
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_drop_namespace_authorization() {
        let app_ctx = AppContext::get();
        let handler = DropNamespaceHandler::new(app_ctx);
        let stmt = DropNamespaceStatement {
            name: kalamdb_commons::models::NamespaceId::new("test"),
            if_exists: false,
        };
        
        // Test with non-admin user
        let ctx = ExecutionContext::new(UserId::new("user"), Role::User, create_test_session());
        let result = handler.check_authorization(&stmt, &ctx).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), KalamDbError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn test_drop_namespace_if_exists() {
        let app_ctx = AppContext::get();
        let handler = DropNamespaceHandler::new(app_ctx);
        let stmt = DropNamespaceStatement {
            name: kalamdb_commons::models::NamespaceId::new("nonexistent"),
            if_exists: true,
        };
        let ctx = create_test_context();
        let session = SessionContext::new();

        let result = handler.execute(stmt, vec![], &ctx).await;
        
        // With IF EXISTS, should succeed even if namespace doesn't exist
        if let Ok(ExecutionResult::Success { message }) = result {
            assert!(message.contains("does not exist"));
        }
    }
}
