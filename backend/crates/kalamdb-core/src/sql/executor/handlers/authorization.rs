//! Authorization Gateway - Centralized RBAC enforcement for SQL operations
//!
//! This module provides a single point of authorization checking for all SQL statements,
//! implementing fail-fast permission validation before statement execution.

use crate::error::KalamDbError;
use super::types::ExecutionContext;
use kalamdb_sql::statement_classifier::SqlStatement;
use kalamdb_commons::{NamespaceId, UserId};

/// Authorization handler for SQL statements
///
/// Implements role-based access control (RBAC) with the following role hierarchy:
/// - System: Full access to all operations (localhost-only)
/// - DBA: Administrative operations (user management, namespace DDL, storage)
/// - Service: Service account operations (limited DDL, full DML)
/// - User: Standard user operations (DML only, table-level authorization)
pub struct AuthorizationHandler;

impl AuthorizationHandler {
    /// Check if the execution context has permission to execute the given statement type
    ///
    /// # Arguments
    /// * `ctx` - The execution context containing user identity and role
    /// * `statement_type` - The classified SQL statement type
    ///
    /// # Returns
    /// * `Ok(())` if authorized
    /// * `Err(KalamDbError::Unauthorized)` if not authorized
    ///
    /// # Authorization Rules
    /// 1. Admin users (DBA, System) can execute any statement
    /// 2. DDL operations (CREATE/ALTER/DROP) require DBA+ role
    /// 3. User management (CREATE/ALTER/DROP USER) requires DBA+ role
    /// 4. Storage operations require DBA+ role
    /// 5. System namespace operations require System role
    /// 6. Read-only operations (SELECT, SHOW, DESCRIBE) allowed for all authenticated users
    /// 7. Table-level operations defer to per-table authorization checks
    pub fn check_authorization(
        ctx: &ExecutionContext,
        statement_type: &SqlStatement,
    ) -> Result<(), KalamDbError> {
        // Admin users (DBA, System) can do anything
        if ctx.is_admin() {
            return Ok(());
        }

        match statement_type {
            // Storage and global operations require admin privileges
            SqlStatement::CreateStorage
            | SqlStatement::AlterStorage
            | SqlStatement::DropStorage
            | SqlStatement::KillJob => {
                Err(KalamDbError::Unauthorized(
                    "Admin privileges (DBA or System role) required for storage and job operations".to_string(),
                ))
            }

            // User management requires admin privileges (except for self-modification)
            SqlStatement::CreateUser | SqlStatement::DropUser => {
                Err(KalamDbError::Unauthorized(
                    "Admin privileges (DBA or System role) required for user management".to_string(),
                ))
            }

            // ALTER USER allowed for self (changing own password), admin for others
            // The actual check is deferred to execute_alter_user method
            SqlStatement::AlterUser => {
                Ok(())
            }

            // Namespace DDL requires admin privileges
            SqlStatement::CreateNamespace
            | SqlStatement::AlterNamespace
            | SqlStatement::DropNamespace => {
                Err(KalamDbError::Unauthorized(
                    "Admin privileges (DBA or System role) required for namespace operations".to_string(),
                ))
            }

            // Read-only operations on system tables are allowed for all authenticated users
            SqlStatement::ShowNamespaces
            | SqlStatement::ShowTables
            | SqlStatement::ShowStorages
            | SqlStatement::ShowStats
            | SqlStatement::DescribeTable => {
                Ok(())
            }

            // CREATE TABLE, DROP TABLE, FLUSH TABLE, ALTER TABLE - check table ownership in execute methods
            SqlStatement::CreateTable
            | SqlStatement::AlterTable
            | SqlStatement::DropTable
            | SqlStatement::FlushTable
            | SqlStatement::FlushAllTables => {
                // Table-level authorization will be checked in the execution methods
                // Users can only create/modify/drop tables they own
                // Admin users can operate on any table (already returned above)
                Ok(())
            }

            // SELECT, INSERT, UPDATE, DELETE - check table access in execution
            SqlStatement::Select
            | SqlStatement::Insert
            | SqlStatement::Update
            | SqlStatement::Delete => {
                // Query-level authorization will be enforced by using per-user sessions
                // User tables are filtered by user_id in UserTableProvider
                // Shared tables enforce access control based on access_level
                Ok(())
            }

            // Subscriptions, transactions, and other operations allowed for all users
            SqlStatement::Subscribe
            | SqlStatement::KillLiveQuery
            | SqlStatement::BeginTransaction
            | SqlStatement::CommitTransaction
            | SqlStatement::RollbackTransaction => {
                Ok(())
            }

            SqlStatement::Unknown => {
                // Unknown statements will fail in execute anyway
                // Allow them through so we can return a better error message
                Ok(())
            }
        }
    }

