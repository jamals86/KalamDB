//! Typed DDL handler for ALTER STORAGE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::StorageId;
use kalamdb_sql::ddl::AlterStorageStatement;
use std::sync::Arc;

/// Typed handler for ALTER STORAGE statements
pub struct AlterStorageHandler {
    app_context: Arc<AppContext>,
}

impl AlterStorageHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<AlterStorageStatement> for AlterStorageHandler {
    async fn execute(
        &self,
        _session: &SessionContext,
        statement: AlterStorageStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let storages_provider = self.app_context.system_tables().storages();
        let storage_registry = self.app_context.storage_registry();

        // Get existing storage
        let storage_id = statement.storage_id;
        let mut storage = storages_provider
            .get_storage_by_id(&storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to get storage: {}", e)))?
            .ok_or_else(|| {
                KalamDbError::InvalidOperation(format!("Storage '{}' not found", statement.storage_id))
            })?;

        // Update fields if provided
        if let Some(name) = statement.storage_name {
            storage.storage_name = name;
        }

        if let Some(desc) = statement.description {
            storage.description = Some(desc);
        }

        if let Some(shared_template) = statement.shared_tables_template {
            // Validate template before updating
            if !shared_template.is_empty() {
                storage_registry.validate_template(&shared_template, false)?;
            }
            storage.shared_tables_template = shared_template;
        }

        if let Some(user_template) = statement.user_tables_template {
            // Validate template before updating
            if !user_template.is_empty() {
                storage_registry.validate_template(&user_template, true)?;
            }
            storage.user_tables_template = user_template;
        }

        // Update timestamp
        storage.updated_at = chrono::Utc::now().timestamp();

        // Save updated storage
        storages_provider
            .update_storage(storage)
            .map_err(|e| KalamDbError::Other(format!("Failed to update storage: {}", e)))?;

        Ok(ExecutionResult::Success {
            message: format!("Storage '{}' altered successfully", statement.storage_id),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &AlterStorageStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to alter storage. DBA or System role required."
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

    fn create_test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), role)
    }

    #[tokio::test]
    async fn test_alter_storage_authorization() {
        let app_ctx = AppContext::get();
        let handler = AlterStorageHandler::new(app_ctx);
        let stmt = AlterStorageStatement {
            storage_id: "test_storage".to_string(),
            storage_name: Some("Updated Storage".to_string()),
            description: None,
            shared_tables_template: None,
            user_tables_template: None,
        };
        
        // User role should be denied
        let user_ctx = create_test_context(Role::User);
        let result = handler.check_authorization(&stmt, &user_ctx).await;
        assert!(result.is_err());
        
        // DBA role should be allowed
        let dba_ctx = create_test_context(Role::Dba);
        let result = handler.check_authorization(&stmt, &dba_ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_alter_storage_success() {
        let app_ctx = AppContext::get();
        
        // First create a storage to alter
        let storages_provider = app_ctx.system_tables().storages();
        let storage_id = format!("test_alter_{}", chrono::Utc::now().timestamp_millis());
        let storage = kalamdb_commons::system_tables::Storage {
            storage_id: StorageId::from(storage_id.as_str()),
            storage_name: "Original Name".to_string(),
            description: None,
            storage_type: "local".to_string(),
            base_directory: "/tmp/test".to_string(),
            credentials: None,
            shared_tables_template: String::new(),
            user_tables_template: String::new(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };
        storages_provider.insert_storage(storage).unwrap();

        // Now alter it
        let handler = AlterStorageHandler::new(app_ctx);
        let stmt = AlterStorageStatement {
            storage_id: storage_id.clone(),
            storage_name: Some("Updated Name".to_string()),
            description: Some("New description".to_string()),
            shared_tables_template: None,
            user_tables_template: None,
        };
        let ctx = create_test_context(Role::System);
        let session = SessionContext::new();

        let result = handler.execute(&session, stmt, vec![], &ctx).await;
        
        assert!(result.is_ok());
        if let Ok(ExecutionResult::Success { message }) = result {
            assert!(message.contains(&storage_id));
        }

        // Verify the changes
        let updated = storages_provider.get_storage_by_id(&StorageId::from(storage_id.as_str())).unwrap();
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert_eq!(updated.storage_name, "Updated Name");
        assert_eq!(updated.description, Some("New description".to_string()));
    }
}
