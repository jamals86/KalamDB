// Authentication handlers for Admin UI
//
// Provides login, logout, refresh, current user, and server-setup endpoints
// that use HttpOnly cookies for JWT token storage.

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use kalamdb_auth::{
    authenticate, create_and_sign_token, create_auth_cookie, create_logout_cookie,
    extract_client_ip_secure, AuthError, AuthRequest, CookieConfig, UserRepository,
};
use kalamdb_auth::helpers::cookie::extract_auth_token;
use kalamdb_auth::security::password::{hash_password, validate_password};
use kalamdb_commons::{AuthType, Role};
use kalamdb_commons::models::{StorageId, UserId, UserName};
use kalamdb_configs::AuthSettings;
use kalamdb_system::User;
use kalamdb_system::providers::storages::models::StorageMode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Maximum username length (prevent memory exhaustion)
const MAX_USERNAME_LENGTH: usize = 128;
/// Maximum password length (bcrypt limit is 72 bytes, but allow some headroom for encoding)
const MAX_PASSWORD_LENGTH: usize = 256;

/// Login request body
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    #[serde(deserialize_with = "validate_username_length")]
    pub username: String,
    #[serde(deserialize_with = "validate_password_length")]
    pub password: String,
}

fn validate_username_length<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.len() > MAX_USERNAME_LENGTH {
        return Err(serde::de::Error::custom(format!(
            "username exceeds maximum length of {} characters",
            MAX_USERNAME_LENGTH
        )));
    }
    Ok(s)
}

fn validate_password_length<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.len() > MAX_PASSWORD_LENGTH {
        return Err(serde::de::Error::custom(format!(
            "password exceeds maximum length of {} characters",
            MAX_PASSWORD_LENGTH
        )));
    }
    Ok(s)
}

/// Login response body
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: UserInfo,
    pub expires_at: String,
    /// JWT access token for SDK usage (also set as HttpOnly cookie)
    pub access_token: String,
    /// Refresh token for obtaining new access tokens (longer-lived)
    pub refresh_token: String,
    /// Refresh token expiration time in RFC3339 format
    pub refresh_expires_at: String,
}

/// User info returned in responses
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: UserId,
    pub username: UserName,
    pub role: Role,
    pub email: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Error response body
#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
    pub message: String,
}

impl AuthErrorResponse {
    /// Create a new error response
    #[inline]
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

/// POST /v1/api/auth/login
///
/// Authenticates a user and returns an HttpOnly cookie with JWT token.
/// Only allows dba and system roles to access the admin UI.
pub async fn login_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthSettings>,
    body: web::Json<LoginRequest>,
) -> HttpResponse {
    // Extract client IP with anti-spoofing checks for localhost validation
    let connection_info = extract_client_ip_secure(&req);

    // Authenticate using unified auth flow (includes localhost/empty password rules)
    let auth_request = AuthRequest::Credentials {
        username: body.username.clone(),
        password: body.password.clone(),
    };

    let auth_result = match authenticate(auth_request, &connection_info, user_repo.get_ref()).await
    {
        Ok(result) => result,
        Err(err) => return map_auth_error_to_response(err),
    };

    // Check role - only dba and system can access admin UI
    if !matches!(auth_result.user.role, Role::Dba | Role::System) {
        return HttpResponse::Forbidden().json(AuthErrorResponse::new(
            "forbidden",
            "Admin UI access requires dba or system role",
        ));
    }

    // Load full user record for response fields
    let username_typed = auth_result.user.username.clone();
    let user = match user_repo.get_user_by_username(&username_typed).await {
        Ok(user) => user,
        Err(e) => {
            log::error!("Failed to load user after authentication: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Authentication failed"));
        },
    };

    // Generate JWT access token
    let (token, _claims) = match create_and_sign_token(
        &user.user_id,
        &user.username,
        &user.role,
        user.email.as_deref(),
        Some(config.jwt_expiry_hours),
        &config.jwt_secret,
    ) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Error generating JWT: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to generate token"));
        },
    };

    // Generate refresh token (7 days by default, or 7x access token expiry)
    let refresh_expiry_hours = config.jwt_expiry_hours * 7;
    let (refresh_token, _refresh_claims) = match create_and_sign_token(
        &user.user_id,
        &user.username,
        &user.role,
        user.email.as_deref(),
        Some(refresh_expiry_hours),
        &config.jwt_secret,
    ) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Error generating refresh token: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to generate token"));
        },
    };

    // Create HttpOnly cookie
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie =
        create_auth_cookie(&token, Duration::hours(config.jwt_expiry_hours), &cookie_config);

    let expires_at = Utc::now() + Duration::hours(config.jwt_expiry_hours);
    let refresh_expires_at = Utc::now() + Duration::hours(refresh_expiry_hours);

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok().cookie(cookie).json(LoginResponse {
        user: UserInfo {
            id: user.user_id.clone(),
            username: user.username.clone(),
            role: user.role,
            email: user.email,
            created_at,
            updated_at,
        },
        expires_at: expires_at.to_rfc3339(),
        access_token: token,
        refresh_token,
        refresh_expires_at: refresh_expires_at.to_rfc3339(),
    })
}

