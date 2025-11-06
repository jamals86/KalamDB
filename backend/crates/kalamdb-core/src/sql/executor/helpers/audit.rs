//! Audit Logging Utilities
//!
//! Helper functions for creating and managing audit log entries.
//! **Phase 2 Task T018**: Centralized audit logging for SQL operations.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::types::ExecutionContext; // TEMP: models not yet publicly re-exported
use kalamdb_commons::models::system::AuditLogEntry;
use kalamdb_commons::models::AuditLogId;
use chrono::Utc;

/// Create an audit log entry for a SQL operation
///
/// # Arguments
/// * `context` - Execution context with user and audit information
/// * `action` - Action performed (e.g., "CREATE_TABLE", "DROP_USER")
/// * `target` - Target of the action (e.g., "default.users", "user:alice")
/// * `details` - Optional JSON details about the operation
///
/// # Returns
/// * `AuditLogEntry` - Populated audit log entry
///
/// # Example
/// ```ignore
/// use kalamdb_core::sql::executor::handlers::audit::create_audit_entry;
///
/// let entry = create_audit_entry(
///     &exec_ctx,
///     "CREATE_TABLE",
///     "default.users",
///     Some(r#"{"columns": 5, "table_type": "USER"}"#.to_string()),
/// );
/// ```
pub fn create_audit_entry(
    context: &ExecutionContext,
    action: &str,
    target: &str,
    details: Option<String>,
) -> AuditLogEntry {
    // Generate a unique audit ID (timestamp-based)
    let timestamp = Utc::now().timestamp_millis();
    let audit_id = AuditLogId::from(format!("audit_{}", timestamp));

    AuditLogEntry {
        audit_id,
        timestamp,
        actor_user_id: context.user_id().clone(),
        actor_username: kalamdb_commons::UserName::from(context.user_id().as_str()),
        action: action.to_string(),
        target: target.to_string(),
        details,
        ip_address: context.ip_address().map(|s| s.to_string()),
    }
}

/// Create audit entry for DDL operations
///
/// Shorthand for common DDL actions.
///
/// # Arguments
/// * `context` - Execution context
/// * `operation` - DDL operation (CREATE, ALTER, DROP)
/// * `object_type` - Object type (TABLE, NAMESPACE, STORAGE, USER)
/// * `object_name` - Fully qualified object name
/// * `details` - Optional operation details
pub fn log_ddl_operation(
    context: &ExecutionContext,
    operation: &str,
    object_type: &str,
    object_name: &str,
    details: Option<String>,
) -> AuditLogEntry {
    let action = format!("{}_{}", operation, object_type);
    create_audit_entry(context, &action, object_name, details)
}

/// Create audit entry for DML operations
///
/// Logs INSERT, UPDATE, DELETE operations.
///
/// # Arguments
/// * `context` - Execution context
/// * `operation` - DML operation (INSERT, UPDATE, DELETE)
/// * `table_name` - Fully qualified table name
/// * `rows_affected` - Number of rows affected
pub fn log_dml_operation(
    context: &ExecutionContext,
    operation: &str,
    table_name: &str,
    rows_affected: usize,
) -> AuditLogEntry {
    let details = serde_json::json!({
        "rows_affected": rows_affected,
    })
    .to_string();

    create_audit_entry(context, operation, table_name, Some(details))
}

/// Create audit entry for query operations
///
/// Logs SELECT and other read operations.
///
/// # Arguments
/// * `context` - Execution context
/// * `query_type` - Query type (SELECT, DESCRIBE, SHOW)
/// * `target` - Query target
/// * `execution_time_ms` - Execution time in milliseconds
pub fn log_query_operation(
    context: &ExecutionContext,
    query_type: &str,
    target: &str,
    execution_time_ms: u64,
) -> AuditLogEntry {
    let details = serde_json::json!({
        "execution_time_ms": execution_time_ms,
    })
    .to_string();

    create_audit_entry(context, query_type, target, Some(details))
}

