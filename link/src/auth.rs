//! Authentication provider for KalamDB client.
//!
//! Handles JWT tokens and API key authentication, attaching appropriate
//! headers to HTTP requests.

use crate::error::Result;

/// Authentication credentials for KalamDB server.
///
/// Supports both JWT tokens and API keys. The auth provider automatically
/// attaches the `X-USER-ID` header based on the authentication method.
///
/// # Examples
///
/// ```rust
/// use kalam_link::AuthProvider;
///
/// // JWT token authentication
/// let auth = AuthProvider::jwt_token("eyJhbGc...".to_string());
///
/// // API key authentication
/// let auth = AuthProvider::api_key("kalam_1234567890".to_string());
///
/// // No authentication (localhost bypass mode)
/// let auth = AuthProvider::none();
/// ```
#[derive(Debug, Clone)]
pub enum AuthProvider {
    /// JWT token authentication
    JwtToken(String),

    /// API key authentication
    ApiKey(String),

    /// No authentication (localhost bypass)
    None,
}

impl AuthProvider {
    /// Create JWT token authentication
    pub fn jwt_token(token: String) -> Self {
        Self::JwtToken(token)
    }

    /// Create API key authentication
    pub fn api_key(key: String) -> Self {
        Self::ApiKey(key)
    }

    /// No authentication (for localhost bypass mode)
    pub fn none() -> Self {
        Self::None
    }

    /// Attach authentication headers to an HTTP request builder
    pub fn apply_to_request(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        match self {
            Self::JwtToken(token) => {
                // Extract user_id from JWT token payload (simplified - real impl would decode JWT)
                // For now, attach the full token and let server decode
                Ok(request.bearer_auth(token))
            }
            Self::ApiKey(key) => {
                // Attach API key as custom header
                Ok(request.header("X-API-KEY", key))
            }
            Self::None => {
                // No authentication headers
                Ok(request)
            }
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
        let jwt = AuthProvider::jwt_token("test_token".to_string());
        assert!(jwt.is_authenticated());

        let api = AuthProvider::api_key("test_key".to_string());
        assert!(api.is_authenticated());

        let none = AuthProvider::none();
        assert!(!none.is_authenticated());
    }
}
