//! Authentication and authorization module

pub mod roles;

pub use roles::{
    can_read, can_write, is_admin, validate_role, ROLE_ADMIN, ROLE_READONLY, ROLE_USER,
    VALID_ROLES,
};
