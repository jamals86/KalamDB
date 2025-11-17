//! Typed DDL handler for CREATE NAMESPACE statements
//!
//! This module demonstrates the TypedStatementHandler pattern where handlers
//! receive fully parsed AST structs instead of raw SQL strings.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::NamespaceId;
use kalamdb_commons::system::Namespace;
use kalamdb_sql::ddl::CreateNamespaceStatement;
use std::sync::Arc;

/// Typed handler for CREATE NAMESPACE statements
pub struct CreateNamespaceHandler {
    app_context: Arc<AppContext>,
}

impl CreateNamespaceHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateNamespaceStatement> for CreateNamespaceHandler {
    async fn execute(
        &self,
        statement: CreateNamespaceStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let namespaces_provider = self.app_context.system_tables().namespaces();
        let name = statement.name.as_str();

        // Validate namespace name
        Namespace::validate_name(name)?;

        // Check if namespace already exists
        let namespace_id = NamespaceId::new(name);
        let existing = namespaces_provider.get_namespace(&namespace_id)?;

        if existing.is_some() {
            if statement.if_not_exists {
                let message = format!("Namespace '{}' already exists", name);
                return Ok(ExecutionResult::Success { message });
            } else {
                return Err(KalamDbError::AlreadyExists(format!(
                    "Namespace '{}' already exists",
                    name
                )));
            }
        }

        // Create namespace entity
        let namespace = Namespace::new(name);

        // Insert namespace via provider
        namespaces_provider.create_namespace(namespace)?;

        let message = format!("Namespace '{}' created successfully", name);
        Ok(ExecutionResult::Success { message })
    }

    async fn check_authorization(
        &self,
        _statement: &CreateNamespaceStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Only DBA and System roles can create namespaces
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to create namespaces. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{create_test_session, init_test_app_context};
    use datafusion::prelude::SessionContext;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), Role::Dba, create_test_session())
    }

    #[tokio::test]
    async fn test_typed_create_namespace() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateNamespaceHandler::new(app_ctx);
        let session = SessionContext::new();
        let ctx = test_context();

        let stmt = CreateNamespaceStatement {
            name: NamespaceId::new("test_typed_ns"),
            if_not_exists: false,
        };

        let result = handler.execute(stmt, vec![], &ctx).await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionResult::Success { message } => {
                assert!(message.contains("test_typed_ns"));
                assert!(message.contains("created successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_typed_create_namespace_if_not_exists() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateNamespaceHandler::new(app_ctx);
        let session = SessionContext::new();
        let ctx = test_context();

        let stmt = CreateNamespaceStatement {
            name: NamespaceId::new("test_typed_ns_ine"),
            if_not_exists: true,
        };

        // First creation should succeed
        let result1 = handler.execute(stmt.clone(), vec![], &ctx).await;
        assert!(result1.is_ok());

        // Second creation with IF NOT EXISTS should also succeed
        let result2 = handler.execute(stmt, vec![], &ctx).await;
        assert!(result2.is_ok());

        match result2.unwrap() {
            ExecutionResult::Success { message } => assert!(message.contains("already exists")),
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_typed_authorization_check() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateNamespaceHandler::new(app_ctx);
        let user_ctx = ExecutionContext::new(
            UserId::from("regular_user"),
            Role::User,
            create_test_session(),
        );

        let stmt = CreateNamespaceStatement {
            name: NamespaceId::new("unauthorized_ns"),
            if_not_exists: false,
        };

        let auth_result = handler.check_authorization(&stmt, &user_ctx).await;
        assert!(auth_result.is_err());

        if let Err(KalamDbError::Unauthorized(msg)) = auth_result {
            assert!(msg.contains("Insufficient privileges"));
        } else {
            panic!("Expected Unauthorized error");
        }
    }
}
