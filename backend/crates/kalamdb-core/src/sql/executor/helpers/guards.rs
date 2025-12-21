//! DDL Guard Helpers
//!
//! Common authorization and validation guards for DDL operations.
//! These helpers consolidate repeated validation patterns across handlers.

use crate::error::KalamDbError;
use crate::sql::executor::models::ExecutionContext;
use kalamdb_commons::models::NamespaceId;

/// Block modifications (ALTER, DROP, CREATE) on system namespaces.
///
/// System namespaces (`system`, `information_schema`, `pg_catalog`, `datafusion`)
/// are managed internally and cannot be modified by users.
///
/// # Arguments
/// * `namespace_id` - The namespace to check
/// * `operation` - The operation being attempted (e.g., "ALTER", "DROP", "CREATE")
/// * `object_type` - The type of object (e.g., "TABLE", "NAMESPACE")
/// * `object_name` - Optional object name for error messages
///
/// # Returns
/// * `Ok(())` if the namespace is NOT a system namespace
/// * `Err(KalamDbError::InvalidOperation)` if modification is blocked
///
/// # Example
/// ```ignore
/// block_system_namespace_modification(&namespace_id, "ALTER", "TABLE", Some(&table_name))?;
/// ```
pub fn block_system_namespace_modification(
    namespace_id: &NamespaceId,
    operation: &str,
    object_type: &str,
    object_name: Option<&str>,
) -> Result<(), KalamDbError> {
    if namespace_id.is_system_namespace() {
        let full_name = match object_name {
            Some(name) => format!("'{}.{}'", namespace_id.as_str(), name),
            None => format!("'{}'", namespace_id.as_str()),
        };

        log::warn!(
            "❌ {} {} blocked: Cannot modify system {} {}",
            operation,
            object_type,
            object_type.to_lowercase(),
            full_name
        );

        return Err(KalamDbError::InvalidOperation(format!(
            "Cannot {} system {} {}. System {}s are managed internally.",
            operation.to_lowercase(),
            object_type.to_lowercase(),
            full_name,
            object_type.to_lowercase()
        )));
    }
    Ok(())
}

/// Require admin privileges (DBA or System role) for an operation.
///
/// # Arguments
/// * `context` - The execution context containing user role information
/// * `action` - Description of the action (e.g., "create storage", "drop namespace")
///
/// # Returns
/// * `Ok(())` if user has admin privileges
/// * `Err(KalamDbError::Unauthorized)` if user lacks privileges
///
/// # Example
/// ```ignore
/// require_admin(context, "create storage")?;
/// ```
pub fn require_admin(context: &ExecutionContext, action: &str) -> Result<(), KalamDbError> {
    if !context.is_admin() {
        log::error!(
            "❌ {}: Insufficient privileges (user: {}, role: {:?})",
            action,
            context.user_id.as_str(),
            context.user_role
        );
        return Err(KalamDbError::Unauthorized(format!(
            "Insufficient privileges to {}. DBA or System role required.",
            action
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn create_context(role: Role) -> ExecutionContext {
        ExecutionContext::new(
            UserId::from("test_user"),
            role,
            datafusion::prelude::SessionContext::new(),
        )
    }

    #[test]
    fn test_block_system_namespace_modification() {
        // System namespaces should be blocked
        let system_ns = NamespaceId::from("system");
        let result = block_system_namespace_modification(&system_ns, "ALTER", "TABLE", Some("users"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot alter"));

        let info_schema = NamespaceId::from("information_schema");
        let result = block_system_namespace_modification(&info_schema, "DROP", "TABLE", None);
        assert!(result.is_err());

        // User namespaces should pass
        let user_ns = NamespaceId::from("my_namespace");
        let result = block_system_namespace_modification(&user_ns, "ALTER", "TABLE", Some("data"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_require_admin() {
        // Admin roles should pass
        let system_ctx = create_context(Role::System);
        assert!(require_admin(&system_ctx, "create storage").is_ok());

        let dba_ctx = create_context(Role::Dba);
        assert!(require_admin(&dba_ctx, "drop namespace").is_ok());

        // Non-admin roles should fail
        let user_ctx = create_context(Role::User);
        let result = require_admin(&user_ctx, "create storage");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient privileges"));

        let service_ctx = create_context(Role::Service);
        assert!(require_admin(&service_ctx, "drop namespace").is_err());
    }
}
