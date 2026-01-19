// Authentication handlers for Admin UI
//
// Provides login, logout, refresh, and current user endpoints
// that use HttpOnly cookies for JWT token storage.

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use kalamdb_auth::{
    authenticate, create_and_sign_token, create_auth_cookie, create_logout_cookie,
    extract_client_ip_secure, AuthError, AuthRequest, CookieConfig, UserRepository,
};
use kalamdb_auth::helpers::cookie::extract_auth_token;
use kalamdb_commons::Role;
use kalamdb_configs::AuthSettings;
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
}

/// User info returned in responses
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub role: String,
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
    let username_typed =
        kalamdb_commons::models::UserName::from(auth_result.user.username.as_str());
    let user = match user_repo.get_user_by_username(&username_typed).await {
        Ok(user) => user,
        Err(e) => {
            log::error!("Failed to load user after authentication: {}", e);
            return HttpResponse::InternalServerError()
                .json(AuthErrorResponse::new("internal_error", "Authentication failed"));
        },
    };

    // Generate JWT token
    let (token, _claims) = match create_and_sign_token(
        &user.id,
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

    // Create HttpOnly cookie
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie =
        create_auth_cookie(&token, Duration::hours(config.jwt_expiry_hours), &cookie_config);

    let expires_at = Utc::now() + Duration::hours(config.jwt_expiry_hours);

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok().cookie(cookie).json(LoginResponse {
        user: UserInfo {
            id: user.id.to_string(),
            username: user.username.as_str().to_string(),
            role: user.role.to_string(),
            email: user.email,
            created_at,
            updated_at,
        },
        expires_at: expires_at.to_rfc3339(),
        access_token: token,
    })
}

fn map_auth_error_to_response(err: AuthError) -> HttpResponse {
    match err {
        AuthError::InvalidCredentials(_)
        | AuthError::UserNotFound(_)
        | AuthError::UserDeleted
        | AuthError::AuthenticationFailed(_) => HttpResponse::Unauthorized()
            .json(AuthErrorResponse::new("unauthorized", "Invalid username or password")),
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
    let username_typed =
        kalamdb_commons::models::UserName::from(auth_result.user.username.as_str());
    let user = match user_repo.get_user_by_username(&username_typed).await {
        Ok(user) if user.deleted_at.is_none() => user,
        _ => {
            return HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "User no longer valid"));
        },
    };

    // Generate new token
    let (new_token, _new_claims) = match create_and_sign_token(
        &user.id,
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

    // Create new cookie
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie =
        create_auth_cookie(&new_token, Duration::hours(config.jwt_expiry_hours), &cookie_config);

    let expires_at = Utc::now() + Duration::hours(config.jwt_expiry_hours);

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok().cookie(cookie).json(LoginResponse {
        user: UserInfo {
            id: user.id.to_string(),
            username: user.username.to_string(),
            role: user.role.to_string(),
            email: user.email,
            created_at,
            updated_at,
        },
        expires_at: expires_at.to_rfc3339(),
        access_token: new_token,
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
    let username_typed =
        kalamdb_commons::models::UserName::from(auth_result.user.username.as_str());
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
        id: user.id.to_string(),
        username: user.username.to_string(),
        role: user.role.to_string(),
        email: user.email,
        created_at,
        updated_at,
    })
}
