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
//!                    │  - Credentials{...}    → WebSocket Basic auth        │
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
//!    pub enum AuthRequest {
//!        // ... existing variants ...
//!        ApiKey { key: String },
//!    }
//!    ```
//!
//! 2. **Add variant to `AuthMethod`** (if tracking method used):
//!    ```rust,ignore
//!    pub enum AuthMethod {
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

use crate::basic_auth;
use crate::context::AuthenticatedUser;
use crate::error::{AuthError, AuthResult};
use crate::jwt_auth;
use crate::password;
use crate::user_repo::UserRepository;
use kalamdb_commons::models::ConnectionInfo;
use kalamdb_commons::{AuthType, Role};
use log::debug;
use once_cell::sync::Lazy;
use std::sync::Arc;

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
    
    /// Direct username/password (WebSocket authenticate message)
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
            WsAuthCredentials::Basic { username, password } => {
                AuthRequest::Credentials { username, password }
            }
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
        AuthRequest::Jwt { token } => {
            // Direct JWT token authentication (from WebSocket)
            let user = authenticate_bearer(&token, connection_info, repo).await?;
            Ok(AuthenticationResult {
                user,
                method: AuthMethod::Bearer,
            })
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
        // Security: Use generic message to prevent username enumeration
        debug!("Authentication failed for user attempt");
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
                // Security: Generic message prevents username enumeration
                debug!("Authentication failed for system user attempt");
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
                // Security: Generic message prevents username enumeration
                debug!("Authentication failed for remote user attempt");
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
            // Security: Generic message prevents username enumeration
            debug!("Authentication failed for user attempt");
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

/// Cached JWT configuration for performance
///
/// Reading environment variables on every request is expensive.
/// This lazy static caches the configuration at first use.
struct JwtConfig {
    secret: String,
    trusted_issuers: Vec<String>,
}

static JWT_CONFIG: Lazy<JwtConfig> = Lazy::new(|| {
    let secret = std::env::var("KALAMDB_JWT_SECRET")
        .unwrap_or_else(|_| "kalamdb-dev-secret-key-change-in-production".to_string());
    let trusted = std::env::var("KALAMDB_JWT_TRUSTED_ISSUERS")
        .unwrap_or_else(|_| "kalamdb-test".to_string());
    let trusted_issuers: Vec<String> = trusted.split(',').map(|s| s.trim().to_string()).collect();
    
    JwtConfig {
        secret,
        trusted_issuers,
    }
});

/// Authenticate using JWT Bearer token
///
/// Uses cached JWT configuration for performance (avoids env var reads per request).
async fn authenticate_bearer(
    token: &str,
    connection_info: &ConnectionInfo,
    repo: &Arc<dyn UserRepository>,
) -> AuthResult<AuthenticatedUser> {
    // Use cached JWT configuration
    let config = &*JWT_CONFIG;
    let secret = &config.secret;
    let issuers = &config.trusted_issuers;

    // Validate JWT
    let claims = jwt_auth::validate_jwt_token(token, secret, issuers)?;

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
            } else if header.starts_with("Bearer ") {
                // Try to extract username from JWT claims without validation
                extract_jwt_username_unsafe(header.strip_prefix("Bearer ").unwrap_or(""))
            } else {
                "unknown".to_string()
            }
        }
        AuthRequest::Credentials { username, .. } => username.clone(),
        AuthRequest::Jwt { token } => extract_jwt_username_unsafe(token),
    }
}

/// Extract username from JWT token without validation (for audit logging only)
fn extract_jwt_username_unsafe(token: &str) -> String {
    // JWT format: header.payload.signature
    // We decode the payload without verification for audit purposes only
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return "unknown".to_string();
    }

    // Decode payload (base64url)
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    if let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(parts[1]) {
        if let Ok(payload_str) = String::from_utf8(payload_bytes) {
            if let Ok(claims) = serde_json::from_str::<serde_json::Value>(&payload_str) {
                if let Some(username) = claims.get("username").and_then(|v| v.as_str()) {
                    return username.to_string();
                }
                if let Some(sub) = claims.get("sub").and_then(|v| v.as_str()) {
                    return sub.to_string();
                }
            }
        }
    }
    "unknown".to_string()
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
        assert_eq!(extract_username_for_audit(&request), "jwt_user");
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
        assert_eq!(extract_username_for_audit(&request), "user_from_sub");
    }

    #[test]
    fn test_extract_username_from_invalid_jwt() {
        let request = AuthRequest::Jwt {
            token: "invalid_token".to_string(),
        };
        assert_eq!(extract_username_for_audit(&request), "unknown");
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
        assert_eq!(extract_username_for_audit(&request), "bearer_user");
    }

    #[cfg(feature = "websocket")]
    #[test]
    fn test_from_ws_auth_credentials_basic() {
        use kalamdb_commons::websocket::WsAuthCredentials;

        let ws_creds = WsAuthCredentials::Basic {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
        };

        let auth_request: AuthRequest = ws_creds.into();
        match auth_request {
            AuthRequest::Credentials { username, password } => {
                assert_eq!(username, "testuser");
                assert_eq!(password, "testpass");
            }
            _ => panic!("Expected Credentials variant"),
        }
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
            }
            _ => panic!("Expected Jwt variant"),
        }
    }
}
