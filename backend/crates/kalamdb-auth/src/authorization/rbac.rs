//! Role-Based Access Control (RBAC) for KalamDB
//!
//! This module provides permission checking based on user roles.
//! Uses the canonical Role enum from kalamdb-commons.

use kalamdb_commons::{Role, TableAccess, TableType};

/// Check if a role can access a given table type.
///
/// # Access Rules
/// - **System role**: Can access ALL table types
/// - **Dba role**: Can access ALL table types
/// - **Service role**: Can access USER, SHARED, STREAM tables
/// - **User role**: Can access USER, SHARED, STREAM tables
///
/// # Arguments
/// * `role` - User's role
/// * `table_type` - Type of table being accessed
///
/// # Returns
/// True if access is allowed, false otherwise
pub fn can_access_table_type(role: Role, table_type: TableType) -> bool {
    match role {
        Role::System | Role::Dba => true, // Admins can access everything
        Role::Service | Role::User => {
            // Regular users cannot access system tables directly
            !matches!(table_type, TableType::System)
        },
    }
}

/// Check if a role can create tables.
///
/// # Access Rules
/// - **System role**: Can create any table type
/// - **Dba role**: Can create any table type
/// - **Service role**: Cannot create tables (DML only)
/// - **User role**: Cannot create tables (DML only)
///
/// # Arguments
/// * `role` - User's role
/// * `table_type` - Type of table to create
///
/// # Returns
/// True if creation is allowed, false otherwise
pub fn can_create_table(role: Role, table_type: TableType) -> bool {
    match role {
        Role::System | Role::Dba => true,
        // Allow Service role to create SHARED tables as well
        Role::Service => {
            matches!(table_type, TableType::User | TableType::Stream | TableType::Shared)
        },
        // User role can only create USER and STREAM tables
        Role::User => matches!(table_type, TableType::User | TableType::Stream),
    }
}

/// Check if a role can manage users (create, update, delete).
///
/// # Access Rules
/// - **System role**: Can manage all users
/// - **Dba role**: Can manage all users
/// - **Service role**: Cannot manage users
/// - **User role**: Cannot manage users
///
/// # Arguments
/// * `role` - User's role
///
/// # Returns
/// True if user management is allowed, false otherwise
pub fn can_manage_users(role: Role) -> bool {
    matches!(role, Role::System | Role::Dba)
}

/// Check if a role can delete tables.
///
/// # Access Rules
/// - **System role**: Can delete any table
/// - **Dba role**: Can delete any table
/// - **Service role**: Cannot delete tables (DML only)
/// - **User role**: Cannot delete tables (DML only)
///
/// # Arguments
/// * `role` - User's role
/// * `table_type` - Type of table to delete
/// * `is_owner` - Whether the user owns the table
///
/// # Returns
/// True if deletion is allowed, false otherwise
pub fn can_delete_table(role: Role, _table_type: TableType, _is_owner: bool) -> bool {
    matches!(role, Role::System | Role::Dba)
}

/// Check if a role can modify table schema (ALTER TABLE).
///
/// # Access Rules
/// - **System role**: Can modify any table
/// - **Dba role**: Can modify any table
/// - **Service role**: Can alter SHARED, USER, and STREAM tables
/// - **User role**: Can alter USER and STREAM tables
///
/// # Arguments
/// * `role` - User's role
/// * `table_type` - Type of table to modify
/// * `is_owner` - Whether the user owns the table
///
/// # Returns
/// True if modification is allowed, false otherwise
pub fn can_alter_table(role: Role, table_type: TableType, _is_owner: bool) -> bool {
    match role {
        Role::System | Role::Dba => true,
        // Allow Service role to alter SHARED/USER/STREAM tables
        Role::Service => matches!(table_type, TableType::Shared | TableType::User | TableType::Stream),
        // Allow User role to alter USER and STREAM tables
        Role::User => matches!(table_type, TableType::User | TableType::Stream),
    }
}

/// Check if a role can execute administrative operations.
///
/// # Administrative Operations
/// - Backup/restore namespaces
/// - Flush operations
/// - Job management
/// - Storage configuration
///
/// # Arguments
/// * `role` - User's role
///
/// # Returns
/// True if admin operations are allowed, false otherwise
pub fn can_execute_admin_operations(role: Role) -> bool {
    matches!(role, Role::System | Role::Dba)
}

