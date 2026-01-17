//! Authentication provider for KalamDB client.
//!
//! Handles JWT tokens and HTTP Basic Auth, attaching appropriate headers to HTTP requests.

use crate::error::Result;
use base64::{engine::general_purpose, Engine as _};

/// Authentication credentials for KalamDB server.
///
/// Supports JWT tokens and HTTP Basic Auth.
/// The auth provider automatically attaches the appropriate Authorization header.
///
/// # Examples
///
/// ```rust
/// use kalam_link::AuthProvider;
///
/// // HTTP Basic Auth (recommended)
/// let auth = AuthProvider::basic_auth("username".to_string(), "password".to_string());
///
/// // JWT token authentication
/// let auth = AuthProvider::jwt_token("eyJhbGc...".to_string());
///
/// // No authentication (localhost bypass mode)
/// let auth = AuthProvider::none();
/// ```
#[derive(Debug, Clone)]
pub enum AuthProvider {
    /// HTTP Basic Auth (username, password)
    BasicAuth(String, String),

    /// JWT token authentication
    JwtToken(String),

    /// No authentication (localhost bypass)
    None,
}

impl AuthProvider {
    /// Create HTTP Basic Auth (recommended for user authentication)
    ///
    /// Encodes username:password as base64 for Authorization: Basic header
    /// following RFC 7617.
    ///
    /// # Example
    /// ```
    /// use kalam_link::AuthProvider;
    /// let auth = AuthProvider::basic_auth("alice".to_string(), "secret123".to_string());
    /// ```
    pub fn basic_auth(username: String, password: String) -> Self {
        Self::BasicAuth(username, password)
    }

    /// Create system user authentication (convenience for CLI and internal tools)
    ///
    /// Uses the default system username "root" with provided password.
    /// This is a convenience method for CLI tools and internal processes.
    ///
    /// # Example
    /// ```
    /// use kalam_link::AuthProvider;
    /// let auth = AuthProvider::system_user_auth("generated_password_123".to_string());
    /// ```
    pub fn system_user_auth(password: String) -> Self {
        Self::BasicAuth("root".to_string(), password)
    }

    /// Create JWT token authentication
    pub fn jwt_token(token: String) -> Self {
        Self::JwtToken(token)
    }

    /// No authentication (for localhost bypass mode)
    pub fn none() -> Self {
        Self::None
    }

    /// Attach authentication headers to an HTTP request builder
    ///
    /// Applies the appropriate Authorization header based on the auth method:
    /// - BasicAuth: `Authorization: Basic <base64(username:password)>`
    /// - JwtToken: `Authorization: Bearer <token>`
    /// - None: No headers
    pub fn apply_to_request(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        match self {
            Self::BasicAuth(username, password) => {
                // Encode username:password as base64 (RFC 7617)
                let credentials = format!("{}:{}", username, password);
                let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());
                Ok(request.header("Authorization", format!("Basic {}", encoded)))
            },
            Self::JwtToken(token) => {
                // Authorization: Bearer <token>
                Ok(request.bearer_auth(token))
            },
            Self::None => {
                // No authentication headers
                Ok(request)
            },
        }
    }

    /// Check if authentication is configured
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_provider_creation() {
        let basic = AuthProvider::basic_auth("alice".to_string(), "secret".to_string());
        assert!(basic.is_authenticated());

        let system = AuthProvider::system_user_auth("password123".to_string());
        assert!(system.is_authenticated());

        let jwt = AuthProvider::jwt_token("test_token".to_string());
        assert!(jwt.is_authenticated());

        let none = AuthProvider::none();
        assert!(!none.is_authenticated());
    }

    #[test]
    fn test_basic_auth_encoding() {
        let auth = AuthProvider::basic_auth("alice".to_string(), "secret123".to_string());

        // Create a dummy request to test header application
        let client = reqwest::Client::new();
        let request = client.get("http://localhost:8080");
        let result = auth.apply_to_request(request);

        assert!(result.is_ok());
        // Note: reqwest::RequestBuilder doesn't expose headers for inspection,
        // so we can only verify it doesn't error
    }

    #[test]
    fn test_system_user_auth_uses_root() {
        let auth = AuthProvider::system_user_auth("test_password".to_string());

        match auth {
            AuthProvider::BasicAuth(username, password) => {
                assert_eq!(username, "root");
                assert_eq!(password, "test_password");
            },
            _ => panic!("Expected BasicAuth variant"),
        }
    }

    #[test]
    fn test_basic_auth_base64_format() {
        // Manually verify the base64 encoding format
        let username = "alice";
        let password = "secret123";
        let credentials = format!("{}:{}", username, password);
        let encoded = general_purpose::STANDARD.encode(credentials.as_bytes());

        // Expected: "YWxpY2U6c2VjcmV0MTIz" (base64 of "alice:secret123")
        assert_eq!(encoded, "YWxpY2U6c2VjcmV0MTIz");
    }
}
