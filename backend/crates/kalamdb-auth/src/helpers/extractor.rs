//! Actix-web extractor for automatic authentication.
//!
//! This module provides `FromRequest` implementations for authentication,
//! allowing handlers to receive authenticated users as function parameters.
//!
//! # Setup
//!
//! The `Arc<dyn UserRepository>` must be registered as app data:
//!
//! ```rust,ignore
//! App::new()
//!     .app_data(web::Data::new(user_repo.clone()))
//!     .service(my_handler)
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use kalamdb_auth::{AuthenticatedUser, AuthSession, OptionalAuth};
//!
//! // Simple case - just the user
//! #[post("/api")]
//! async fn simple_handler(user: AuthenticatedUser) -> impl Responder {
//!     // user is guaranteed to be authenticated
//! }
//!
//! // Full auth info including method and connection info (for audit logging)
//! #[post("/sql")]
//! async fn sql_handler(session: AuthSession) -> impl Responder {
//!     // Access user via session.user
//!     // Access auth method via session.method (Basic, Bearer, Direct)
//!     // Access connection info via session.connection_info
//! }
//!
//! // Optional authentication - allows anonymous access
//! #[get("/public")]
//! async fn public_endpoint(auth: OptionalAuth) -> impl Responder {
//!     if let Some(user) = auth.user() {
//!         // user is authenticated
//!     } else {
//!         // anonymous access
//!     }
//! }
//! ```

use actix_web::{dev::Payload, http::StatusCode, FromRequest, HttpRequest, ResponseError};
use kalamdb_commons::models::ConnectionInfo;
use std::fmt;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;

use crate::errors::error::AuthError;
use crate::helpers::ip_extractor::extract_client_ip_secure;
use crate::models::context::AuthenticatedUser;
use crate::repository::user_repo::UserRepository;
use crate::services::unified::{authenticate, AuthMethod, AuthRequest};

/// Error type for authentication extraction.
///
/// Wraps `AuthError` and implements `ResponseError` for automatic
/// HTTP error responses.
#[derive(Debug)]
pub struct AuthExtractError {
    inner: AuthError,
    /// Time taken to process the request (for response payload)
    took_ms: f64,
}

impl AuthExtractError {
    /// Create a new extraction error from an `AuthError`.
    pub fn new(error: AuthError, took_ms: f64) -> Self {
        Self {
            inner: error,
            took_ms,
        }
    }

    /// Get the underlying auth error.
    pub fn inner(&self) -> &AuthError {
        &self.inner
    }

    /// Get the error code for API responses.
    pub fn error_code(&self) -> &'static str {
        match &self.inner {
            AuthError::MissingAuthorization(_) => "MISSING_AUTHORIZATION",
            AuthError::MalformedAuthorization(_) => "MALFORMED_AUTHORIZATION",
            AuthError::InvalidCredentials(_) => "INVALID_CREDENTIALS",
            AuthError::RemoteAccessDenied(_) => "REMOTE_ACCESS_DENIED",
            AuthError::UserNotFound(_) => "USER_NOT_FOUND",
            AuthError::DatabaseError(_) => "DATABASE_ERROR",
            AuthError::TokenExpired => "TOKEN_EXPIRED",
            AuthError::InvalidSignature => "INVALID_SIGNATURE",
            AuthError::UntrustedIssuer(_) => "UNTRUSTED_ISSUER",
            AuthError::MissingClaim(_) => "MISSING_CLAIM",
            AuthError::InsufficientPermissions(_) => "INSUFFICIENT_PERMISSIONS",
            _ => "AUTHENTICATION_ERROR",
        }
    }
}

impl fmt::Display for AuthExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl ResponseError for AuthExtractError {
    fn status_code(&self) -> StatusCode {
        match &self.inner {
            AuthError::MissingAuthorization(_) => StatusCode::UNAUTHORIZED,
            AuthError::MalformedAuthorization(_) => StatusCode::BAD_REQUEST,
            AuthError::InvalidCredentials(_) => StatusCode::UNAUTHORIZED,
            AuthError::RemoteAccessDenied(_) => StatusCode::FORBIDDEN,
            AuthError::UserNotFound(_) => StatusCode::UNAUTHORIZED,
            AuthError::TokenExpired => StatusCode::UNAUTHORIZED,
            AuthError::InvalidSignature => StatusCode::UNAUTHORIZED,
            AuthError::UntrustedIssuer(_) => StatusCode::UNAUTHORIZED,
            AuthError::MissingClaim(_) => StatusCode::UNAUTHORIZED,
            AuthError::InsufficientPermissions(_) => StatusCode::FORBIDDEN,
            AuthError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        let body = serde_json::json!({
            "status": "error",
            "error": {
                "code": self.error_code(),
                "message": self.inner.to_string()
            },
            "results": [],
            "took": self.took_ms
        });

        actix_web::HttpResponse::build(self.status_code())
            .content_type("application/json")
            .json(body)
    }
}

impl From<AuthError> for AuthExtractError {
    fn from(error: AuthError) -> Self {
        Self {
            inner: error,
            took_ms: 0.0,
        }
    }
}

/// Full authentication session with user, method, and connection info.
///
/// Use this when you need access to the authentication method (Basic, Bearer, Direct)
/// for audit logging or when you need the connection info.
///
/// # Example
///
/// ```rust,ignore
/// async fn handler(session: AuthSession) -> impl Responder {
///     // Access the authenticated user
///     let user = &session.user;
///     
///     // Check auth method for audit logging
///     if session.is_password_auth() {
///         // Log password-based authentication
///     }
///     
///     // Access connection info
///     let ip = &session.connection_info.remote_addr;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthSession {
    /// The authenticated user
    pub user: AuthenticatedUser,
    /// The authentication method used
    pub method: AuthMethod,
    /// Connection information (IP address, localhost check)
    pub connection_info: ConnectionInfo,
}

