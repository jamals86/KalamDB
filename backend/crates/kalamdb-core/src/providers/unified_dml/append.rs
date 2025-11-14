//! Append version function for MVCC DML operations
//!
//! This module implements append_version(), the core function used by INSERT/UPDATE/DELETE
//! to create new versions in the hot storage layer.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use kalamdb_tables::{SharedTableRow, UserTableRow};
use kalamdb_commons::ids::{SeqId, UserTableRowId};
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::models::{TableId, UserId};
use std::sync::Arc;

/// Append a new version to the table's hot storage (synchronous)
///
/// **MVCC Architecture**: This function is used by INSERT, UPDATE, and DELETE operations.
/// - INSERT: Appends new row with _deleted=false
/// - UPDATE: Appends modified row with new SeqId and _deleted=false
/// - DELETE: Appends tombstone row with _deleted=true
///
/// # Arguments
/// * `app_context` - Application context with SnowflakeGenerator
/// * `_table_id` - Table identifier (unused, for future extensions)
/// * `table_type` - User or Shared table type
/// * `user_id` - User ID (required for user tables, ignored for shared tables)
/// * `fields` - JSON object with all user-defined columns (including PK)
/// * `deleted` - Tombstone flag (false for INSERT/UPDATE, true for DELETE)
///
/// # Returns
/// * `Ok(SeqId)` - The sequence ID of the newly appended version
/// * `Err(KalamDbError)` - If append fails
pub fn append_version_sync(
    app_context: Arc<AppContext>,
    _table_id: &TableId,
    table_type: TableType,
    user_id: Option<UserId>,
    fields: serde_json::Value,
    deleted: bool,
) -> Result<SeqId, KalamDbError> {
    use kalamdb_store::entity_store::EntityStore as EntityStoreV2;

    // Validate table type
    match table_type {
        TableType::System => {
            return Err(KalamDbError::InvalidOperation(
                "System tables cannot use append_version()".to_string(),
            ));
        }
        TableType::Stream => {
            return Err(KalamDbError::InvalidOperation(
                "Stream tables cannot use append_version()".to_string(),
            ));
        }
        _ => {}
    }

    // Generate new SeqId via SystemColumnsService
    let sys_cols = app_context.system_columns_service();
    let seq_id = sys_cols.generate_seq_id()?;

    match table_type {
        TableType::User => {
            let user_id = user_id.ok_or_else(|| {
                KalamDbError::InvalidOperation("user_id required for user table".to_string())
            })?;

            // Create UserTableRow
            let entity = UserTableRow {
                user_id: user_id.clone(),
                _seq: seq_id,
                _deleted: deleted,
                fields: fields.clone(),
            };

            // Create composite key
            let row_key = UserTableRowId::new(user_id.clone(), seq_id);

            // Get user table store from AppContext
            let store = app_context.user_table_store();

            // Store the entity
            store.put(&row_key, &entity).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to append version to user table: {}",
                    e
                ))
            })?;

            log::debug!(
                "Appended version to user table for user {} with _seq {} (deleted: {})",
                user_id.as_str(),
                seq_id,
                deleted
            );

            Ok(seq_id)
        }
        TableType::Shared => {
            // Create SharedTableRow
            let entity = SharedTableRow {
                _seq: seq_id,
                _deleted: deleted,
                fields: fields.clone(),
            };

            // Key is just the SeqId (SharedTableRowId is a type alias for SeqId)
            let row_key = seq_id;

            // Get shared table store from AppContext
            let store = app_context.shared_table_store();

            // Store the entity
            store.put(&row_key, &entity).map_err(|e| {
                KalamDbError::InvalidOperation(format!(
                    "Failed to append version to shared table: {}",
                    e
                ))
            })?;

            log::debug!(
                "Appended version to shared table with _seq {} (deleted: {})",
                seq_id,
                deleted
            );

            Ok(seq_id)
        }
        TableType::System | TableType::Stream => {
            // Already handled above, unreachable
            unreachable!()
        }
    }
}

/// Append a new version to the table's hot storage (async wrapper)
///
/// **MVCC Architecture**: This function is used by INSERT, UPDATE, and DELETE operations.
/// - INSERT: Appends new row with _deleted=false
/// - UPDATE: Appends modified row with new SeqId and _deleted=false
/// - DELETE: Appends tombstone row with _deleted=true
///
/// # Arguments
/// * `app_context` - Application context with SnowflakeGenerator
/// * `table_id` - Table identifier
/// * `table_type` - User or Shared table type
/// * `user_id` - User ID (required for user tables, ignored for shared tables)
/// * `fields` - JSON object with all user-defined columns (including PK)
/// * `deleted` - Tombstone flag (false for INSERT/UPDATE, true for DELETE)
///
/// # Returns
/// * `Ok(SeqId)` - The sequence ID of the newly appended version
/// * `Err(KalamDbError)` - If append fails
pub async fn append_version(
    app_context: Arc<AppContext>,
    table_id: &TableId,
    table_type: TableType,
    user_id: Option<UserId>,
    fields: serde_json::Value,
    deleted: bool,
) -> Result<SeqId, KalamDbError> {
    // Delegate to synchronous implementation
    append_version_sync(app_context, table_id, table_type, user_id, fields, deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use kalamdb_commons::models::{NamespaceId, TableName};

    #[tokio::test]
    async fn test_append_version_user_table() {
        let app_ctx = Arc::new(AppContext::new_test());
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let user_id = UserId::new("user1");
        let fields = serde_json::json!({"id": 1, "name": "Alice"});

        let result = append_version(
            Arc::clone(&app_ctx),
            &table_id,
            TableType::User,
            Some(user_id),
            fields,
            false,
        )
        .await;

        // Should succeed with in-memory test backend
        assert!(result.is_ok(), "INSERT via append_version should succeed: {:?}", result.err());
        let seq_id = result.unwrap();
        assert!(seq_id.as_i64() > 0, "SeqId should be positive");
    }

    #[tokio::test]
    async fn test_append_version_requires_user_id_for_user_tables() {
        let app_ctx = Arc::new(AppContext::new_test());
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let fields = serde_json::json!({"id": 1, "name": "Alice"});

        let result = append_version(
            Arc::clone(&app_ctx),
            &table_id,
            TableType::User,
            None, // Missing user_id
            fields,
            false,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("user_id required"));
    }

    #[tokio::test]
    async fn test_append_version_system_table_rejected() {
        let app_ctx = Arc::new(AppContext::new_test());
        let table_id = TableId::new(NamespaceId::new("system"), TableName::new("users"));
        let fields = serde_json::json!({"id": 1});

        let result = append_version(
            Arc::clone(&app_ctx),
            &table_id,
            TableType::System,
            None,
            fields,
            false,
        )
        .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("System tables cannot"));
    }
}
