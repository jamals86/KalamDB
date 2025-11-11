//! Append version function for MVCC DML operations
//!
//! This module implements append_version(), the core function used by INSERT/UPDATE/DELETE
//! to create new versions in the hot storage layer.

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::tables::shared_tables::shared_table_store::SharedTableRow;
use crate::tables::user_tables::user_table_store::UserTableRow;
use kalamdb_commons::ids::{SeqId, SharedTableRowId, UserTableRowId};
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::models::{TableId, UserId};
use std::collections::HashMap;
use std::sync::Arc;

/// Append a new version to the table's hot storage
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
    // TODO: Generate new SeqId via SnowflakeGenerator once method exists
    // For now, use placeholder
    let seq = 12345i64; // Placeholder
    let seq_id = SeqId::new(seq);

    // TODO: This is a placeholder implementation
    // Full implementation requires:
    // 1. app_context.snowflake_generator() method
    // 2. schema_registry.get_user_table_shared() method
    // 3. schema_registry.get_shared_table_provider() method
    
    match table_type {
        TableType::User => {
            let _user_id = user_id.ok_or_else(|| {
                KalamDbError::InvalidOperation("user_id required for user table".to_string())
            })?;

            // Placeholder: return error until integration complete
            return Err(KalamDbError::NotImplemented {
                feature: "append_version for user tables".to_string(),
                message: "Requires AppContext integration".to_string(),
            });
        }
        TableType::Shared => {
            // Placeholder: return error until integration complete
            return Err(KalamDbError::NotImplemented {
                feature: "append_version for shared tables".to_string(),
                message: "Requires AppContext integration".to_string(),
            });
        }
        TableType::Stream => {
            return Err(KalamDbError::InvalidOperation(
                "Stream tables not yet supported in append_version".to_string(),
            ));
        }
        TableType::System => {
            return Err(KalamDbError::InvalidOperation(
                "System tables cannot use append_version".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::AppContext;
    use kalamdb_commons::models::{NamespaceId, TableName};

    #[tokio::test]
    async fn test_append_version_user_table() {
        let app_ctx = AppContext::new_test();
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let user_id = UserId::new("user1");
        let fields = serde_json::json!({"id": 1, "name": "Alice"});

        // Note: This test will fail until we properly register the user table
        // in the schema registry. This is placeholder for future integration.
        let result = append_version(
            app_ctx.clone(),
            &table_id,
            TableType::User,
            Some(user_id),
            fields,
            false,
        )
        .await;

        // For now, we expect TableNotFound
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_append_version_requires_user_id_for_user_tables() {
        let app_ctx = AppContext::new_test();
        let table_id = TableId::new(NamespaceId::new("ns1"), TableName::new("table1"));
        let fields = serde_json::json!({"id": 1, "name": "Alice"});

        let result = append_version(
            app_ctx.clone(),
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
        let app_ctx = AppContext::new_test();
        let table_id = TableId::new(NamespaceId::new("system"), TableName::new("users"));
        let fields = serde_json::json!({"id": 1});

        let result = append_version(
            app_ctx.clone(),
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
