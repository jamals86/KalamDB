//! Logout handler
//!
//! POST /v1/api/auth/logout - Clears authentication cookie

use actix_web::{web, HttpResponse};
use kalamdb_auth::{create_logout_cookie, CookieConfig};
use kalamdb_configs::AuthSettings;

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