fn map_auth_error_to_response(err: AuthError) -> HttpResponse {
    match err {
        AuthError::InvalidCredentials(_)
        | AuthError::UserNotFound(_)
        | AuthError::UserDeleted
        | AuthError::AuthenticationFailed(_) => HttpResponse::Unauthorized()
            .json(AuthErrorResponse::new("unauthorized", "Invalid username or password")),
        AuthError::SetupRequired(message) => {
            // HTTP 428 Precondition Required - server requires initial setup
            HttpResponse::build(actix_web::http::StatusCode::PRECONDITION_REQUIRED)
                .json(AuthErrorResponse::new("setup_required", message))
        },
        AuthError::AccountLocked(message) => {
            HttpResponse::Unauthorized().json(AuthErrorResponse::new("account_locked", message))
        },
        AuthError::RemoteAccessDenied(message) | AuthError::InsufficientPermissions(message) => {
            HttpResponse::Forbidden().json(AuthErrorResponse::new("forbidden", message))
        },
        AuthError::MalformedAuthorization(message)
        | AuthError::MissingAuthorization(message)
        | AuthError::MissingClaim(message)
        | AuthError::WeakPassword(message) => {
            HttpResponse::Unauthorized().json(AuthErrorResponse::new("unauthorized", message))
        },
        AuthError::TokenExpired | AuthError::InvalidSignature | AuthError::UntrustedIssuer(_) => {
            HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "Invalid username or password"))
        },
        AuthError::DatabaseError(_) | AuthError::HashingError(_) => {
            HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Authentication failed"))
        },
    }
}

/// POST /v1/api/auth/refresh
///
/// Refreshes the JWT token if the current one is still valid.
pub async fn refresh_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthSettings>,
) -> HttpResponse {
    // Extract token from cookie
    let token = match extract_auth_token(req.cookies().ok().iter().flat_map(|c| c.iter().cloned()))
    {
        Some(t) => t,
        None => {
            return HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "No auth token found"));
        },
    };

    // Validate existing token via unified auth (uses configured trusted issuers)
    let connection_info = extract_client_ip_secure(&req);
    let auth_request = AuthRequest::Jwt { token: token.clone() };
    let auth_result = match authenticate(auth_request, &connection_info, user_repo.get_ref()).await
    {
        Ok(result) => result,
        Err(err) => return map_auth_error_to_response(err),
    };

    // Verify user still exists and is active by username (we don't have find_by_id)
    let username_typed = auth_result.user.username.clone();
    let user = match user_repo.get_user_by_username(&username_typed).await {
        Ok(user) if user.deleted_at.is_none() => user,
        _ => {
            return HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "User no longer valid"));
        },
    };

    // Generate new access token
    let (new_token, _new_claims) = match create_and_sign_token(
        &user.user_id,
        &user.username,
        &user.role,
        user.email.as_deref(),
        Some(config.jwt_expiry_hours),
        &config.jwt_secret,
    ) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Error generating JWT: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to refresh token"));
        },
    };

    // Generate new refresh token (7 days by default, or 7x access token expiry)
    let refresh_expiry_hours = config.jwt_expiry_hours * 7;
    let (new_refresh_token, _refresh_claims) = match create_and_sign_token(
        &user.user_id,
        &user.username,
        &user.role,
        user.email.as_deref(),
        Some(refresh_expiry_hours),
        &config.jwt_secret,
    ) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Error generating refresh token: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to refresh token"));
        },
    };

    // Create new cookie
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie =
        create_auth_cookie(&new_token, Duration::hours(config.jwt_expiry_hours), &cookie_config);

    let expires_at = Utc::now() + Duration::hours(config.jwt_expiry_hours);
    let refresh_expires_at = Utc::now() + Duration::hours(refresh_expiry_hours);

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok().cookie(cookie).json(LoginResponse {
        user: UserInfo {
            id: user.user_id.clone(),
            username: user.username.clone(),
            role: user.role,
            email: user.email,
            created_at,
            updated_at,
        },
        expires_at: expires_at.to_rfc3339(),
        access_token: new_token,
        refresh_token: new_refresh_token,
        refresh_expires_at: refresh_expires_at.to_rfc3339(),
    })
}

