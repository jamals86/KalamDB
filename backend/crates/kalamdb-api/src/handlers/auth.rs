// Authentication handlers for Admin UI
//
// Provides login, logout, refresh, and current user endpoints
// that use HttpOnly cookies for JWT token storage.

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use kalamdb_auth::{
    cookie::{extract_auth_token, CookieConfig},
    create_auth_cookie, create_logout_cookie, generate_jwt_token,
    jwt_auth::{validate_jwt_token, JwtClaims, DEFAULT_JWT_EXPIRY_HOURS, KALAMDB_ISSUER},
    password::verify_password,
    UserRepository,
};
use kalamdb_commons::Role;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Login request body
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
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
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// Auth configuration from environment/config
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub cookie_secure: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            // IMPORTANT: This must match the default in kalamdb-auth/src/unified.rs JWT_CONFIG
            jwt_secret: std::env::var("KALAMDB_JWT_SECRET")
                .unwrap_or_else(|_| "kalamdb-dev-secret-key-change-in-production".to_string()),
            jwt_expiry_hours: std::env::var("KALAMDB_JWT_EXPIRY_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_JWT_EXPIRY_HOURS),
            cookie_secure: std::env::var("KALAMDB_COOKIE_SECURE")
                .map(|s| s == "true" || s == "1")
                .unwrap_or(false),
        }
    }
}

/// POST /v1/api/auth/login
///
/// Authenticates a user and returns an HttpOnly cookie with JWT token.
/// Only allows dba and system roles to access the admin UI.
pub async fn login_handler(
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthConfig>,
    body: web::Json<LoginRequest>,
) -> HttpResponse {
    // Find user by username
    let user = match user_repo.get_user_by_username(&body.username).await {
        Ok(user) => user,
        Err(e) => {
            // Return generic error to prevent username enumeration
            log::debug!("Login failed for '{}': {}", body.username, e);
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Invalid username or password".to_string(),
            });
        }
    };

    // Check if user is soft-deleted
    if user.deleted_at.is_some() {
        return HttpResponse::Unauthorized().json(ErrorResponse {
            error: "unauthorized".to_string(),
            message: "Invalid username or password".to_string(),
        });
    }

    // Check if user has a password set (required for Admin UI)
    // Root user starts with empty password for localhost CLI access
    // Admin UI requires a password for security
    if user.password_hash.is_empty() {
        log::info!(
            "Login attempt for '{}' failed: password not set. Set with: ALTER USER {} SET PASSWORD '...'",
            body.username,
            body.username
        );
        return HttpResponse::Unauthorized().json(ErrorResponse {
            error: "password_required".to_string(),
            message: format!(
                "Password not set for '{}'. Set a password using: ALTER USER {} SET PASSWORD 'your-password'",
                body.username, body.username
            ),
        });
    }

    // Verify password
    match verify_password(&body.password, &user.password_hash).await {
        Ok(true) => {}
        Ok(false) => {
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Invalid username or password".to_string(),
            });
        }
        Err(e) => {
            log::error!("Error verifying password: {}", e);
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Authentication failed".to_string(),
            });
        }
    }

    // Check role - only dba and system can access admin UI
    let role = Role::from(user.role.as_str());
    if !matches!(role, Role::Dba | Role::System) {
        return HttpResponse::Forbidden().json(ErrorResponse {
            error: "forbidden".to_string(),
            message: "Admin UI access requires dba or system role".to_string(),
        });
    }

    // Generate JWT token
    let claims = JwtClaims::new(
        &user.id.to_string(),
        user.username.as_str(),
        user.role.as_str(),
        user.email.as_deref(),
        Some(config.jwt_expiry_hours),
    );

    let token = match generate_jwt_token(&claims, &config.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Error generating JWT: {}", e);
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to generate token".to_string(),
            });
        }
    };

    // Create HttpOnly cookie
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie = create_auth_cookie(
        &token,
        Duration::hours(config.jwt_expiry_hours),
        &cookie_config,
    );

    let expires_at = Utc::now() + Duration::hours(config.jwt_expiry_hours);

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok()
        .cookie(cookie)
        .json(LoginResponse {
            user: UserInfo {
                id: user.id.to_string(),
                username: user.username.to_string(),
                role: user.role.to_string(),
                email: user.email,
                created_at,
                updated_at,
            },
            expires_at: expires_at.to_rfc3339(),
            access_token: token,
        })
}

