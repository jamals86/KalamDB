//! Unified authentication module for HTTP and WebSocket handlers
//!
//! This module provides a single entry point for authentication that can be used
//! by both HTTP REST endpoints and WebSocket handlers. When adding a new authentication
//! method (e.g., API keys, mTLS), you only need to update this module.

use crate::basic_auth;
use crate::context::AuthenticatedUser;
use crate::error::{AuthError, AuthResult};
use crate::jwt_auth;
use crate::password;
use crate::user_repo::UserRepository;
use kalamdb_commons::models::ConnectionInfo;
use kalamdb_commons::{AuthType, Role};
use log::warn;
use std::sync::Arc;

/// Authentication method detected from request
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// HTTP Basic Authentication (username:password base64 encoded)
    Basic,
    /// JWT Bearer token
    Bearer,
    /// Direct username/password (for WebSocket)
    Direct,
}

/// Authentication request that can come from HTTP or WebSocket
#[derive(Debug, Clone)]
pub enum AuthRequest {
    /// HTTP Authorization header (Basic or Bearer)
    Header(String),
    /// Direct username/password (WebSocket authenticate message)
    Credentials { username: String, password: String },
}

/// Result of authentication including method used
#[derive(Debug, Clone)]
pub struct AuthenticationResult {
    /// The authenticated user
    pub user: AuthenticatedUser,
    /// The method used for authentication
    pub method: AuthMethod,
}