/// POST /v1/api/auth/logout
///
/// Clears the authentication cookie.
pub async fn logout_handler(config: web::Data<AuthSettings>) -> HttpResponse {
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie = create_logout_cookie(&cookie_config);

    HttpResponse::Ok().cookie(cookie).json(serde_json::json!({
        "message": "Logged out successfully"
    }))
}

/// GET /v1/api/auth/me
///
/// Returns information about the currently authenticated user.
pub async fn me_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
) -> HttpResponse {
    // Extract token from cookie
    let token = match extract_auth_token(req.cookies().ok().iter().flat_map(|c| c.iter().cloned()))
    {
        Some(t) => t,
        None => {
            return HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "Not authenticated"));
        },
    };

    // Validate token via unified auth (uses configured trusted issuers)
    let connection_info = extract_client_ip_secure(&req);
    let auth_request = AuthRequest::Jwt { token };
    let auth_result = match authenticate(auth_request, &connection_info, user_repo.get_ref()).await
    {
        Ok(result) => result,
        Err(err) => return map_auth_error_to_response(err),
    };

    // Get current user info
    let username_typed = auth_result.user.username.clone();
    let user = match user_repo.get_user_by_username(&username_typed).await {
        Ok(user) if user.deleted_at.is_none() => user,
        _ => {
            return HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "User not found"));
        },
    };

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok().json(UserInfo {
        id: user.user_id.clone(),
        username: user.username.clone(),
        role: user.role,
        email: user.email,
        created_at,
        updated_at,
    })
}

/// Server setup request body
#[derive(Debug, Deserialize)]
pub struct ServerSetupRequest {
    /// Username for the new DBA user
    #[serde(deserialize_with = "validate_username_length")]
    pub username: String,
    /// Password for the new DBA user
    #[serde(deserialize_with = "validate_password_length")]
    pub password: String,
    /// Password for the root user
    #[serde(deserialize_with = "validate_password_length")]
    pub root_password: String,
    /// Email for the new DBA user (optional)
    pub email: Option<String>,
}

/// Server setup response body
#[derive(Debug, Serialize)]
pub struct ServerSetupResponse {
    pub user: UserInfo,
    pub message: String,
}

