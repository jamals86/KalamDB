//! Role-based access helpers (RBAC)
//!
//! Centralized authorization rules for table/DDL operations.

use kalamdb_commons::schemas::TableType;
use kalamdb_commons::Role;

/// Check if a role can access a table type (read-level classification).
#[inline]
pub fn can_access_table_type(role: Role, table_type: TableType) -> bool {
    match role {
        Role::System | Role::Dba => true,
        Role::Service => matches!(table_type, TableType::Shared | TableType::Stream | TableType::User),
        Role::User => matches!(table_type, TableType::User | TableType::Stream),
    }
}

/// Check if a role can create a table of the given type.
#[inline]
pub fn can_create_table(role: Role, table_type: TableType) -> bool {
    match role {
        Role::System => true,
        Role::Dba => matches!(table_type, TableType::User | TableType::Shared | TableType::Stream),
        Role::Service => matches!(table_type, TableType::User | TableType::Shared | TableType::Stream),
        Role::User => matches!(table_type, TableType::User | TableType::Stream),
    }
}

/// Check if a role can delete a table.
#[inline]
pub fn can_delete_table(role: Role, table_type: TableType, _is_owner: bool) -> bool {
    match role {
        Role::System => true,
        Role::Dba => !matches!(table_type, TableType::System),
        Role::Service | Role::User => false,
    }
}

/// Check if a role can alter a table.
///
/// - System/Dba can alter any table.
/// - Service can alter Shared/User/Stream tables.
/// - User can alter User/Stream tables.
#[inline]
pub fn can_alter_table(role: Role, table_type: TableType, _is_owner: bool) -> bool {
    match role {
        Role::System | Role::Dba => true,
        Role::Service => matches!(table_type, TableType::Shared | TableType::User | TableType::Stream),
        Role::User => matches!(table_type, TableType::User | TableType::Stream),
    }
}

/// Check if a role can manage users.
#[inline]
pub fn can_manage_users(role: Role) -> bool {
    matches!(role, Role::System | Role::Dba)
}

/// Check if a role can create views.
#[inline]
pub fn can_create_view(role: Role) -> bool {
    matches!(role, Role::System | Role::Dba)
}

/// Check if a role has admin privileges.
#[inline]
pub fn is_admin_role(role: Role) -> bool {
    matches!(role, Role::System | Role::Dba)
}

/// Check if a role is system.
#[inline]
pub fn is_system_role(role: Role) -> bool {
    matches!(role, Role::System)
}

/// Helper for CREATE TABLE: allow USER/SERVICE to downgrade shared to user table.
#[inline]
pub fn can_downgrade_shared_to_user(role: Role) -> bool {
    matches!(role, Role::User | Role::Service)
}
