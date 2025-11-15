//! HTTP request authentication extractor
//!
//! Extracts and validates authentication credentials from HTTP requests,
//! supporting Basic Auth and JWT Bearer tokens.

use crate::basic_auth::parse_basic_auth_header;
use crate::error::{AuthError, AuthResult};
use crate::password;
use actix_web::HttpRequest;
use kalamdb_commons::{AuthType, Role, UserId};
use crate::user_repo::UserRepository;
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

/// Extract and validate authentication from HTTP request using a repository abstraction
///
/// Preferred path for provider-based storage. Mirrors `extract_auth` but avoids direct
/// dependency on RocksDbAdapter.
pub async fn extract_auth_with_repo(
    req: &HttpRequest,
    repo: &Arc<dyn UserRepository>,
)
-> AuthResult<AuthenticatedRequest> {
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
        extract_basic_auth_with_repo(req, repo, auth_header).await
    } else if auth_header.starts_with("Bearer") {
        // Minimal JWT support (HS256) for integration tests.
        // Reads secret from env KALAMDB_JWT_SECRET or falls back to default dev secret.
        let token = auth_header.strip_prefix("Bearer").unwrap_or("").trim();
        if token.is_empty() {
            return Err(AuthError::MalformedAuthorization("Bearer token missing".to_string()));
        }
        let secret = std::env::var("KALAMDB_JWT_SECRET")
            .unwrap_or_else(|_| "kalamdb-dev-secret-key-change-in-production".to_string());
        let trusted = std::env::var("KALAMDB_JWT_TRUSTED_ISSUERS")
            .unwrap_or_else(|_| "kalamdb-test".to_string());
        let issuers: Vec<String> = trusted.split(',').map(|s| s.trim().to_string()).collect();
        match crate::jwt_auth::validate_jwt_token(token, &secret, &issuers) {
            Ok(claims) => {
                // Look up user by username to avoid mismatched sub causing 500 later
                if let Some(uname) = claims.username.clone() {
                    if let Ok(user) = repo.get_user_by_username(&uname).await {
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
                        return Ok(AuthenticatedRequest {
                            user_id: user.id.clone(),
                            role,
                            username: user.username.to_string(),
                        });
                    } else {
                        return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
                    }
                }
                Err(AuthError::MissingClaim("username".to_string()))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(AuthError::MalformedAuthorization(
            "Authorization header must start with 'Basic ' or 'Bearer '".to_string(),
        ))
    }
}



/// Extract and validate Basic Auth credentials using a UserRepository
async fn extract_basic_auth_with_repo(
    req: &HttpRequest,
    repo: &Arc<dyn UserRepository>,
    auth_header: &str,
) -> AuthResult<AuthenticatedRequest> {
    let (username, password_plain) = parse_basic_auth_header(auth_header)?;

    let user = repo.get_user_by_username(&username).await?;

    if user.deleted_at.is_some() {
        return Err(AuthError::InvalidCredentials(
            "Invalid username or password".to_string(),
        ));
    }

    let client_ip = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .or_else(|| req.peer_addr().map(|addr| addr.ip().to_string()));

    let is_localhost = client_ip
        .as_deref()
        .map(|ip| ip == "127.0.0.1" || ip == "::1" || ip.starts_with("127.") || ip == "localhost")
        .unwrap_or(false);

    let is_system_internal = user.role == Role::System && user.auth_type == AuthType::Internal;

    let allow_remote = user
        .auth_data
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("allow_remote").and_then(|b| b.as_bool()))
        .unwrap_or(false);

    if is_system_internal {
        if is_localhost {
            if user.password_hash.is_empty() {
                return Ok(AuthenticatedRequest {
                    user_id: user.id.clone(),
                    role: user.role,
                    username: user.username.to_string(),
                });
            } else {
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
