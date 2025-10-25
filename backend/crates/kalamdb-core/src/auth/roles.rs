//! Authorization and role validation

use crate::error::KalamDbError;

/// Valid user roles in KalamDB
pub const VALID_ROLES: &[&str] = &["admin", "user", "readonly"];

/// Admin role - full system access
pub const ROLE_ADMIN: &str = "admin";

/// User role - standard user access
pub const ROLE_USER: &str = "user";

/// Readonly role - read-only access
pub const ROLE_READONLY: &str = "readonly";

/// Validate if a role string is valid
///
/// # Arguments
/// * `role` - Role string to validate
///
/// # Returns
/// * `Ok(())` if role is valid
/// * `Err(KalamDbError)` if role is invalid
///
/// # Example
/// ```
/// use kalamdb_core::auth::roles::validate_role;
///
/// assert!(validate_role("admin").is_ok());
/// assert!(validate_role("user").is_ok());
/// assert!(validate_role("readonly").is_ok());
/// assert!(validate_role("superuser").is_err());
/// ```
pub fn validate_role(role: &str) -> Result<(), KalamDbError> {
    if VALID_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(KalamDbError::InvalidSql(format!(
            "Invalid role '{}'. Valid roles are: {}",
            role,
            VALID_ROLES.join(", ")
        )))
    }
}

/// Check if a role is admin
pub fn is_admin(role: &str) -> bool {
    role == ROLE_ADMIN
}

/// Check if a role has write access
pub fn can_write(role: &str) -> bool {
    matches!(role, ROLE_ADMIN | ROLE_USER)
}

/// Check if a role has read access
pub fn can_read(role: &str) -> bool {
    VALID_ROLES.contains(&role)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_role_valid() {
        assert!(validate_role("admin").is_ok());
        assert!(validate_role("user").is_ok());
        assert!(validate_role("readonly").is_ok());
    }

    #[test]
    fn test_validate_role_invalid() {
        assert!(validate_role("superuser").is_err());
        assert!(validate_role("guest").is_err());
        assert!(validate_role("").is_err());
        assert!(validate_role("ADMIN").is_err()); // Case sensitive
    }

    #[test]
    fn test_is_admin() {
        assert!(is_admin("admin"));
        assert!(!is_admin("user"));
        assert!(!is_admin("readonly"));
    }

    #[test]
    fn test_can_write() {
        assert!(can_write("admin"));
        assert!(can_write("user"));
        assert!(!can_write("readonly"));
    }

    #[test]
    fn test_can_read() {
        assert!(can_read("admin"));
        assert!(can_read("user"));
        assert!(can_read("readonly"));
        assert!(!can_read("invalid"));
    }
}