/// Authenticate a request using the unified authentication flow
///
/// This function handles all authentication methods:
/// - HTTP Basic Auth (from Authorization header)
/// - JWT Bearer tokens (from Authorization header)
/// - Direct username/password (from WebSocket messages)
///
/// # Arguments
/// * `request` - The authentication request (header or direct credentials)
/// * `connection_info` - Connection information (IP address for localhost checks)
/// * `repo` - User repository for looking up user records
///
/// # Returns
/// * `Ok(AuthenticationResult)` - Successful authentication with user and method
/// * `Err(AuthError)` - Authentication failed
pub async fn authenticate(
    request: AuthRequest,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticationResult> {
    match request {
        AuthRequest::Header(header) => authenticate_header(&header, connection_info, repo).await,
        AuthRequest::Credentials { username, password } => {
            authenticate_credentials(&username, &password, connection_info, repo).await
        }
    }
}

/// Authenticate using an Authorization header (Basic or Bearer)
async fn authenticate_header(
    auth_header: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticationResult> {
    if auth_header.starts_with("Basic ") {
        let user = authenticate_basic(auth_header, connection_info, repo).await?;
        Ok(AuthenticationResult {
            user,
            method: AuthMethod::Basic,
        })
    } else if auth_header.starts_with("Bearer") {
        // Handle both "Bearer " and malformed "Bearer" without space
        let token = auth_header.strip_prefix("Bearer").unwrap_or("").trim();
        if token.is_empty() {
            return Err(AuthError::MalformedAuthorization(
                "Bearer token missing".to_string(),
            ));
        }
        let user = authenticate_bearer(token, connection_info, repo).await?;
        Ok(AuthenticationResult {
            user,
            method: AuthMethod::Bearer,
        })
    } else {
        Err(AuthError::MalformedAuthorization(
            "Authorization header must start with 'Basic ' or 'Bearer '".to_string(),
        ))
    }
}

/// Authenticate using direct username/password (for WebSocket)
async fn authenticate_credentials(
    username: &str,
    password: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticationResult> {
    let user = authenticate_username_password(username, password, connection_info, repo).await?;
    Ok(AuthenticationResult {
        user,
        method: AuthMethod::Direct,
    })
}

/// Authenticate using Basic Auth header
async fn authenticate_basic(
    auth_header: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    let (username, password) = basic_auth::parse_basic_auth_header(auth_header)?;
    authenticate_username_password(&username, &password, connection_info, repo).await
}

/// Core authentication logic for username/password
///
/// This is the central authentication function that handles:
/// - User lookup
/// - Deleted user check
/// - System/internal user localhost restrictions
/// - Password verification
/// - Remote access policies
async fn authenticate_username_password(
    username: &str,
    password: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    // Look up user
    let user = repo.get_user_by_username(username).await?;

    // Check if user is deleted
    if user.deleted_at.is_some() {
        warn!("Attempt to authenticate deleted user: {}", username);
        return Err(AuthError::InvalidCredentials(
            "Invalid username or password".to_string(),
        ));
    }

    // OAuth users cannot use password auth
    if user.auth_type == AuthType::OAuth {
        return Err(AuthError::AuthenticationFailed(
            "OAuth users cannot authenticate with password. Use OAuth token instead.".to_string(),
        ));
    }

    let is_localhost = connection_info.is_localhost();
    let is_system_internal = user.role == Role::System && user.auth_type == AuthType::Internal;

    // Parse allow_remote from auth_data
    let allow_remote = user
        .auth_data
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("allow_remote").and_then(|b| b.as_bool()))
        .unwrap_or(false);

    if is_system_internal {
        if is_localhost {
            // Localhost system users: accept empty password OR valid password
            let password_ok = user.password_hash.is_empty()
                || password::verify_password(password, &user.password_hash)
                    .await
                    .unwrap_or(false);

            if !password_ok {
                warn!("Invalid password for system user: {}", username);
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            }
        } else {
            // Remote system users
            if user.password_hash.is_empty() {
                return Err(AuthError::RemoteAccessDenied(
                    "System users with empty passwords cannot authenticate remotely. Set a password with: ALTER USER root SET PASSWORD '...'".to_string(),
                ));
            }
            if !allow_remote {
                return Err(AuthError::RemoteAccessDenied(
                    "Remote access is not allowed for this user".to_string(),
                ));
            }
            if !password::verify_password(password, &user.password_hash)
                .await
                .unwrap_or(false)
            {
                warn!("Invalid password for remote system user: {}", username);
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            }
        }
    } else {
        // Regular users must have a password
        if user.password_hash.is_empty() {
            return Err(AuthError::InvalidCredentials(
                "Invalid username or password".to_string(),
            ));
        }
        if !password::verify_password(password, &user.password_hash)
            .await
            .unwrap_or(false)
        {
            warn!("Invalid password for user: {}", username);
            return Err(AuthError::InvalidCredentials(
                "Invalid username or password".to_string(),
            ));
        }
    }

    Ok(AuthenticatedUser::new(
        user.id.clone(),
        user.username.as_str().to_string(),
        user.role,
        user.email.clone(),
        connection_info.clone(),
    ))
}

/// Authenticate using JWT Bearer token
async fn authenticate_bearer(
    token: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    // Get JWT configuration from environment
    let secret = std::env::var("KALAMDB_JWT_SECRET")
        .unwrap_or_else(|_| "kalamdb-dev-secret-key-change-in-production".to_string());
    let trusted = std::env::var("KALAMDB_JWT_TRUSTED_ISSUERS")
        .unwrap_or_else(|_| "kalamdb-test".to_string());
    let issuers: Vec<String> = trusted.split(',').map(|s| s.trim().to_string()).collect();

    // Validate JWT
    let claims = jwt_auth::validate_jwt_token(token, &secret, &issuers)?;

    // Get username from claims
    let username = claims
        .username
        .as_ref()
        .ok_or_else(|| AuthError::MissingClaim("username".to_string()))?;

    // Look up user
    let user = repo.get_user_by_username(username).await?;

    if user.deleted_at.is_some() {
        return Err(AuthError::InvalidCredentials(
            "Invalid username or password".to_string(),
        ));
    }

    // Override role from claims if present
    let role = claims
        .role
        .as_deref()
        .and_then(|r| match r.to_lowercase().as_str() {
            "system" => Some(Role::System),
            "dba" => Some(Role::Dba),
            "service" => Some(Role::Service),
            "user" => Some(Role::User),
            _ => None,
        })
        .unwrap_or(user.role);

    Ok(AuthenticatedUser::new(
        user.id.clone(),
        user.username.as_str().to_string(),
        role,
        user.email.clone(),
        connection_info.clone(),
    ))
}

/// Extract username from an Authorization header for audit logging
///
/// This is useful when authentication fails but we still want to log
/// the attempted username for security auditing.
pub fn extract_username_for_audit(request: &AuthRequest) -> String {
    match request {
        AuthRequest::Header(header) => {
            if header.starts_with("Basic ") {
                basic_auth::parse_basic_auth_header(header)
                    .map(|(u, _)| u)
                    .unwrap_or_else(|_| "unknown".to_string())
            } else {
                "unknown".to_string()
            }
        }
        AuthRequest::Credentials { username, .. } => username.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_method_debug() {
        assert_eq!(format!("{:?}", AuthMethod::Basic), "Basic");
        assert_eq!(format!("{:?}", AuthMethod::Bearer), "Bearer");
        assert_eq!(format!("{:?}", AuthMethod::Direct), "Direct");
    }

    #[test]
    fn test_extract_username_from_credentials() {
        let request = AuthRequest::Credentials {
            username: "testuser".to_string(),
            password: "secret".to_string(),
        };
        assert_eq!(extract_username_for_audit(&request), "testuser");
    }

    #[test]
    fn test_extract_username_from_basic_header() {
        // "testuser:password" base64 encoded
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "testuser:password");
        let request = AuthRequest::Header(format!("Basic {}", encoded));
        assert_eq!(extract_username_for_audit(&request), "testuser");
    }

    #[test]
    fn test_extract_username_from_bearer_header() {
        let request = AuthRequest::Header("Bearer some.jwt.token".to_string());
        assert_eq!(extract_username_for_audit(&request), "unknown");
    }
}
