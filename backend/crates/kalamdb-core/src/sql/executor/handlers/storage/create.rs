//! Typed DDL handler for CREATE STORAGE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::helpers::storage::ensure_filesystem_directory;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::{StorageId, StorageType};
use kalamdb_sql::ddl::CreateStorageStatement;
use std::sync::Arc;

/// Typed handler for CREATE STORAGE statements
pub struct CreateStorageHandler {
    app_context: Arc<AppContext>,
}

impl CreateStorageHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateStorageStatement> for CreateStorageHandler {
    async fn execute(
        &self,
        statement: CreateStorageStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Extract providers from AppContext
        let storages_provider = self.app_context.system_tables().storages();
        let storage_registry = self.app_context.storage_registry();

        // Check if storage already exists
        let storage_id = StorageId::from(statement.storage_id.as_str());
        if storages_provider
            .get_storage_by_id(&storage_id)
            .map_err(|e| KalamDbError::Other(format!("Failed to check storage: {}", e)))?
            .is_some()
        {
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' already exists",
                statement.storage_id
            )));
        }

        // Validate templates using StorageRegistry
        if !statement.shared_tables_template.is_empty() {
            storage_registry.validate_template(&statement.shared_tables_template, false)?;
        }
        if !statement.user_tables_template.is_empty() {
            storage_registry.validate_template(&statement.user_tables_template, true)?;
        }

        // Filesystem storages must eagerly create their base directories
        if statement.storage_type == StorageType::Filesystem {
            ensure_filesystem_directory(&statement.base_directory)?;
        }

        // Validate credentials JSON (if provided)
        let normalized_credentials = if let Some(raw) = statement.credentials.as_ref() {
            let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
                KalamDbError::InvalidOperation(format!("Invalid credentials JSON: {}", e))
            })?;

            if !value.is_object() {
                return Err(KalamDbError::InvalidOperation(
                    "Credentials must be a JSON object".to_string(),
                ));
            }

            Some(serde_json::to_string(&value).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to normalize credentials JSON: {}",
                    e
                ))
            })?)
        } else {
            None
        };

        // Create storage record
        let storage = kalamdb_commons::system::Storage {
            storage_id: statement.storage_id.clone(),
            storage_name: statement.storage_name,
            description: statement.description,
            storage_type: statement.storage_type.to_string(),
            base_directory: statement.base_directory,
            credentials: normalized_credentials,
            shared_tables_template: statement.shared_tables_template,
            user_tables_template: statement.user_tables_template,
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        // Insert into system.storages
        storages_provider
            .insert_storage(storage)
            .map_err(|e| KalamDbError::Other(format!("Failed to create storage: {}", e)))?;

        Ok(ExecutionResult::Success {
            message: format!("Storage '{}' created successfully", statement.storage_id),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &CreateStorageStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to create storage. DBA or System role required."
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_session;
    use datafusion::prelude::SessionContext;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::{Role, StorageId};

    fn create_test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), role, create_test_session())
    }

    #[tokio::test]
    async fn test_create_storage_authorization() {
        let app_ctx = AppContext::get();
        let handler = CreateStorageHandler::new(app_ctx);
        let stmt = CreateStorageStatement {
            storage_id: StorageId::new("test_storage"),
            storage_name: "Test Storage".to_string(),
            description: None,
            storage_type: kalamdb_commons::models::StorageType::from("local"),
            base_directory: "/tmp/storage".to_string(),
            credentials: None,
            shared_tables_template: String::new(),
            user_tables_template: String::new(),
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
    async fn test_create_storage_success() {
        let app_ctx = AppContext::get();
        let handler = CreateStorageHandler::new(app_ctx);
        let storage_id = format!("test_storage_{}", chrono::Utc::now().timestamp_millis());
        let stmt = CreateStorageStatement {
            storage_id: StorageId::from(storage_id.as_str()),
            storage_name: "Test Storage".to_string(),
            description: Some("Test description".to_string()),
            storage_type: kalamdb_commons::models::StorageType::from("local"),
            base_directory: "/tmp/test".to_string(),
            credentials: None,
            shared_tables_template: String::new(),
            user_tables_template: String::new(),
        };
        let ctx = create_test_context(Role::System);
        let session = SessionContext::new();

        let result = handler.execute(stmt, vec![], &ctx).await;

        assert!(result.is_ok());
        if let Ok(ExecutionResult::Success { message }) = result {
            assert!(message.contains(&storage_id));
        }
    }

    #[tokio::test]
    async fn test_create_storage_duplicate() {
        let app_ctx = AppContext::get();
        let handler = CreateStorageHandler::new(app_ctx);
        let storage_id = format!("test_dup_{}", chrono::Utc::now().timestamp_millis());
        let stmt = CreateStorageStatement {
            storage_id: StorageId::from(storage_id.as_str()),
            storage_name: "Test Duplicate".to_string(),
            description: None,
            storage_type: kalamdb_commons::models::StorageType::from("local"),
            base_directory: "/tmp/test".to_string(),
            credentials: None,
            shared_tables_template: String::new(),
            user_tables_template: String::new(),
        };
        let ctx = create_test_context(Role::System);
        let session = SessionContext::new();

        // First creation should succeed
        let result1 = handler.execute(stmt.clone(), vec![], &ctx).await;
        assert!(result1.is_ok());

        // Second creation should fail
        let result2 = handler.execute(stmt, vec![], &ctx).await;
        assert!(result2.is_err());
    }
}