/// Create audit entry for authentication events
///
/// Logs login, logout, password changes, etc.
///
/// # Arguments
/// * `user_id` - User ID
/// * `action` - Authentication action (LOGIN, LOGOUT, PASSWORD_CHANGE, etc.)
/// * `success` - Whether the action succeeded
/// * `ip_address` - Optional IP address
pub fn log_auth_event(
    user_id: &kalamdb_commons::UserId,
    action: &str,
    success: bool,
    ip_address: Option<String>,
) -> AuditLogEntry {
    let timestamp = Utc::now().timestamp_millis();
    let audit_id = AuditLogId::from(format!("audit_{}", timestamp));

    let details = serde_json::json!({
        "success": success,
    })
    .to_string();

    AuditLogEntry {
        audit_id,
        timestamp,
        actor_user_id: user_id.clone(),
        actor_username: kalamdb_commons::UserName::from(user_id.as_str()),
        action: action.to_string(),
        target: format!("user:{}", user_id.as_str()),
        details: Some(details),
        ip_address,
    }
}

/// Persist audit log entry to storage
///
/// **Note**: This is a placeholder. Actual implementation will use
/// AuditLogsTableProvider to persist entries to system.audit_logs.
///
/// # Arguments
/// * `entry` - Audit log entry to persist
///
/// # Returns
/// * `Ok(())` - Entry persisted successfully
/// * `Err(KalamDbError)` - Persistence failed
pub async fn persist_audit_entry(_entry: &AuditLogEntry) -> Result<(), KalamDbError> {
    // TODO: Phase 7 (US3) - Implement actual persistence via SystemTablesRegistry
    // let audit_logs_provider = app_ctx.system_tables().audit_logs();
    // audit_logs_provider.insert(entry).await?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{UserId, Role};

    #[test]
    fn test_create_audit_entry() {
        let ctx = ExecutionContext::with_audit_info(
            UserId::from("alice"),
            Role::User,
            None,
            Some("req-123".to_string()),
            Some("192.168.1.1".to_string()),
        );

        let entry = create_audit_entry(
            &ctx,
            "CREATE_TABLE",
            "default.users",
            Some(r#"{"columns": 5}"#.to_string()),
        );

        assert_eq!(entry.action, "CREATE_TABLE");
        assert_eq!(entry.target, "default.users");
        assert_eq!(entry.actor_user_id.as_str(), "alice");
        assert_eq!(entry.ip_address, Some("192.168.1.1".to_string()));
        assert!(entry.details.is_some());
    }

    #[test]
    fn test_log_ddl_operation() {
        let ctx = ExecutionContext::new(UserId::from("bob"), Role::Dba);

        let entry = log_ddl_operation(
            &ctx,
            "CREATE",
            "TABLE",
            "prod.events",
            Some(r#"{"table_type": "STREAM"}"#.to_string()),
        );

        assert_eq!(entry.action, "CREATE_TABLE");
        assert_eq!(entry.target, "prod.events");
        assert_eq!(entry.actor_user_id.as_str(), "bob");
    }

    #[test]
    fn test_log_dml_operation() {
        let ctx = ExecutionContext::new(UserId::from("charlie"), Role::User);

        let entry = log_dml_operation(&ctx, "INSERT", "default.logs", 1000);

        assert_eq!(entry.action, "INSERT");
        assert_eq!(entry.target, "default.logs");
        assert!(entry.details.unwrap().contains("1000"));
    }

    #[test]
    fn test_log_query_operation() {
        let ctx = ExecutionContext::new(UserId::from("dave"), Role::User);

        let entry = log_query_operation(&ctx, "SELECT", "default.users", 150);

        assert_eq!(entry.action, "SELECT");
        assert_eq!(entry.target, "default.users");
        assert!(entry.details.unwrap().contains("150"));
    }

    #[test]
    fn test_log_auth_event() {
        let user_id = UserId::from("eve");

        let entry = log_auth_event(
            &user_id,
            "LOGIN",
            true,
            Some("10.0.0.1".to_string()),
        );

        assert_eq!(entry.action, "LOGIN");
        assert_eq!(entry.target, "user:eve");
        assert_eq!(entry.ip_address, Some("10.0.0.1".to_string()));
        assert!(entry.details.unwrap().contains("true"));
    }
}
