//! Authentication handlers for Admin UI and API
//!
//! Provides login, logout, refresh, current user, and server-setup endpoints
//! that use HttpOnly cookies for JWT token storage.
//!
//! ## Endpoints
//! - POST /v1/api/auth/login - Authenticate user and get JWT tokens
//! - POST /v1/api/auth/refresh - Refresh JWT tokens
//! - POST /v1/api/auth/logout - Clear authentication cookie
//! - GET /v1/api/auth/me - Get current user info
//! - POST /v1/api/auth/setup - Initial server setup (localhost only)
//! - GET /v1/api/auth/status - Check server setup status

pub mod models;

mod login;
mod logout;
mod me;
mod refresh;
mod setup;

pub use login::login_handler;
pub use logout::logout_handler;
pub use me::me_handler;
pub use refresh::refresh_handler;
pub use setup::{server_setup_handler, setup_status_handler};

use actix_web::HttpResponse;
use kalamdb_auth::AuthError;
use models::AuthErrorResponse;

/// Map authentication errors to HTTP responses
///
/// Uses generic error messages to prevent user enumeration attacks.
/// Sensitive errors (user not found, wrong password) return the same
/// "Invalid username or password" message.
pub(crate) fn map_auth_error_to_response(err: AuthError) -> HttpResponse {
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