/// POST /v1/api/auth/setup
///
/// Initial server setup endpoint. Only works when:
/// 1. Root user has no password set (empty password_hash)
/// 2. Called from localhost only
///
/// This endpoint:
/// 1. Sets the root user password
/// 2. Creates a new DBA user with the provided credentials
/// 3. Returns user info (user must login separately to get tokens)
pub async fn server_setup_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthSettings>,
    body: web::Json<ServerSetupRequest>,
) -> HttpResponse {
    use kalamdb_commons::constants::AuthConstants;

    // Only allow setup from localhost
    let connection_info = extract_client_ip_secure(&req);
    if !connection_info.is_localhost() && !config.allow_remote_setup {
        return HttpResponse::Forbidden().json(AuthErrorResponse::new(
            "forbidden",
            "Server setup can only be performed from localhost",
        ));
    }

    // Check if root user exists and has empty password
    let root_username = UserName::new(AuthConstants::DEFAULT_SYSTEM_USERNAME);
    let root_user = match user_repo.get_user_by_username(&root_username).await {
        Ok(user) => user,
        Err(e) => {
            log::error!("Failed to get root user: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to verify setup status"));
        },
    };

    // Only allow setup if root has no password
    if !root_user.password_hash.is_empty() {
        return HttpResponse::Conflict().json(AuthErrorResponse::new(
            "already_configured",
            "Server has already been configured. Root password is set.",
        ));
    }

    // Validate passwords
    if let Err(e) = validate_password(&body.password) {
        return HttpResponse::BadRequest().json(AuthErrorResponse::new(
            "weak_password",
            format!("DBA user password: {}", e),
        ));
    }
    if let Err(e) = validate_password(&body.root_password) {
        return HttpResponse::BadRequest().json(AuthErrorResponse::new(
            "weak_password",
            format!("Root password: {}", e),
        ));
    }

    // Check username is not root
    if body.username.to_lowercase() == "root" {
        return HttpResponse::BadRequest().json(AuthErrorResponse::new(
            "invalid_username",
            "Cannot create a DBA user with username 'root'. Choose a different username.",
        ));
    }

    // Check if DBA username already exists
    let dba_username = UserName::new(&body.username);
    if user_repo.get_user_by_username(&dba_username).await.is_ok() {
        return HttpResponse::Conflict().json(AuthErrorResponse::new(
            "user_exists",
            format!("User '{}' already exists", body.username),
        ));
    }

    // Hash passwords
    let root_password_hash = match hash_password(&body.root_password, None).await {
        Ok(hash) => hash,
        Err(e) => {
            log::error!("Failed to hash root password: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to hash password"));
        },
    };

    let dba_password_hash = match hash_password(&body.password, None).await {
        Ok(hash) => hash,
        Err(e) => {
            log::error!("Failed to hash DBA password: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to hash password"));
        },
    };

    // Update root user with password
    let mut updated_root = root_user.clone();
    updated_root.password_hash = root_password_hash;
    updated_root.updated_at = chrono::Utc::now().timestamp_millis();
    // No longer need auth_data with allow_remote - password is enough
    updated_root.auth_data = None;

    if let Err(e) = user_repo.update_user(&updated_root).await {
        log::error!("Failed to update root password: {}", e);
        return HttpResponse::InternalServerError()
            .json(AuthErrorResponse::new("internal_error", "Failed to configure root user"));
    }

    // Create new DBA user
    let created_at = chrono::Utc::now().timestamp_millis();
    let dba_user = User {
        user_id: UserId::new(format!("u_{}", uuid::Uuid::new_v4().simple())),
        username: dba_username.clone(),
        password_hash: dba_password_hash,
        role: Role::Dba,
        email: body.email.clone(),
        auth_type: AuthType::Password,
        auth_data: None,
        storage_mode: StorageMode::Table,
        storage_id: Some(StorageId::local()),
        failed_login_attempts: 0,
        locked_until: None,
        last_login_at: None,
        created_at,
        updated_at: created_at,
        last_seen: None,
        deleted_at: None,
    };

    if let Err(e) = user_repo.create_user(dba_user.clone()).await {
        log::error!("Failed to create DBA user: {}", e);
        return HttpResponse::InternalServerError()
            .json(AuthErrorResponse::new("internal_error", "Failed to create DBA user"));
    }

    let created_at_str = chrono::DateTime::from_timestamp_millis(created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    log::info!(
        "Server setup completed: root password set, DBA user '{}' created",
        body.username
    );

    // Return user info only - user must login separately to get tokens
    HttpResponse::Ok().json(ServerSetupResponse {
        user: UserInfo {
            id: dba_user.user_id,
            username: dba_user.username,
            role: dba_user.role,
            email: dba_user.email,
            created_at: created_at_str.clone(),
            updated_at: created_at_str,
        },
        message: format!(
            "Server setup complete. Root password configured and DBA user '{}' created. Please login to continue.",
            body.username
        ),
    })
}

/// GET /v1/api/auth/status
///
/// Returns the current setup status of the server.
/// Returns whether setup is required or not.
pub async fn setup_status_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthSettings>,
) -> HttpResponse {
    use kalamdb_commons::constants::AuthConstants;

    // Only allow status check from localhost
    let connection_info = extract_client_ip_secure(&req);
    if !connection_info.is_localhost() && !config.allow_remote_setup {
        return HttpResponse::Forbidden().json(AuthErrorResponse::new(
            "forbidden",
            "Setup status can only be checked from localhost",
        ));
    }

    let root_username = UserName::new(AuthConstants::DEFAULT_SYSTEM_USERNAME);
    let root_user = match user_repo.get_user_by_username(&root_username).await {
        Ok(user) => user,
        Err(e) => {
            log::error!("Failed to get root user: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Failed to check setup status"));
        },
    };

    let needs_setup = root_user.password_hash.is_empty();

    HttpResponse::Ok().json(serde_json::json!({
        "needs_setup": needs_setup,
        "message": if needs_setup {
            "Server requires initial setup. Call POST /v1/api/auth/setup with username, password, root_password, and optional email."
        } else {
            "Server is configured and ready."
        }
    }))
}
