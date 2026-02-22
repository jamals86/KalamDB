// Cookie handling for HttpOnly authentication cookies
//
// This module provides utilities for creating and managing HttpOnly cookies
// for JWT token storage in the Admin UI.

use actix_web::cookie::{Cookie, SameSite};
use chrono::{Duration, Utc};

/// Cookie name for the authentication token
pub const AUTH_COOKIE_NAME: &str = "kalamdb_auth";

/// Cookie name for the refresh token
pub const REFRESH_COOKIE_NAME: &str = "kalamdb_refresh";

/// Configuration for authentication cookies
#[derive(Debug, Clone)]
pub struct CookieConfig {
    /// Whether to set the Secure flag (should be true in production/HTTPS)
    pub secure: bool,
    /// Cookie path (default: "/")
    pub path: String,
    /// SameSite policy
    pub same_site: SameSite,
    /// Domain (None = current domain)
    pub domain: Option<String>,
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self {
            // SECURITY: Default to true for HTTPS-only cookie transmission.
            // Set to false only in development environments without TLS.
            secure: true,
            path: "/".to_string(),
            same_site: SameSite::Strict,
            domain: None,
        }
    }
}

impl CookieConfig {
    /// Create a production-ready config with Secure flag enabled
    pub fn production() -> Self {
        Self {
            secure: true,
            ..Default::default()
        }
    }
}

/// Create an HttpOnly authentication cookie with the given JWT token.
///
/// # Arguments
/// * `token` - JWT token string
/// * `expires_in` - Token expiration duration
/// * `config` - Cookie configuration
///
/// # Returns
/// An HttpOnly cookie with the token
pub fn create_auth_cookie<'a>(
    token: &str,
    expires_in: Duration,
    config: &CookieConfig,
) -> Cookie<'a> {
    let expiry = Utc::now() + expires_in;

    let mut cookie = Cookie::build(AUTH_COOKIE_NAME, token.to_string())
        .path(config.path.clone())
        .http_only(true)
        .secure(config.secure)
        .same_site(config.same_site)
        .expires(
            cookie::time::OffsetDateTime::from_unix_timestamp(expiry.timestamp())
                .unwrap_or_else(|_| {
                    log::warn!(
                        "JWT expiry timestamp {} is out of OffsetDateTime range; \
                        falling back to current time plus 24 h",
                        expiry.timestamp()
                    );
                    cookie::time::OffsetDateTime::now_utc()
                        + cookie::time::Duration::hours(24)
                }),
        )
        .finish();

    if let Some(ref domain) = config.domain {
        cookie.set_domain(domain.clone());
    }

    cookie
}

/// Create a cookie that clears/expires the authentication cookie.
///
/// Used during logout to remove the auth cookie from the browser.
///
/// # Arguments
/// * `config` - Cookie configuration
///
/// # Returns
/// An expired cookie that will clear the auth cookie
pub fn create_logout_cookie<'a>(config: &CookieConfig) -> Cookie<'a> {
    let mut cookie = Cookie::build(AUTH_COOKIE_NAME, "")
        .path(config.path.clone())
        .http_only(true)
        .secure(config.secure)
        .same_site(config.same_site)
        .expires(cookie::time::OffsetDateTime::UNIX_EPOCH)
        .finish();

    if let Some(ref domain) = config.domain {
        cookie.set_domain(domain.clone());
    }

    cookie
}

/// Extract the auth token from request cookies.
///
/// # Arguments
/// * `cookies` - Iterator over cookies from the request
///
/// # Returns
/// The auth token if present
pub fn extract_auth_token<'a, I>(cookies: I) -> Option<String>
where
    I: Iterator<Item = Cookie<'a>>,
{
    cookies
        .filter(|c| c.name() == AUTH_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_auth_cookie() {
        let config = CookieConfig::default();
        let token = "test.jwt.token";
        let expires_in = Duration::hours(24);

        let cookie = create_auth_cookie(token, expires_in, &config);

        assert_eq!(cookie.name(), AUTH_COOKIE_NAME);
        assert_eq!(cookie.value(), token);
        assert!(cookie.http_only().unwrap_or(false));
        assert_eq!(cookie.same_site(), Some(SameSite::Strict));
        assert_eq!(cookie.path(), Some("/"));
    }

    #[test]
    fn test_create_logout_cookie() {
        let config = CookieConfig::default();
        let cookie = create_logout_cookie(&config);

        assert_eq!(cookie.name(), AUTH_COOKIE_NAME);
        assert_eq!(cookie.value(), "");
        assert!(cookie.http_only().unwrap_or(false));
    }

    #[test]
    fn test_production_config() {
        let config = CookieConfig::production();
        assert!(config.secure);
    }
}
