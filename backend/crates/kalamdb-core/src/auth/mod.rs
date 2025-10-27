//! Authentication and authorization module

pub mod rbac;
pub mod roles;

// Re-export RBAC functions
pub use rbac::{
    can_access_shared_table, can_access_table_type, can_alter_table, can_create_table,
    can_delete_table, can_execute_admin_operations, can_manage_users,
};

// Legacy string-based role functions (deprecated, use rbac module instead)
pub use roles::{
    can_read, can_write, is_admin, validate_role, ROLE_ADMIN, ROLE_READONLY, ROLE_USER,
    VALID_ROLES,
};
