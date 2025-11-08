//! Typed DDL handler for CREATE TABLE statements

use crate::app_context::AppContext;
use crate::test_helpers::create_test_session;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_sql::ddl::CreateTableStatement;
use std::sync::Arc;

/// Typed handler for CREATE TABLE statements (all table types: USER, SHARED, STREAM)
pub struct CreateTableHandler {
    app_context: Arc<AppContext>,
}

impl CreateTableHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateTableStatement> for CreateTableHandler {
    async fn execute(
        &self,
        statement: CreateTableStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        use crate::sql::executor::helpers::table_creation;

        // Delegate to helper function
        let message = table_creation::create_table(
            self.app_context.clone(),
            statement,
            context,
        )?;

        Ok(ExecutionResult::Success { message })
    }

    async fn check_authorization(
        &self,
        statement: &CreateTableStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // Authorization check is handled inside table_creation helpers
        // (they call can_create_table for each table type)
        // This allows unified error messages
        use crate::auth::rbac::can_create_table;
        
        if !can_create_table(context.user_role, statement.table_type) {
            return Err(KalamDbError::Unauthorized(format!(
                "Insufficient privileges to create {} tables",
                statement.table_type
            )));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::init_test_app_context;
    use kalamdb_commons::models::{NamespaceId, UserId};
    use kalamdb_commons::Role;
    use kalamdb_commons::schemas::TableType;
    use arrow::datatypes::{DataType, Field, Schema};

    fn create_test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), role, create_test_session())
    }

    fn create_test_statement(table_type: TableType) -> CreateTableStatement {
        let schema = Arc::new(Schema::new(vec![
            Arc::new(Field::new("name", DataType::Utf8, false)),
            Arc::new(Field::new("age", DataType::Int32, true)),
        ]));

        CreateTableStatement {
            namespace_id: NamespaceId::new("default"),
            table_name: format!("test_table_{}", chrono::Utc::now().timestamp_millis()).into(),
            table_type,
            schema,
                column_defaults: std::collections::HashMap::new(),
                primary_key_column: None,
                storage_id: None,
                use_user_storage: false,
                flush_policy: None,
                deleted_retention_hours: None,
                ttl_seconds: if table_type == TableType::Stream {
                    Some(3600)
                } else {
                    None
                },
            if_not_exists: false,
                access_level: None,
        }
    }

    #[tokio::test]
    async fn test_create_table_authorization_user() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateTableHandler::new(app_ctx);
        let stmt = create_test_statement(TableType::User);
        
        // User role can create USER tables
        let user_ctx = create_test_context(Role::User);
        let result = handler.check_authorization(&stmt, &user_ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_table_authorization_shared_denied() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateTableHandler::new(app_ctx);
        let stmt = create_test_statement(TableType::Shared);
        
        // User role CANNOT create SHARED tables
        let user_ctx = create_test_context(Role::User);
        let result = handler.check_authorization(&stmt, &user_ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_table_authorization_stream_dba() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        let handler = CreateTableHandler::new(app_ctx);
        let stmt = create_test_statement(TableType::Stream);
        
        // DBA role can create STREAM tables
        let dba_ctx = create_test_context(Role::Dba);
        let result = handler.check_authorization(&stmt, &dba_ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_user_table_success() {
        init_test_app_context();
        let app_ctx = AppContext::get();
        
        // Ensure default namespace exists
        let namespaces_provider = app_ctx.system_tables().namespaces();
        let namespace_id = NamespaceId::new("default");
        if namespaces_provider.get_namespace(&namespace_id).unwrap().is_none() {
            let namespace = kalamdb_commons::system::Namespace {
                namespace_id: namespace_id.clone(),
                name: "default".to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
                options: None,
                table_count: 0,
            };
            namespaces_provider.create_namespace(namespace).unwrap();
        }

        let handler = CreateTableHandler::new(app_ctx);
        let stmt = create_test_statement(TableType::User);
        let ctx = create_test_context(Role::User);
        let session = SessionContext::new();

        let result = handler.execute(stmt, vec![], &ctx).await;
        
        assert!(result.is_ok());
        if let Ok(ExecutionResult::Success { message }) = result {
            assert!(message.contains("created successfully"));
        }
    }

    #[tokio::test]
    async fn test_create_table_if_not_exists() {
        let app_ctx = AppContext::get();
        
        // Ensure default namespace exists
        let namespaces_provider = app_ctx.system_tables().namespaces();
        let namespace_id = NamespaceId::new("default");
        if namespaces_provider.get_namespace(&namespace_id).unwrap().is_none() {
            let namespace = kalamdb_commons::system::Namespace {
                namespace_id: namespace_id.clone(),
                name: "default".to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
                options: None,
                table_count: 0,
            };
            namespaces_provider.create_namespace(namespace).unwrap();
        }

        let handler = CreateTableHandler::new(app_ctx);
        let table_name = format!("test_ine_{}", chrono::Utc::now().timestamp_millis());
        
        let mut stmt = create_test_statement(TableType::User);
        stmt.table_name = table_name.clone().into();
        let ctx = create_test_context(Role::User);
        let session = SessionContext::new();

        // First creation should succeed
        let result1 = handler.execute(stmt.clone(), vec![], &ctx).await;
        assert!(result1.is_ok());

        // Second creation without IF NOT EXISTS should fail
        let result2 = handler.execute(stmt.clone(), vec![], &ctx).await;
        assert!(result2.is_err());

        // Third creation with IF NOT EXISTS should succeed with message
        let mut stmt_ine = stmt.clone();
        stmt_ine.if_not_exists = true;
        let result3 = handler.execute(stmt_ine, vec![], &ctx).await;
        assert!(result3.is_ok());
        if let Ok(ExecutionResult::Success { message }) = result3 {
            assert!(message.contains("already exists"));
        }
    }
}