    /// Check if a user has permission to access a specific namespace
    ///
    /// # Arguments
    /// * `ctx` - The execution context containing user identity and role
    /// * `namespace_id` - The namespace to check access for
    ///
    /// # Returns
    /// * `Ok(())` if authorized
    /// * `Err(KalamDbError::Unauthorized)` if not authorized
    ///
    /// # Authorization Rules
    /// 1. System role can access any namespace
    /// 2. DBA role can access any namespace
    /// 3. Service and User roles can only access non-system namespaces
    /// 4. "system" namespace requires System role
    pub fn check_namespace_access(
        ctx: &ExecutionContext,
        namespace_id: &NamespaceId,
    ) -> Result<(), KalamDbError> {
        // Admin users can access any namespace
        if ctx.is_admin() {
            return Ok(());
        }

        // Check for system namespace access
        if namespace_id.as_str() == "system" {
            return Err(KalamDbError::Unauthorized(
                "System namespace can only be accessed by System role".to_string(),
            ));
        }

        // All authenticated users can access non-system namespaces
        Ok(())
    }

    /// Check if a user can modify another user's account
    ///
    /// # Arguments
    /// * `ctx` - The execution context containing user identity and role
    /// * `target_user_id` - The user ID being modified
    ///
    /// # Returns
    /// * `Ok(())` if authorized
    /// * `Err(KalamDbError::Unauthorized)` if not authorized
    ///
    /// # Authorization Rules
    /// 1. Admin users can modify any user
    /// 2. Users can modify their own account (e.g., change password)
    /// 3. Users cannot modify other users' accounts
    pub fn check_user_modification(
        ctx: &ExecutionContext,
        target_user_id: &UserId,
    ) -> Result<(), KalamDbError> {
        // Admin users can modify any user
        if ctx.is_admin() {
            return Ok(());
        }

        // Users can modify their own account
        if &ctx.user_id == target_user_id {
            return Ok(());
        }

        // Users cannot modify other users
        Err(KalamDbError::Unauthorized(
            "You can only modify your own account. Admin privileges required to modify other users.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::{Role, UserId, NamespaceId};

    fn create_user_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("user1"), Role::User)
    }

    fn create_service_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("service1"), Role::Service)
    }

    fn create_dba_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("dba1"), Role::Dba)
    }

    fn create_system_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("system"), Role::System)
    }

    // Test DDL authorization
    #[test]
    fn test_user_cannot_create_namespace() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateNamespace);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Admin privileges"));
    }

    #[test]
    fn test_dba_can_create_namespace() {
        let ctx = create_dba_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateNamespace);
        assert!(result.is_ok());
    }

    #[test]
    fn test_system_can_create_namespace() {
        let ctx = create_system_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateNamespace);
        assert!(result.is_ok());
    }

    // Test user management authorization
    #[test]
    fn test_user_cannot_create_user() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateUser);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Admin privileges"));
    }

    #[test]
    fn test_dba_can_create_user() {
        let ctx = create_dba_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateUser);
        assert!(result.is_ok());
    }

    #[test]
    fn test_user_can_alter_self() {
        let ctx = create_user_context();
        // ALTER USER passes authorization check at statement level
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::AlterUser);
        assert!(result.is_ok());

        // But user modification check enforces self-only rule
        let result = AuthorizationHandler::check_user_modification(&ctx, &UserId::from("user1"));
        assert!(result.is_ok());

        let result = AuthorizationHandler::check_user_modification(&ctx, &UserId::from("other_user"));
        assert!(result.is_err());
    }

    // Test storage operations authorization
    #[test]
    fn test_user_cannot_create_storage() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateStorage);
        assert!(result.is_err());
    }

    #[test]
    fn test_dba_can_create_storage() {
        let ctx = create_dba_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::CreateStorage);
        assert!(result.is_ok());
    }

    // Test read-only operations
    #[test]
    fn test_user_can_show_namespaces() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::ShowNamespaces);
        assert!(result.is_ok());
    }

    #[test]
    fn test_user_can_describe_table() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::DescribeTable);
        assert!(result.is_ok());
    }

    // Test DML operations (deferred to table-level checks)
    #[test]
    fn test_user_can_select() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::Select);
        assert!(result.is_ok());
    }

    #[test]
    fn test_user_can_insert() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::Insert);
        assert!(result.is_ok());
    }

    // Test namespace access
    #[test]
    fn test_user_cannot_access_system_namespace() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_namespace_access(&ctx, &NamespaceId::from("system"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("System namespace"));
    }

    #[test]
    fn test_dba_can_access_system_namespace() {
        let ctx = create_dba_context();
        let result = AuthorizationHandler::check_namespace_access(&ctx, &NamespaceId::from("system"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_user_can_access_app_namespace() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_namespace_access(&ctx, &NamespaceId::from("app"));
        assert!(result.is_ok());
    }

    // Test transactions
    #[test]
    fn test_user_can_begin_transaction() {
        let ctx = create_user_context();
        let result = AuthorizationHandler::check_authorization(&ctx, &SqlStatement::BeginTransaction);
        assert!(result.is_ok());
    }
}