/// Check if a user can access a shared table based on its access level.
///
/// # Access Rules
/// - **Public**: All authenticated users can READ (SELECT only)
/// - **Private**: Only service/dba/system roles can access
/// - **Restricted**: Only service/dba/system roles can access (owner check for future expansion)
///
/// # Arguments
/// * `access_level` - Table's access level
/// * `is_owner` - Whether the user owns the table (reserved for future use)
/// * `role` - User's role
///
/// # Returns
/// True if access is allowed, false otherwise
pub fn can_access_shared_table(access_level: TableAccess, is_owner: bool, role: Role) -> bool {
    // Admins and service accounts can access everything
    if matches!(role, Role::System | Role::Dba | Role::Service) {
        return true;
    }

    match access_level {
        TableAccess::Public => true, // All authenticated users can read public tables
        TableAccess::Private => false, // Only privileged roles (checked above)
        TableAccess::Restricted => {
            // For now, only privileged roles or table owner
            // TODO: Implement permissions table for explicit grants
            is_owner
        },
    }
}

/// Check if a user can write to (INSERT/UPDATE/DELETE) a shared table.
///
/// # Access Rules
/// - **Public**: Only service/dba/system roles can write (users are READ ONLY)
/// - **Private**: Only service/dba/system roles can write
/// - **Restricted**: Only service/dba/system roles or table owner can write
///
/// # Arguments
/// * `access_level` - Table's access level
/// * `is_owner` - Whether the user owns the table (reserved for future use)
/// * `role` - User's role
///
/// # Returns
/// True if write access is allowed, false otherwise
pub fn can_write_shared_table(access_level: TableAccess, is_owner: bool, role: Role) -> bool {
    // Admins and service accounts can write to everything
    if matches!(role, Role::System | Role::Dba | Role::Service) {
        return true;
    }

    match access_level {
        TableAccess::Public => false, // Regular users CANNOT write to public tables (read-only)
        TableAccess::Private => false, // Only privileged roles (checked above)
        TableAccess::Restricted => {
            // For now, only privileged roles or table owner can write
            // TODO: Implement permissions table for explicit grants
            is_owner
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_access_table_type() {
        // System and DBA can access everything
        assert!(can_access_table_type(Role::System, TableType::System));
        assert!(can_access_table_type(Role::System, TableType::User));
        assert!(can_access_table_type(Role::Dba, TableType::System));
        assert!(can_access_table_type(Role::Dba, TableType::Shared));

        // Regular users cannot access system tables
        assert!(!can_access_table_type(Role::User, TableType::System));
        assert!(can_access_table_type(Role::User, TableType::User));
        assert!(can_access_table_type(Role::User, TableType::Shared));
        assert!(can_access_table_type(Role::Service, TableType::Stream));
    }

    #[test]
    fn test_can_create_table() {
        // Only System and Dba can create tables
        assert!(can_create_table(Role::System, TableType::System));
        assert!(can_create_table(Role::System, TableType::User));
        assert!(can_create_table(Role::Dba, TableType::User));
        assert!(can_create_table(Role::Dba, TableType::Shared));

        // Service can create User, Stream and Shared tables
        assert!(can_create_table(Role::Service, TableType::User));
        assert!(can_create_table(Role::Service, TableType::Shared));
        assert!(can_create_table(Role::User, TableType::User));
        assert!(!can_create_table(Role::User, TableType::System));
    }

    #[test]
    fn test_can_manage_users() {
        assert!(can_manage_users(Role::System));
        assert!(can_manage_users(Role::Dba));
        assert!(!can_manage_users(Role::User));
        assert!(!can_manage_users(Role::Service));
    }

    #[test]
    fn test_can_delete_table() {
        // Only System and Dba can delete tables
        assert!(can_delete_table(Role::System, TableType::System, false));
        assert!(can_delete_table(Role::Dba, TableType::User, false));
        assert!(can_delete_table(Role::Dba, TableType::Shared, true));

        // Service and User cannot delete tables (DML only)
        assert!(!can_delete_table(Role::Service, TableType::User, true));
        assert!(!can_delete_table(Role::Service, TableType::Shared, false));
        assert!(!can_delete_table(Role::User, TableType::User, true));
        assert!(!can_delete_table(Role::User, TableType::User, false));
        assert!(!can_delete_table(Role::User, TableType::System, true));
    }

    #[test]
    fn test_can_access_shared_table() {
        // Public tables accessible to all authenticated users (read-only)
        assert!(can_access_shared_table(TableAccess::Public, false, Role::User));
        assert!(can_access_shared_table(TableAccess::Public, true, Role::User));
        assert!(can_access_shared_table(TableAccess::Public, false, Role::Service));
    }

    #[test]
    fn test_can_write_shared_table() {
        assert!(can_write_shared_table(TableAccess::Public, false, Role::Service));
        assert!(!can_write_shared_table(TableAccess::Public, false, Role::User));
    }
}