impl AuthSession {
    /// Check if authentication was password-based (Basic Auth or Direct credentials).
    /// Useful for determining when to log authentication events.
    pub fn is_password_auth(&self) -> bool {
        matches!(self.method, AuthMethod::Basic | AuthMethod::Direct)
    }
}

impl Deref for AuthSession {
    type Target = AuthenticatedUser;

    fn deref(&self) -> &Self::Target {
        &self.user
    }
}

impl FromRequest for AuthSession {
    type Error = AuthExtractError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            // Get user repository from app data - MUST be registered
            let repo: Arc<dyn UserRepository> = if let Some(repo) =
                req.app_data::<actix_web::web::Data<Arc<dyn UserRepository>>>()
            {
                repo.get_ref().clone()
            } else {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return Err(AuthExtractError::new(
                        AuthError::DatabaseError(
                            "User repository not configured. Ensure Arc<dyn UserRepository> is registered as app data.".to_string(),
                        ),
                        took,
                    ));
            };

            // Extract Authorization header
            let auth_header = match req.headers().get("Authorization") {
                Some(val) => match val.to_str() {
                    Ok(h) => h.to_string(),
                    Err(_) => {
                        let took = start_time.elapsed().as_secs_f64() * 1000.0;
                        return Err(AuthExtractError::new(
                            AuthError::MalformedAuthorization(
                                "Authorization header contains invalid characters".to_string(),
                            ),
                            took,
                        ));
                    },
                },
                None => {
                    let took = start_time.elapsed().as_secs_f64() * 1000.0;
                    return Err(AuthExtractError::new(
                        AuthError::MissingAuthorization(
                            "Authorization header is required. Use 'Authorization: Basic <credentials>' or 'Authorization: Bearer <token>'".to_string(),
                        ),
                        took,
                    ));
                },
            };

            // Extract client IP with security checks
            let connection_info = extract_client_ip_secure(&req);

            // Build auth request and authenticate
            let auth_request = AuthRequest::Header(auth_header);

            match authenticate(auth_request, &connection_info, &repo).await {
                Ok(result) => Ok(AuthSession {
                    user: result.user,
                    method: result.method,
                    connection_info,
                }),
                Err(e) => {
                    let took = start_time.elapsed().as_secs_f64() * 1000.0;
                    Err(AuthExtractError::new(e, took))
                },
            }
        })
    }
}

/// Implement FromRequest for AuthenticatedUser.
///
/// This allows handlers to receive authenticated users directly:
/// ```rust,ignore
/// async fn handler(user: AuthenticatedUser) -> impl Responder { ... }
/// ```
///
/// The extractor will:
/// 1. Extract the Authorization header
/// 2. Get the UserRepository from app data (MUST be registered)
/// 3. Call the unified authenticate function
/// 4. Return the authenticated user or an error response
///
/// # Required App Data
///
/// The `Arc<dyn UserRepository>` MUST be registered as app data before using this extractor.
impl FromRequest for AuthenticatedUser {
    type Error = AuthExtractError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        // Clone what we need for the async block
        let req = req.clone();

