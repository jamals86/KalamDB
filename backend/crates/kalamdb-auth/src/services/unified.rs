//! Unified authentication module for HTTP and WebSocket handlers
//!
//! This module provides a single entry point for authentication that can be used
//! by both HTTP REST endpoints and WebSocket handlers.
//!
//! # Architecture
//!
//! ```text
//!                    ┌─────────────────────────────────────────────────────┐
//!                    │              AuthRequest (Single Entry Point)        │
//!                    │  - Header(String)      → HTTP Authorization header   │
//!                    │  - Credentials{...}    → Login flow credentials      │
//!                    │  - Jwt{token}          → WebSocket JWT auth          │
//!                    └─────────────────────────────────────────────────────┘
//!                                         │
//!                                         ▼
//!                    ┌─────────────────────────────────────────────────────┐
//!                    │              authenticate() function                 │
//!                    │  Routes to appropriate handler based on variant      │
//!                    └─────────────────────────────────────────────────────┘
//!                                         │
//!         ┌───────────────────────────────┼───────────────────────────────┐
//!         ▼                               ▼                               ▼
//!   ┌──────────────┐              ┌──────────────┐              ┌──────────────┐
//!   │ Basic Auth   │              │ JWT/Bearer   │              │ Direct Creds │
//!   │ (username:   │              │ (token       │              │ (WebSocket)  │
//!   │  password)   │              │  validation) │              │              │
//!   └──────────────┘              └──────────────┘              └──────────────┘
//! ```
//!
//! # Adding a New Authentication Method
//!
//! To add a new authentication method (e.g., API keys, OAuth, mTLS):
//!
//! 1. **Add variant to `AuthRequest`** in this file:
//!    ```rust,ignore
//!    enum AuthRequest {
//!        // ... existing variants ...
//!        ApiKey { key: String },
//!    }
//!    ```
//!
//! 2. **Add variant to `AuthMethod`** (if tracking method used):
//!    ```rust,ignore
//!    enum AuthMethod {
//!        // ... existing variants ...
//!        ApiKey,
//!    }
//!    ```
//!
//! 3. **Add handler in `authenticate()`**:
//!    ```rust,ignore
//!    AuthRequest::ApiKey { key } => {
//!        let user = authenticate_api_key(&key, connection_info, repo).await?;
//!        Ok(AuthenticationResult { user, method: AuthMethod::ApiKey })
//!    }
//!    ```
//!
//! 4. **Update `extract_username_for_audit()`** for logging.
//!
//! 5. **Update `WsAuthCredentials`** in `kalamdb-commons/websocket.rs` if WebSocket support needed.
//!
//! 6. **Update the `From<WsAuthCredentials>` impl** below.
//!
//! 7. **Update client SDKs** (TypeScript, WASM) to support the new method.

use crate::errors::error::{AuthError, AuthResult};
use crate::helpers::basic_auth;
use crate::models::context::AuthenticatedUser;
use crate::providers::jwt_auth;
use crate::providers::jwt_config;
use crate::repository::user_repo::UserRepository;
use crate::security::password;
use crate::services::login_tracker::LoginTracker;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use kalamdb_commons::constants::AuthConstants;
use kalamdb_commons::models::ConnectionInfo;
use kalamdb_commons::models::UserName;
use kalamdb_commons::{AuthType, Role};
use log::debug;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tracing::Instrument;

/// Cached login tracker instance
static LOGIN_TRACKER: Lazy<LoginTracker> = Lazy::new(LoginTracker::new);

/// Authentication method detected from request
///
/// Used for audit logging and tracking which authentication mechanism was used.
/// Add new variants here when implementing additional auth methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
    /// HTTP Basic Authentication (username:password base64 encoded)
    Basic,
    /// JWT Bearer token
    Bearer,
    /// Direct username/password (for WebSocket)
    Direct,
    // Future auth methods:
    // ApiKey,
    // OAuth,
    // Mtls,
}

