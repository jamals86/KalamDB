//! Token refresh handler
//!
//! POST /v1/api/auth/refresh - Refreshes the JWT token if still valid

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use kalamdb_auth::{
    authenticate, create_and_sign_token, create_auth_cookie, extract_client_ip_secure,
    helpers::cookie::extract_auth_token,
    AuthRequest, CookieConfig, UserRepository,
};
use kalamdb_configs::AuthSettings;
use std::sync::Arc;

use super::map_auth_error_to_response;
use super::models::{AuthErrorResponse, LoginResponse, UserInfo};
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
