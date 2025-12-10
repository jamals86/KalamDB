//! Storage DDL Handlers
//!
//! Handlers for CREATE STORAGE statements.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult};
use datafusion::execution::context::SessionContext;
use kalamdb_commons::models::{StorageId, StorageType};
use kalamdb_sql::CreateStorageStatement;
use std::sync::Arc;

/// Execute CREATE STORAGE statement
///
/// Registers a new storage backend (filesystem, S3, etc.) for storing table data.
/// Storage backends define where and how table data is physically stored.
///
/// # Arguments
/// * `app_context` - Application context with system table providers and storage registry
/// * `_session` - DataFusion session context (reserved for future use)
/// * `sql` - Raw SQL statement
/// * `_exec_ctx` - Execution context with user information
///
/// # Returns
/// Success message indicating storage registration status
///
/// # Example SQL
/// ```sql
/// CREATE STORAGE local_fs
///   TYPE 'filesystem'
///   NAME 'Local Filesystem'
///   DESCRIPTION 'Local development storage'
///   BASE_DIRECTORY '/var/lib/kalamdb/data'
///   SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
///   USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/';
///
/// CREATE STORAGE s3_prod
///   TYPE 's3'
///   NAME 'Production S3 Storage'
///   BASE_DIRECTORY 's3://my-bucket/kalamdb/'
///   SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
///   USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/';
/// ```
pub async fn execute_create_storage(
    app_context: &Arc<AppContext>,
    _session: &SessionContext,
    sql: &str,
    _exec_ctx: &ExecutionContext,
) -> Result<ExecutionResult, KalamDbError> {
    let storages_provider = app_context.system_tables().storages();
    let storage_registry = app_context.storage_registry();

    // Parse CREATE STORAGE statement
    let stmt = CreateStorageStatement::parse(sql).map_err(|e| {
        KalamDbError::InvalidOperation(format!("CREATE STORAGE parse error: {}", e))
    })?;

    // Check if storage already exists
    let storage_id = StorageId::from(stmt.storage_id.as_str());
    if storages_provider
        .get_storage_by_id(&storage_id)
        .map_err(|e| KalamDbError::Other(format!("Failed to check storage: {}", e)))?
        .is_some()
    {
        return Err(KalamDbError::InvalidOperation(format!(
            "Storage '{}' already exists",
            stmt.storage_id
        )));
    }

    // Validate templates using StorageRegistry
    if !stmt.shared_tables_template.is_empty() {
        storage_registry.validate_template(&stmt.shared_tables_template, false)?;
    }
    if !stmt.user_tables_template.is_empty() {
        storage_registry.validate_template(&stmt.user_tables_template, true)?;
    }

    // Ensure filesystem storages eagerly create their base directory
    if stmt.storage_type == StorageType::Filesystem {
        ensure_filesystem_directory(&stmt.base_directory)?;
    }

    // Validate credentials JSON (if provided)
    let normalized_credentials = if let Some(raw) = stmt.credentials.as_ref() {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Invalid credentials JSON: {}", e))
        })?;

        if !value.is_object() {
            return Err(KalamDbError::InvalidOperation(
                "Credentials must be a JSON object".to_string(),
            ));
        }

        Some(serde_json::to_string(&value).map_err(|e| {
            KalamDbError::InvalidOperation(format!("Failed to normalize credentials JSON: {}", e))
        })?)
    } else {
        None
    };

    // Create storage record
    let storage = kalamdb_commons::system::Storage {
        storage_id: stmt.storage_id.clone(),
        storage_name: stmt.storage_name,
        description: stmt.description,
        storage_type: stmt.storage_type.to_string(),
        base_directory: stmt.base_directory,
        credentials: normalized_credentials,
        shared_tables_template: stmt.shared_tables_template,
        user_tables_template: stmt.user_tables_template,
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
    };

    // Insert into system.storages
    storages_provider
        .insert_storage(storage)
        .map_err(|e| KalamDbError::Other(format!("Failed to create storage: {}", e)))?;

    Ok(ExecutionResult::Success {
        message: format!("Storage '{}' created successfully", stmt.storage_id),
    })
}

/// Ensure a filesystem directory exists (used by CREATE STORAGE handlers)
pub fn ensure_filesystem_directory(path: &str) -> Result<(), KalamDbError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(KalamDbError::InvalidOperation(
            "Filesystem storage requires a non-empty base directory".to_string(),
        ));
    }

    let dir = std::path::Path::new(trimmed);
    std::fs::create_dir_all(dir).map_err(|e| {
        KalamDbError::io_message(format!(
            "Failed to create storage directory '{}': {}",
            dir.display(),
            e
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::create_test_session;
    use datafusion::prelude::SessionContext;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), Role::Dba, create_test_session())
    }

    #[tokio::test]
    async fn test_create_storage_success() {
        let app_ctx = AppContext::get();
        let session = SessionContext::new();
        let ctx = test_context();

        let sql = r#"
            CREATE STORAGE test_storage
              TYPE 'filesystem'
              NAME 'Test Storage'
              DESCRIPTION 'Test storage for unit tests'
              BASE_DIRECTORY '/tmp/kalamdb/test_storage'
              SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
              USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/'
        "#;

        let result = execute_create_storage(&app_ctx, &session, sql, &ctx).await;

        assert!(result.is_ok());
        match result.unwrap() {
            ExecutionResult::Success { message } => {
                assert!(message.contains("test_storage"));
                assert!(message.contains("created successfully"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_create_storage_duplicate() {
        let app_ctx = AppContext::get();
        let session = SessionContext::new();
        let ctx = test_context();

        let sql = r#"
            CREATE STORAGE test_storage_dup
              TYPE 'filesystem'
              NAME 'Duplicate Storage'
              BASE_DIRECTORY '/tmp/kalamdb/dup'
              SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
              USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/'
        "#;

        // First creation
        let result1 = execute_create_storage(&app_ctx, &session, sql, &ctx).await;
        assert!(result1.is_ok());

        // Second creation should fail
        let result2 = execute_create_storage(&app_ctx, &session, sql, &ctx).await;
        assert!(result2.is_err());
        match result2.unwrap_err() {
            KalamDbError::InvalidOperation(msg) => {
                assert!(msg.contains("already exists"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    #[tokio::test]
    async fn test_create_storage_invalid_type() {
        let app_ctx = AppContext::get();
        let session = SessionContext::new();
        let ctx = test_context();

        let sql = r#"
            CREATE STORAGE test_storage_bad
              TYPE 'invalid_type'
              NAME 'Bad Storage'
              BASE_DIRECTORY '/tmp/kalamdb/bad'
              SHARED_TABLES_TEMPLATE '{namespace}/{tableName}/'
              USER_TABLES_TEMPLATE '{namespace}/{tableName}/{userId}/'
        "#;

        let result = execute_create_storage(&app_ctx, &session, sql, &ctx).await;
        assert!(result.is_err());
    }
}