/// Authentication request that can come from HTTP or WebSocket
///
/// This is the **single entry point** for all authentication in KalamDB.
/// Both HTTP handlers and WebSocket handlers convert their input to this enum.
///
/// # Variants
///
/// - `Header` - Raw HTTP Authorization header (parses Basic or Bearer)
/// - `Credentials` - Direct username/password (from WebSocket JSON)
/// - `Jwt` - Direct JWT token (from WebSocket JSON)
///
/// # Example
///
/// ```rust,ignore
/// // HTTP handler
/// let auth_request = AuthRequest::Header(authorization_header);
///
/// // WebSocket handler (via From<WsAuthCredentials>)
/// let auth_request: AuthRequest = ws_credentials.into();
///
/// // Authenticate
/// let result = authenticate(auth_request, &connection_info, &repo).await?;
/// ```
#[derive(Debug, Clone)]
pub enum AuthRequest {
    /// HTTP Authorization header (Basic or Bearer)
    /// Automatically parsed to determine auth method
    Header(String),

    /// Direct username/password (login flow)
    /// Bypasses header parsing for structured JSON input
    Credentials { username: String, password: String },

    /// Direct JWT token (WebSocket authenticate message)
    /// Bypasses header parsing for structured JSON input
    Jwt { token: String },
    // Future auth methods can be added here:
    // ApiKey { key: String },
    // OAuth { provider: String, token: String },
}

/// Convert WebSocket authentication credentials to unified AuthRequest
///
/// This impl enables seamless conversion from the client-facing `WsAuthCredentials`
/// (used in WebSocket JSON messages) to the internal `AuthRequest` type.
///
/// When adding a new authentication method:
/// 1. Add variant to `WsAuthCredentials` in `kalamdb-commons/websocket.rs`
/// 2. Add corresponding variant to `AuthRequest` above
/// 3. Add conversion case in this `From` impl
#[cfg(feature = "websocket")]
impl From<kalamdb_commons::websocket::WsAuthCredentials> for AuthRequest {
    fn from(creds: kalamdb_commons::websocket::WsAuthCredentials) -> Self {
        use kalamdb_commons::websocket::WsAuthCredentials;
        match creds {
            WsAuthCredentials::Jwt { token } => AuthRequest::Jwt { token },
            // Future auth methods:
            // WsAuthCredentials::ApiKey { key } => AuthRequest::ApiKey { key },
        }
    }
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
/// - Direct username/password (login flow)
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
    let request_kind = match &request {
        AuthRequest::Header(_) => "header",
        AuthRequest::Credentials { .. } => "credentials",
        AuthRequest::Jwt { .. } => "jwt",
    };
    let span = tracing::info_span!(
        "auth.check",
        auth_request_kind = request_kind,
        is_localhost = connection_info.is_localhost(),
        role = tracing::field::Empty, // Fill in role after authentication
        user = tracing::field::Empty  // Fill in username after authentication
    );

    async move {
        match request {
            AuthRequest::Header(header) => {
                authenticate_header(&header, connection_info, repo).await
            },
            AuthRequest::Credentials { username, password } => {
                authenticate_credentials(&username, &password, connection_info, repo).await
            },
            AuthRequest::Jwt { token } => {
                // Direct JWT token authentication (from WebSocket)
                let user = authenticate_bearer(&token, connection_info, repo).await?;

                // Update span with authenticated role for better observability
                tracing::Span::current().record("role", format!("{:?}", user.role).as_str());
                tracing::Span::current().record("user", user.username.as_str());

                Ok(AuthenticationResult {
                    user,
                    method: AuthMethod::Bearer,
                })
            },
        }
    }
    .instrument(span)
    .await
}

