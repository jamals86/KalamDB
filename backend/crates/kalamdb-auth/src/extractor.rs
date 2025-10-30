//! HTTP request authentication extractor
//!
//! Extracts and validates authentication credentials from HTTP requests,
//! supporting Basic Auth and JWT Bearer tokens.

use crate::basic_auth::parse_basic_auth_header;
use crate::error::{AuthError, AuthResult};
use crate::password;
use actix_web::HttpRequest;
use kalamdb_commons::{AuthType, Role, UserId};
use kalamdb_sql::RocksDbAdapter;
use log::warn;
use std::sync::Arc;

/// Authenticated user information extracted from HTTP request
#[derive(Debug, Clone)]
pub struct AuthenticatedRequest {
    /// User ID
    pub user_id: UserId,
    /// User role
    pub role: Role,
    /// Username
    pub username: String,
}

/// Extract and validate authentication from HTTP request
///
/// Supports:
/// - HTTP Basic Auth (Authorization: Basic <base64>)
/// - JWT Bearer Token (Authorization: Bearer <token>) - TODO: Not yet implemented
///
/// # Arguments
/// * `req` - The HTTP request
/// * `adapter` - RocksDB adapter for user lookup
///
/// # Returns
/// Authenticated user information if valid, error otherwise
///
/// # Errors
/// - `AuthError::MissingAuthorization` if no Authorization header
/// - `AuthError::InvalidCredentials` if credentials are invalid
/// - `AuthError::UserNotFound` if user doesn't exist
/// - `AuthError::RemoteAccessDenied` if system user tries to auth remotely
pub async fn extract_auth(
    req: &HttpRequest,
    adapter: &Arc<RocksDbAdapter>,
) -> AuthResult<AuthenticatedRequest> {
    // Get Authorization header
    let auth_header = req
        .headers()
        .get("Authorization")
        .ok_or_else(|| {
            AuthError::MissingAuthorization(
                "Authorization header is required. Use 'Authorization: Basic <credentials>' or 'Authorization: Bearer <token>'".to_string(),
            )
        })?
        .to_str()
        .map_err(|_| {
            AuthError::MalformedAuthorization(
                "Authorization header contains invalid characters".to_string(),
            )
        })?;

    if auth_header.starts_with("Basic ") {
        extract_basic_auth(req, adapter, auth_header).await
    } else if auth_header.starts_with("Bearer ") {
        Err(AuthError::MalformedAuthorization(
            "JWT authentication not yet implemented".to_string(),
        ))
    } else {
        Err(AuthError::MalformedAuthorization(
            "Authorization header must start with 'Basic ' or 'Bearer '".to_string(),
        ))
    }
}

/// Extract and validate Basic Auth credentials
async fn extract_basic_auth(
    req: &HttpRequest,
    adapter: &Arc<RocksDbAdapter>,
    auth_header: &str,
) -> AuthResult<AuthenticatedRequest> {
    // Parse Basic Auth header
    let (username, password_plain) = parse_basic_auth_header(auth_header)?;

    // Look up user in database
    let user = adapter
        .get_user(&username)
        .map_err(|e| {
            warn!("Database error looking up user '{}': {}", username, e);
            AuthError::DatabaseError(format!("Failed to look up user: {}", e))
        })?
        .ok_or_else(|| AuthError::InvalidCredentials("Invalid username or password".to_string()))?;

    // Check if user is deleted (soft delete)
    if user.deleted_at.is_some() {
        return Err(AuthError::InvalidCredentials(
            "Invalid username or password".to_string(),
        ));
    }

    // Determine client IP address
    let client_ip = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .or_else(|| req.peer_addr().map(|addr| addr.ip().to_string()));

    let is_localhost = client_ip
        .as_deref()
        .map(|ip| {
            ip == "127.0.0.1" || ip == "::1" || ip.starts_with("127.") || ip == "localhost"
        })
        .unwrap_or(false);

    let is_system_internal = user.role == Role::System && user.auth_type == AuthType::Internal;

    // Parse per-user allow_remote flag from auth_data JSON, default false
    let allow_remote = user
        .auth_data
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("allow_remote").and_then(|b| b.as_bool()))
        .unwrap_or(false);

    // System/internal users have special authentication rules
    if is_system_internal {
        if is_localhost {
            // Localhost system user: can authenticate without password if hash is empty
            if user.password_hash.is_empty() {
                // Passwordless localhost auth for system user
                return Ok(AuthenticatedRequest {
                    user_id: user.id.clone(),
                    role: user.role,
                    username: user.username.to_string(),
                });
            } else {
                // Verify password if set
                if password::verify_password(&password_plain, &user.password_hash)
                    .await
                    .unwrap_or(false)
                {
                    return Ok(AuthenticatedRequest {
                        user_id: user.id.clone(),
                        role: user.role,
                        username: user.username.to_string(),
                    });
                } else {
                    warn!("Invalid password for system user: {}", username);
                    return Err(AuthError::InvalidCredentials(
                        "Invalid username or password".to_string(),
                    ));
                }
            }
        } else {
            // Remote system user: ALWAYS require a password
            if user.password_hash.is_empty() {
                return Err(AuthError::RemoteAccessDenied(
                    "System users with empty passwords cannot authenticate remotely. Set a password with: ALTER USER root SET PASSWORD '...'".to_string(),
                ));
            }

            // Check if remote access is allowed for this user
            if !allow_remote {
                return Err(AuthError::RemoteAccessDenied(
                    "Remote access is not allowed for this user".to_string(),
                ));
            }

            // Verify password
            if password::verify_password(&password_plain, &user.password_hash)
                .await
                .unwrap_or(false)
            {
                return Ok(AuthenticatedRequest {
                    user_id: user.id.clone(),
                    role: user.role,
                    username: user.username.to_string(),
                });
            } else {
                warn!("Invalid password for remote system user: {}", username);
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            }
        }
    } else {
        // Non-system users: require password
        if !user.password_hash.is_empty() {
            if password::verify_password(&password_plain, &user.password_hash)
                .await
                .unwrap_or(false)
            {
                return Ok(AuthenticatedRequest {
                    user_id: user.id.clone(),
                    role: user.role,
                    username: user.username.to_string(),
                });
            } else {
                warn!("Invalid password for user: {}", username);
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            }
        } else {
            // User has no password set - deny access
            return Err(AuthError::InvalidCredentials(
                "Invalid username or password".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authenticated_request_creation() {
        let auth_req = AuthenticatedRequest {
            user_id: UserId::from("user123"),
            role: Role::User,
            username: "testuser".to_string(),
        };

        assert_eq!(auth_req.user_id.as_str(), "user123");
        assert_eq!(auth_req.role, Role::User);
        assert_eq!(auth_req.username, "testuser");
    }
}
