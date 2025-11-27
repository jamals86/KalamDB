// Authenticated user context for request handling

use kalamdb_commons::{Role, UserId, models::ConnectionInfo};

/// Authenticated user context for a request.
///
/// Contains all information about the authenticated user needed for
/// authorization decisions and audit logging.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// User's unique identifier
    pub user_id: UserId,
    /// Username
    pub username: String,
    /// User's role (User, Service, Dba, System)
    pub role: Role,
    /// Email address (if available)
    pub email: Option<String>,
    /// Connection information (IP address, localhost check)
    pub connection_info: ConnectionInfo,
}

impl AuthenticatedUser {
    /// Create a new authenticated user context.
    ///
    /// # Arguments
    /// * `user_id` - User's unique identifier
    /// * `username` - Username
    /// * `role` - User's role
    /// * `email` - Email address (optional)
    /// * `connection_info` - Connection information
    ///
    /// # Returns
    /// A new AuthenticatedUser instance
    pub fn new(
        user_id: UserId,
        username: String,
        role: Role,
        email: Option<String>,
        connection_info: ConnectionInfo,
    ) -> Self {
        Self {
            user_id,
            username,
            role,
            email,
            connection_info,
        }
    }

    /// Check if the user has DBA or System role.
    ///
    /// # Returns
    /// True if user is DBA or System, false otherwise
    pub fn is_admin(&self) -> bool {
        matches!(self.role, Role::Dba | Role::System)
    }

    /// Check if the user has System role.
    ///
    /// # Returns
    /// True if user is System, false otherwise
    pub fn is_system(&self) -> bool {
        matches!(self.role, Role::System)
    }

    /// Check if the user is connecting from localhost.
    ///
    /// # Returns
    /// True if connection is from localhost, false otherwise
    pub fn is_localhost(&self) -> bool {
        self.connection_info.is_localhost()
    }

    /// Check if the user can access the given resource.
    ///
    /// This is a basic check - more complex authorization logic should be
    /// implemented by callers based on specific requirements.
    ///
    /// # Arguments
    /// * `resource_user_id` - The user ID that owns the resource (if applicable)
    ///
    /// # Returns
    /// True if access should be allowed, false otherwise
    pub fn can_access_user_resource(&self, resource_user_id: &UserId) -> bool {
        // System and DBA can access everything
        if self.is_admin() {
            return true;
        }

        // Users can access their own resources
        &self.user_id == resource_user_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_user(role: Role, is_localhost: bool) -> AuthenticatedUser {
        let addr = if is_localhost {
            Some("127.0.0.1".to_string())
        } else {
            Some("192.168.1.100".to_string())
        };

        AuthenticatedUser::new(
            UserId::new("user_123"),
            "testuser".to_string(),
            role,
            Some("test@example.com".to_string()),
            ConnectionInfo::new(addr),
        )
    }

    #[test]
    fn test_is_admin() {
        assert!(create_test_user(Role::Dba, true).is_admin());
        assert!(create_test_user(Role::System, true).is_admin());
        assert!(!create_test_user(Role::User, true).is_admin());
        assert!(!create_test_user(Role::Service, true).is_admin());
    }

    #[test]
    fn test_is_system() {
        assert!(create_test_user(Role::System, true).is_system());
        assert!(!create_test_user(Role::Dba, true).is_system());
        assert!(!create_test_user(Role::User, true).is_system());
    }

    #[test]
    fn test_can_access_own_resource() {
        let user = create_test_user(Role::User, true);
        assert!(user.can_access_user_resource(&UserId::new("user_123")));
        assert!(!user.can_access_user_resource(&UserId::new("user_456")));
    }

    #[test]
    fn test_admin_can_access_all_resources() {
        let dba = create_test_user(Role::Dba, true);
        assert!(dba.can_access_user_resource(&UserId::new("user_123")));
        assert!(dba.can_access_user_resource(&UserId::new("user_456")));

        let system = create_test_user(Role::System, true);
        assert!(system.can_access_user_resource(&UserId::new("user_123")));
        assert!(system.can_access_user_resource(&UserId::new("user_456")));
    }
}