        Box::pin(async move {
            let start_time = std::time::Instant::now();

            // Get user repository from app data - MUST be registered
            let repo: Arc<dyn UserRepository> = if let Some(repo) =
                req.app_data::<actix_web::web::Data<Arc<dyn UserRepository>>>()
            {
                repo.get_ref().clone()
            } else {
                let took = start_time.elapsed().as_secs_f64() * 1000.0;
                return Err(AuthExtractError::new(
                        AuthError::DatabaseError(
                            "User repository not configured. Ensure Arc<dyn UserRepository> is registered as app data.".to_string(),
                        ),
                        took,
                    ));
            };

            // Extract Authorization header
            let auth_header = match req.headers().get("Authorization") {
                Some(val) => match val.to_str() {
                    Ok(h) => h.to_string(),
                    Err(_) => {
                        let took = start_time.elapsed().as_secs_f64() * 1000.0;
                        return Err(AuthExtractError::new(
                            AuthError::MalformedAuthorization(
                                "Authorization header contains invalid characters".to_string(),
                            ),
                            took,
                        ));
                    },
                },
                None => {
                    let took = start_time.elapsed().as_secs_f64() * 1000.0;
                    return Err(AuthExtractError::new(
                        AuthError::MissingAuthorization(
                            "Authorization header is required. Use 'Authorization: Basic <credentials>' or 'Authorization: Bearer <token>'".to_string(),
                        ),
                        took,
                    ));
                },
            };

            // Extract client IP with security checks
            let connection_info = extract_client_ip_secure(&req);

            // Build auth request and authenticate
            let auth_request = AuthRequest::Header(auth_header);

            match authenticate(auth_request, &connection_info, &repo).await {
                Ok(result) => Ok(result.user),
                Err(e) => {
                    let took = start_time.elapsed().as_secs_f64() * 1000.0;
                    Err(AuthExtractError::new(e, took))
                },
            }
        })
    }
}

/// Optional authentication wrapper.
///
/// Use this when authentication is optional (e.g., public endpoints that
/// behave differently for authenticated users).
///
/// # Example
///
/// ```rust,ignore
/// async fn maybe_auth(auth: OptionalAuth) -> impl Responder {
///     match auth.user() {
///         Some(user) => format!("Hello, {}!", user.username),
///         None => "Hello, anonymous!".to_string(),
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OptionalAuth {
    user: Option<AuthenticatedUser>,
}

impl OptionalAuth {
    /// Create a new OptionalAuth with an authenticated user.
    pub fn authenticated(user: AuthenticatedUser) -> Self {
        Self { user: Some(user) }
    }

    /// Create a new OptionalAuth with no user (anonymous).
    pub fn anonymous() -> Self {
        Self { user: None }
    }

    /// Get the authenticated user, if present.
    pub fn user(&self) -> Option<&AuthenticatedUser> {
        self.user.as_ref()
    }

    /// Take ownership of the authenticated user, if present.
    pub fn into_user(self) -> Option<AuthenticatedUser> {
        self.user
    }

    /// Check if the request is authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }
}

impl FromRequest for OptionalAuth {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();

        Box::pin(async move {
            // Check if Authorization header is present
            if req.headers().get("Authorization").is_none() {
                return Ok(OptionalAuth::anonymous());
            }

            // Try to get user repository
            let repo: Option<Arc<dyn UserRepository>> = if let Some(repo) =
                req.app_data::<actix_web::web::Data<Arc<dyn UserRepository>>>()
            {
                Some(repo.get_ref().clone())
            } else {
                None
            };

            let Some(repo) = repo else {
                // No repository available, treat as anonymous
                return Ok(OptionalAuth::anonymous());
            };

            // Extract Authorization header
            let auth_header = match req.headers().get("Authorization") {
                Some(val) => match val.to_str() {
                    Ok(h) => h.to_string(),
                    Err(_) => return Ok(OptionalAuth::anonymous()),
                },
                None => return Ok(OptionalAuth::anonymous()),
            };

            // Extract client IP
            let connection_info = extract_client_ip_secure(&req);

            // Try to authenticate
            let auth_request = AuthRequest::Header(auth_header);
            match authenticate(auth_request, &connection_info, &repo).await {
                Ok(result) => Ok(OptionalAuth::authenticated(result.user)),
                Err(_) => Ok(OptionalAuth::anonymous()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_extract_error_codes() {
        let err = AuthExtractError::new(AuthError::MissingAuthorization("test".to_string()), 10.0);
        assert_eq!(err.error_code(), "MISSING_AUTHORIZATION");
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let err =
            AuthExtractError::new(AuthError::MalformedAuthorization("test".to_string()), 10.0);
        assert_eq!(err.error_code(), "MALFORMED_AUTHORIZATION");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);

        let err = AuthExtractError::new(AuthError::TokenExpired, 10.0);
        assert_eq!(err.error_code(), "TOKEN_EXPIRED");
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let err =
            AuthExtractError::new(AuthError::InsufficientPermissions("test".to_string()), 10.0);
        assert_eq!(err.error_code(), "INSUFFICIENT_PERMISSIONS");
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_optional_auth() {
        let opt = OptionalAuth::anonymous();
        assert!(!opt.is_authenticated());
        assert!(opt.user().is_none());

        // Can't easily test authenticated case without mocking
    }
}