/// POST /v1/api/auth/refresh
///
/// Refreshes the JWT token if the current one is still valid.
pub async fn refresh_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthConfig>,
) -> HttpResponse {
    // Extract token from cookie
    let token = match extract_auth_token(req.cookies().ok().iter().flat_map(|c| c.iter().cloned())) {
        Some(t) => t,
        None => {
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "No auth token found".to_string(),
            });
        }
    };

    // Validate existing token
    let trusted_issuers = vec![KALAMDB_ISSUER.to_string()];
    let claims = match validate_jwt_token(&token, &config.jwt_secret, &trusted_issuers) {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Token validation failed: {}", e);
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Invalid or expired token".to_string(),
            });
        }
    };

    // Verify user still exists and is active by username (we don't have find_by_id)
    let username = claims.username.as_deref().unwrap_or("");
    let user = match user_repo.get_user_by_username(username).await {
        Ok(user) if user.deleted_at.is_none() => user,
        _ => {
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "User no longer valid".to_string(),
            });
        }
    };

    // Generate new token
    let new_claims = JwtClaims::new(
        &user.id.to_string(),
        user.username.as_str(),
        user.role.as_str(),
        user.email.as_deref(),
        Some(config.jwt_expiry_hours),
    );

    let new_token = match generate_jwt_token(&new_claims, &config.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            log::error!("Error generating JWT: {}", e);
            return HttpResponse::InternalServerError().json(ErrorResponse {
                error: "internal_error".to_string(),
                message: "Failed to refresh token".to_string(),
            });
        }
    };

    // Create new cookie
    let cookie_config = CookieConfig {
        secure: config.cookie_secure,
        ..Default::default()
    };
    let cookie = create_auth_cookie(
        &new_token,
        Duration::hours(config.jwt_expiry_hours),
        &cookie_config,
    );

    let expires_at = Utc::now() + Duration::hours(config.jwt_expiry_hours);

    // Convert timestamps properly
    let created_at = chrono::DateTime::from_timestamp_millis(user.created_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    let updated_at = chrono::DateTime::from_timestamp_millis(user.updated_at)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    HttpResponse::Ok()
        .cookie(cookie)
        .json(LoginResponse {
            user: UserInfo {
                id: user.id.to_string(),
                username: user.username.to_string(),
                role: user.role.to_string(),
                email: user.email,
                created_at,
                updated_at,
            },
            expires_at: expires_at.to_rfc3339(),
            access_token: token,
        })
}

/// POST /v1/api/auth/logout
///
/// Clears the authentication cookie.
pub async fn logout_handler(config: web::Data<AuthConfig>) -> HttpResponse {
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
    config: web::Data<AuthConfig>,
) -> HttpResponse {
    // Extract token from cookie
    let token = match extract_auth_token(req.cookies().ok().iter().flat_map(|c| c.iter().cloned())) {
        Some(t) => t,
        None => {
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Not authenticated".to_string(),
            });
        }
    };

    // Validate token
    let trusted_issuers = vec![KALAMDB_ISSUER.to_string()];
    let claims = match validate_jwt_token(&token, &config.jwt_secret, &trusted_issuers) {
        Ok(c) => c,
        Err(e) => {
            log::debug!("Token validation failed: {}", e);
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "Invalid or expired token".to_string(),
            });
        }
    };

    // Get current user info
    let username = claims.username.as_deref().unwrap_or("");
    let user = match user_repo.get_user_by_username(username).await {
        Ok(user) if user.deleted_at.is_none() => user,
        _ => {
            return HttpResponse::Unauthorized().json(ErrorResponse {
                error: "unauthorized".to_string(),
                message: "User not found".to_string(),
            });
        }
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
