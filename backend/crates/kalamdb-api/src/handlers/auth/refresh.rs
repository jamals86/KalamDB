//! Token refresh handler
//!
//! POST /v1/api/auth/refresh - Refreshes the JWT token if still valid

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use kalamdb_auth::{
    create_and_sign_token, create_auth_cookie, extract_client_ip_secure,
    CookieConfig, UserRepository,
};
use kalamdb_auth::providers::jwt_auth::{create_and_sign_refresh_token, validate_jwt_token};
use kalamdb_configs::AuthSettings;
use std::sync::Arc;

use super::models::{AuthErrorResponse, LoginResponse, UserInfo};
use super::{extract_bearer_or_cookie_token, map_auth_error_to_response};
use crate::limiter::RateLimiter;

/// POST /v1/api/auth/refresh
///
/// Refreshes the JWT token if the current one is still valid.
pub async fn refresh_handler(
    req: HttpRequest,
    user_repo: web::Data<Arc<dyn UserRepository>>,
    config: web::Data<AuthSettings>,
    rate_limiter: web::Data<Arc<RateLimiter>>,
) -> HttpResponse {
    // Extract client IP with anti-spoofing checks for localhost validation
    let connection_info = extract_client_ip_secure(&req);

    // Rate limit auth attempts by client IP
    if !rate_limiter.get_ref().check_auth_rate(&connection_info) {
        return HttpResponse::TooManyRequests().json(AuthErrorResponse::new(
            "rate_limited",
            "Too many authentication attempts. Please retry shortly.",
        ));
    }

    let token = match extract_bearer_or_cookie_token(&req) {
        Ok(t) => t,
        Err(err) => return map_auth_error_to_response(err),
    };

    // Validate existing token directly (accepts both access and refresh tokens).
    // Unlike authenticate_bearer (which rejects refresh tokens for API auth),
    // the refresh endpoint must accept refresh tokens to issue new token pairs.
    let jwt_config = kalamdb_auth::providers::jwt_config::get_jwt_config();
    let claims = match validate_jwt_token(&token, &jwt_config.secret, &jwt_config.trusted_issuers) {
        Ok(c) => c,
        Err(err) => return map_auth_error_to_response(err),
    };

    let username_claim = match claims.username {
        Some(ref u) => u.clone(),
        None => {
            return HttpResponse::Unauthorized()
                .json(AuthErrorResponse::new("unauthorized", "Token missing username claim"));
        },
    };

    // Verify user still exists and is active by username
    let user = match user_repo.get_user_by_username(&username_claim).await {
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
    // SECURITY: Uses create_and_sign_refresh_token to set token_type="refresh",
    // preventing refresh tokens from being used as access tokens.
    let refresh_expiry_hours = config.jwt_expiry_hours * 7;
    let (new_refresh_token, _refresh_claims) = match create_and_sign_refresh_token(
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
