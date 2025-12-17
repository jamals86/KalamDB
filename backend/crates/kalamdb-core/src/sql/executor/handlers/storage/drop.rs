//! Typed DDL handler for DROP STORAGE statements

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::error_extensions::KalamDbResultExt;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_sql::ddl::DropStorageStatement;
use std::sync::Arc;

/// Typed handler for DROP STORAGE statements
pub struct DropStorageHandler {
    app_context: Arc<AppContext>,
}

impl DropStorageHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropStorageStatement> for DropStorageHandler {
    async fn execute(
        &self,
        statement: DropStorageStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let storages_provider = self.app_context.system_tables().storages();
        let tables_provider = self.app_context.system_tables().tables();

        let storage_id = statement.storage_id.clone();

        // Check if storage exists
        let storage = storages_provider
            .get_storage_by_id(&storage_id)
            .into_kalamdb_error("Failed to get storage")?;

        if storage.is_none() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Storage '{}' not found",
                statement.storage_id.as_str()
            )));
        }

        // Check if any tables are using this storage
        let all_tables = tables_provider
            .list_tables()
            .into_kalamdb_error("Failed to check tables")?;

        let tables_using_storage: Vec<_> = all_tables
            .iter()
            .filter(|t| {
                // Check if table uses this storage (compare storage_id from table options)
                match &t.table_options {
                    kalamdb_commons::schemas::TableOptions::User(opts) => {
                        opts.storage_id == storage_id
                    }
                    kalamdb_commons::schemas::TableOptions::Shared(opts) => {
                        opts.storage_id == storage_id
                    }
                    _ => false, // STREAM and SYSTEM tables don't have storage_id
                }
            })
            .collect();

        if !tables_using_storage.is_empty() {
            return Err(KalamDbError::InvalidOperation(format!(
                "Cannot drop storage '{}': {} table(s) still using it",
                statement.storage_id,
                tables_using_storage.len()
            )));
        }

        // Delete the storage
        storages_provider
            .delete_storage(&storage_id)
            .into_kalamdb_error("Failed to drop storage")?;

        // Invalidate storage cache
        self.app_context.storage_registry().invalidate(&storage_id);

        Ok(ExecutionResult::Success {
            message: format!("Storage '{}' dropped successfully", statement.storage_id),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &DropStorageStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "Insufficient privileges to drop storage. DBA or System role required.".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_session;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::{Role, StorageId};

    fn create_test_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(UserId::new("test_user"), role, create_test_session())
    }

    #[tokio::test]
    async fn test_drop_storage_authorization() {
        let app_ctx = AppContext::get();
        let handler = DropStorageHandler::new(app_ctx);
        let stmt = DropStorageStatement {
            storage_id: StorageId::new("test_storage"),
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
    async fn test_drop_storage_success() {
        let app_ctx = AppContext::get();
        let storages_provider = app_ctx.system_tables().storages();

        // Create a test storage
        let storage_id = format!("test_drop_{}", chrono::Utc::now().timestamp_millis());
        let storage = kalamdb_commons::system::Storage {
            storage_id: StorageId::from(storage_id.as_str()),
            storage_name: "Test Drop".to_string(),
            description: None,
            storage_type: kalamdb_commons::models::StorageType::Filesystem,
            base_directory: "/tmp/test".to_string(),
            credentials: None,
            config_json: None,
            shared_tables_template: String::new(),
            user_tables_template: String::new(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };
        storages_provider.insert_storage(storage).unwrap();

        // Drop it
        let handler = DropStorageHandler::new(app_ctx);
        let stmt = DropStorageStatement {
            storage_id: StorageId::from(storage_id.as_str()),
        };
        let ctx = create_test_context(Role::System);
        let result = handler.execute(stmt, vec![], &ctx).await;

        assert!(result.is_ok());
        if let Ok(ExecutionResult::Success { message }) = result {
            assert!(message.contains(&storage_id));
        }

        // Verify deletion
        let deleted = storages_provider
            .get_storage_by_id(&StorageId::from(storage_id.as_str()))
            .unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_drop_storage_not_found() {
        let app_ctx = AppContext::get();
        let handler = DropStorageHandler::new(app_ctx);
        let stmt = DropStorageStatement {
            storage_id: StorageId::from("nonexistent_storage"),
        };
        let ctx = create_test_context(Role::System);
        // let session = SessionContext::new();

        let result = handler.execute(stmt, vec![], &ctx).await;

        assert!(result.is_err());
    }
}