/// Authenticate using an Authorization header (Basic or Bearer)
async fn authenticate_header(
    auth_header: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticationResult> {
    if auth_header.starts_with("Basic ") {
        Err(AuthError::InvalidCredentials(
            "Basic authentication is not supported. Use Bearer token or login endpoint."
                .to_string(),
        ))
    } else if auth_header.starts_with("Bearer") {
        // Handle both "Bearer " and malformed "Bearer" without space
        let token = auth_header.strip_prefix("Bearer").unwrap_or("").trim();
        if token.is_empty() {
            return Err(AuthError::MalformedAuthorization("Bearer token missing".to_string()));
        }
        let user = authenticate_bearer(token, connection_info, repo).await?;

        // Update span with authenticated role for better observability
        tracing::Span::current().record("role", format!("{:?}", user.role).as_str());
        tracing::Span::current().record("user", user.username.as_str());

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
#[allow(dead_code)]
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
/// - Account lockout check
/// - System/internal user localhost restrictions
/// - Password verification
/// - Remote access policies
/// - Login tracking (failed attempts, successful logins)
/// - Setup required check (root with no password)
async fn authenticate_username_password(
    username: &str,
    password: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    let span = tracing::info_span!(
        "auth.username_password",
        username = username,
        is_localhost = connection_info.is_localhost()
    );
    async move {
        if username.trim().is_empty() {
            return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
        }

        // Look up user
        let username_typed = kalamdb_commons::models::UserName::from(username);
        let mut user = repo.get_user_by_username(&username_typed).await?;

        // Check if user is deleted
        if user.deleted_at.is_some() {
            // Security: Use generic message to prevent username enumeration
            debug!("Authentication failed for user attempt");
            return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
        }

        // Check if account is locked BEFORE password verification
        LOGIN_TRACKER.check_lockout(&user)?;

        // OAuth users cannot use password auth
        if user.auth_type == AuthType::OAuth {
            return Err(AuthError::AuthenticationFailed(
                "OAuth users cannot authenticate with password. Use OAuth token instead."
                    .to_string(),
            ));
        }

        let is_localhost = connection_info.is_localhost();
        let is_system_internal = user.role == Role::System && user.auth_type == AuthType::Internal;

        // Check if root user has no password configured - require setup
        // This applies even for localhost connections
        if username == AuthConstants::DEFAULT_SYSTEM_USERNAME
            && user.password_hash.is_empty()
            && password.is_empty()
        {
            return Err(AuthError::SetupRequired(
                "Server requires initial setup. Root password is not configured.".to_string(),
            ));
        }

        // Track whether authentication succeeded
        let mut auth_success = false;

        if is_system_internal {
            if is_localhost {
                // Localhost system users: require valid password (no more empty password bypass)
                let password_ok = !password.is_empty()
                    && !user.password_hash.is_empty()
                    && password::verify_password(password, &user.password_hash)
                        .await
                        .unwrap_or(false);

                if password_ok {
                    auth_success = true;
                } else {
                    // Security: Generic message prevents username enumeration
                    debug!("Authentication failed for system user attempt");
                }
            } else {
                // Remote system users
                if user.password_hash.is_empty() {
                    return Err(AuthError::RemoteAccessDenied(
                        "System users with empty passwords cannot authenticate remotely. Set a password with: ALTER USER root SET PASSWORD '...'".to_string(),
                    ));
                }
                if !password.is_empty()
                    && password::verify_password(password, &user.password_hash)
                        .await
                        .unwrap_or(false)
                {
                    auth_success = true;
                } else {
                    // Security: Generic message prevents username enumeration
                    debug!("Authentication failed for remote user attempt");
                }
            }
        } else {
            // Regular users must have a password
            if user.password_hash.is_empty() {
                return Err(AuthError::InvalidCredentials(
                    "Invalid username or password".to_string(),
                ));
            } else if !password.is_empty()
                && password::verify_password(password, &user.password_hash)
                    .await
                    .unwrap_or(false)
            {
                auth_success = true;
            } else {
                // Security: Generic message prevents username enumeration
                debug!("Authentication failed for user attempt");
            }
        }

        if !auth_success {
            tracing::warn!(username = username, "Password authentication failed");
            // Record failed login attempt (fire and forget, don't fail auth on tracking error)
            if let Err(e) = LOGIN_TRACKER.record_failed_login(&mut user, repo).await {
                log::error!("Failed to record failed login: {}", e);
            }
            return Err(AuthError::InvalidCredentials(
                "Invalid username or password".to_string(),
            ));
        }

        // Record successful login (fire and forget, don't fail auth on tracking error)
        if let Err(e) = LOGIN_TRACKER.record_successful_login(&mut user, repo).await {
            log::error!("Failed to record successful login: {}", e);
        }
        tracing::debug!(username = %user.username, role = ?user.role, "Password authentication succeeded");

        Ok(AuthenticatedUser::new(
            user.user_id,
            user.username.clone(),
            user.role,
            user.email,
            connection_info.clone(),
        ))
    }
    .instrument(span)
    .await
}

/// Initialize JWT configuration from server settings.
///
/// This should be called once at startup after loading server.toml and applying
/// environment overrides. If not called, defaults are used.
pub fn init_jwt_config(secret: &str, trusted_issuers: &str) {
    jwt_config::init_jwt_config(secret, trusted_issuers);
}

fn jwt_config() -> &'static jwt_config::JwtConfig {
    jwt_config::get_jwt_config()
}

/// Authenticate using JWT Bearer token
///
/// Uses cached JWT configuration for performance (avoids env var reads per request).
/// SECURITY: Rejects refresh tokens - only access tokens (or legacy tokens without
/// a token_type claim) are accepted for API authentication.
async fn authenticate_bearer(
    token: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    let span = tracing::info_span!(
        "auth.bearer",
        token_len = token.len(),
        is_localhost = connection_info.is_localhost()
    );
    async move {
        // Use cached JWT configuration
        let config = jwt_config();
        let secret = &config.secret;
        let issuers = &config.trusted_issuers;

        // Validate JWT
        let claims = jwt_auth::validate_jwt_token(token, secret, issuers)?;

        // SECURITY: Reject refresh tokens used as access tokens.
        // Refresh tokens have token_type = "refresh" and must only be used at the
        // /v1/api/auth/refresh endpoint, not for general API authentication.
        // Legacy tokens without a token_type are allowed for backward compatibility.
        if let Some(ref tt) = claims.token_type {
            if *tt == jwt_auth::TokenType::Refresh {
                log::warn!("Refresh token used as access token for user={}", claims.sub);
                return Err(AuthError::InvalidCredentials(
                    "Refresh tokens cannot be used for API authentication".to_string(),
                ));
            }
        }

        // Get username from claims
        let username = claims
            .username
            .as_ref()
            .ok_or_else(|| AuthError::MissingClaim("username".to_string()))?;

        // Look up user
        let username_typed = UserName::from(username.as_str());
        let user = repo.get_user_by_username(&username_typed).await?;

        if user.deleted_at.is_some() {
            return Err(AuthError::InvalidCredentials("Invalid username or password".to_string()));
        }

        // SECURITY: Validate role from claims matches database
        // If JWT contains a role claim, it must match the user's actual role in the database.
        // This prevents privilege escalation attacks where an attacker modifies JWT claims.
        let role = if let Some(claimed_role) = &claims.role {
            // Role claim present - must match database
            if *claimed_role != user.role {
                log::warn!(
                    "JWT role mismatch: claimed={:?}, actual={:?} for user={}",
                    claimed_role,
                    user.role,
                    user.username
                );
                return Err(AuthError::InvalidCredentials(
                    "Token role does not match user role".to_string(),
                ));
            }
            *claimed_role
        } else {
            // No role claim - use database role
            user.role
        };
        tracing::debug!(username = %user.username, role = ?role, "Bearer authentication succeeded");

        Ok(AuthenticatedUser::new(
            user.user_id.clone(),
            user.username.clone(),
            role,
            user.email.clone(),
            connection_info.clone(),
        ))
    }
    .instrument(span)
    .await
}

/// Extract username from an Authorization header for audit logging
///
/// This is useful when authentication fails but we still want to log
/// the attempted username for security auditing.
pub fn extract_username_for_audit(request: &AuthRequest) -> UserName {
    match request {
        AuthRequest::Header(header) => {
            if header.starts_with("Basic ") {
                basic_auth::parse_basic_auth_header(header)
                    .ok()
                    .and_then(|(u, _)| UserName::try_new(u).ok())
                    .unwrap_or_else(|| UserName::from("unknown"))
            } else if header.starts_with("Bearer ") {
                // Try to extract username from JWT claims without validation
                extract_jwt_username_unsafe(header.strip_prefix("Bearer ").unwrap_or(""))
            } else {
                UserName::from("unknown")
            }
        },
        AuthRequest::Credentials { username, .. } => {
            UserName::try_new(username.clone()).unwrap_or_else(|_| UserName::from("unknown"))
        },
        AuthRequest::Jwt { token } => extract_jwt_username_unsafe(token),
    }
}

/// Extract username from JWT token without validation (for audit logging only)
fn extract_jwt_username_unsafe(token: &str) -> UserName {
    // JWT format: header.payload.signature
    // We decode the payload without verification for audit purposes only
    let mut parts = token.splitn(3, '.');
    let _header = parts.next();
    let payload = parts.next();
    let signature = parts.next();
    if payload.is_none() || signature.is_none() {
        return UserName::from("unknown");
    }

    // Decode payload (base64url)
    if let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(payload.unwrap()) {
        if let Ok(payload_str) = String::from_utf8(payload_bytes) {
            if let Ok(claims) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                if let Some(username) = claims.get("username").and_then(|v| v.as_str()) {
                    return UserName::try_new(username.to_string())
                        .unwrap_or_else(|_| UserName::from("unknown"));
                }
                if let Some(sub) = claims.get("sub").and_then(|v| v.as_str()) {
                    return UserName::try_new(sub.to_string())
                        .unwrap_or_else(|_| UserName::from("unknown"));
                }
            }
        }
    }
    UserName::from("unknown")
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
        assert_eq!(extract_username_for_audit(&request), UserName::from("testuser"));
    }

    #[test]
    fn test_extract_username_from_basic_header() {
        // "testuser:password" base64 encoded
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "testuser:password");
        let request = AuthRequest::Header(format!("Basic {}", encoded));
        assert_eq!(extract_username_for_audit(&request), UserName::from("testuser"));
    }

    #[test]
    fn test_extract_username_from_bearer_header() {
        let request = AuthRequest::Header("Bearer some.jwt.token".to_string());
        assert_eq!(extract_username_for_audit(&request), UserName::from("unknown"));
    }

    #[test]
    fn test_extract_username_from_jwt_variant() {
        // Create a valid JWT with username claim
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        // Header: {"alg":"HS256","typ":"JWT"}
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        // Payload: {"username":"jwt_user","exp":9999999999}
        let payload = URL_SAFE_NO_PAD.encode(r#"{"username":"jwt_user","exp":9999999999}"#);
        // Fake signature
        let signature = "fake_signature";

        let token = format!("{}.{}.{}", header, payload, signature);
        let request = AuthRequest::Jwt { token };
        assert_eq!(extract_username_for_audit(&request), UserName::from("jwt_user"));
    }

    #[test]
    fn test_extract_username_from_jwt_with_sub_claim() {
        // Create a valid JWT with only sub claim (no username)
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(r#"{"sub":"user_from_sub","exp":9999999999}"#);
        let signature = "fake_signature";

        let token = format!("{}.{}.{}", header, payload, signature);
        let request = AuthRequest::Jwt { token };
        assert_eq!(extract_username_for_audit(&request), UserName::from("user_from_sub"));
    }

    #[test]
    fn test_extract_username_from_invalid_jwt() {
        let request = AuthRequest::Jwt {
            token: "invalid_token".to_string(),
        };
        assert_eq!(extract_username_for_audit(&request), UserName::from("unknown"));
    }

    #[test]
    fn test_extract_username_from_bearer_header_with_jwt() {
        // Create a valid JWT with username claim
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(r#"{"username":"bearer_user","exp":9999999999}"#);
        let signature = "fake_signature";

        let token = format!("{}.{}.{}", header, payload, signature);
        let request = AuthRequest::Header(format!("Bearer {}", token));
        assert_eq!(extract_username_for_audit(&request), UserName::from("bearer_user"));
    }

    #[cfg(feature = "websocket")]
    #[test]
    fn test_from_ws_auth_credentials_jwt() {
        use kalamdb_commons::websocket::WsAuthCredentials;

        let ws_creds = WsAuthCredentials::Jwt {
            token: "my.jwt.token".to_string(),
        };

        let auth_request: AuthRequest = ws_creds.into();
        match auth_request {
            AuthRequest::Jwt { token } => {
                assert_eq!(token, "my.jwt.token");
            },
            _ => panic!("Expected Jwt variant"),
        }
    }
}
