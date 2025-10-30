//! Authentication middleware for KalamDB API
//!
//! This module provides Actix-Web middleware that:
//! 1. Extracts the Authorization header from HTTP requests
//! 2. Authenticates users via AuthService (HTTP Basic Auth or JWT Bearer tokens)
//! 3. Attaches the authenticated user context to request extensions
//! 4. Returns 401 Unauthorized on authentication failures
//!
//! ## Usage
//!
//! ```rust,ignore
//! use kalamdb_api::middleware::AuthMiddleware;
//! use actix_web::App;
//!
//! App::new()
//!     .wrap(AuthMiddleware::new(auth_service, rocks_adapter))
//!     .service(my_protected_endpoint)
//! ```
//!
//! Protected endpoints can access the authenticated user via request extensions:
//!
//! ```rust,ignore
//! use kalamdb_auth::context::AuthenticatedUser;
//! use actix_web::{web, HttpRequest, HttpResponse};
//!
//! async fn protected_endpoint(req: HttpRequest) -> HttpResponse {
//!     let user = req.extensions().get::<AuthenticatedUser>().unwrap();
//!     HttpResponse::Ok().json(format!("Hello, {}", user.username))
//! }
//! ```

use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use kalamdb_auth::{
    connection::ConnectionInfo, context::AuthenticatedUser, error::AuthError, service::AuthService,
};
use kalamdb_sql::RocksDbAdapter;
use log::{debug, warn};
use serde_json::json;
use std::{
    future::{ready, Ready},
    rc::Rc,
    sync::Arc,
};

/// Authentication middleware factory
///
/// Creates middleware instances that authenticate incoming HTTP requests.
pub struct AuthMiddleware {
    auth_service: Arc<AuthService>,
    rocks_adapter: Arc<RocksDbAdapter>,
}

impl AuthMiddleware {
    /// Create a new authentication middleware
    ///
    /// # Arguments
    /// * `auth_service` - The authentication service for validating credentials
    /// * `rocks_adapter` - The RocksDB adapter for user database access
    pub fn new(auth_service: Arc<AuthService>, rocks_adapter: Arc<RocksDbAdapter>) -> Self {
        Self {
            auth_service,
            rocks_adapter,
        }
    }
}

/// Generate a unique request ID for this request
fn generate_request_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("req_{}", timestamp)
}

impl<S> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
            auth_service: self.auth_service.clone(),
            rocks_adapter: self.rocks_adapter.clone(),
        }))
    }
}

/// Authentication middleware service instance
pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
    auth_service: Arc<AuthService>,
    rocks_adapter: Arc<RocksDbAdapter>,
}

impl<S> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let auth_service = self.auth_service.clone();
        let rocks_adapter = self.rocks_adapter.clone();

        Box::pin(async move {
            // Extract connection information
            let remote_addr = {
                let conn_info = req.connection_info();
                conn_info.realip_remote_addr().map(|s| s.to_string())
            };

            let connection = ConnectionInfo {
                remote_addr: remote_addr.clone(),
            };

            // Check if localhost (bypass authentication for local development)
            if connection.is_localhost() {
                debug!("Localhost request - bypassing authentication");

                // Create a default "localhost" user for local requests
                let localhost_user = AuthenticatedUser::new(
                    kalamdb_commons::UserId::new("localhost"),
                    "localhost".to_string(),
                    kalamdb_commons::Role::Dba, // Grant DBA role to localhost
                    None,                       // No email for localhost user
                    connection.clone(),
                );

                req.extensions_mut().insert(localhost_user);
                return service.call(req).await;
            }

            // Extract Authorization header
            let auth_header = match req.headers().get("Authorization") {
                Some(header) => match header.to_str() {
                    Ok(s) => s.to_string(),
                    Err(_) => {
                        let request_id = generate_request_id();
                        warn!("Invalid Authorization header format from {:?}, request_id={}", remote_addr, request_id);
                        let (req, _) = req.into_parts();
                        let response = HttpResponse::Unauthorized().json(json!({
                            "error": "INVALID_AUTHORIZATION_HEADER",
                            "message": "Authorization header contains invalid characters",
                            "request_id": request_id
                        }));
                        return Ok(ServiceResponse::new(req, response));
                    }
                },
                None => {
                    let request_id = generate_request_id();
                    warn!("Missing Authorization header from {:?}, request_id={}", remote_addr, request_id);
                    let (req, _) = req.into_parts();
                    let response = HttpResponse::Unauthorized().json(json!({
                        "error": "MISSING_AUTHORIZATION",
                        "message": "Authorization header is required. Use 'Authorization: Basic <credentials>' or 'Authorization: Bearer <token>'",
                        "request_id": request_id
                    }));
                    return Ok(ServiceResponse::new(req, response));
                }
            };

            // Authenticate the request
            match auth_service
                .authenticate(&auth_header, &connection, &rocks_adapter)
                .await
            {
                Ok(authenticated_user) => {
                    debug!(
                        "Authenticated user: {} (role: {:?}) from {:?}",
                        authenticated_user.username, authenticated_user.role, remote_addr
                    );

                    // Store authenticated user in request extensions
                    req.extensions_mut().insert(authenticated_user);

                    // Continue with the request
                    service.call(req).await
                }
                Err(auth_error) => {
                    let request_id = generate_request_id();
                    warn!(
                        "Authentication failed from {:?}: {}, request_id={}",
                        remote_addr, auth_error, request_id
                    );

                    let (status_code, error_code, message) = match auth_error {
                        AuthError::MissingAuthorization => (
                            401,
                            "MISSING_AUTHORIZATION",
                            "Authorization header is required",
                        ),
                        AuthError::InvalidCredentials => {
                            (401, "INVALID_CREDENTIALS", "Invalid username or password")
                        }
                        AuthError::UserNotFound => (401, "USER_NOT_FOUND", "User does not exist"),
                        AuthError::TokenExpired => (401, "TOKEN_EXPIRED", "JWT token has expired"),
                        AuthError::InvalidSignature => {
                            (401, "INVALID_SIGNATURE", "JWT token signature is invalid")
                        }
                        AuthError::MalformedAuthorization(_) => (
                            400,
                            "MALFORMED_AUTHORIZATION",
                            "Authorization header format is invalid",
                        ),
                        AuthError::UntrustedIssuer(_) => {
                            (401, "UNTRUSTED_ISSUER", "JWT issuer is not trusted")
                        }
                        AuthError::MissingClaim(_) => {
                            (400, "MISSING_CLAIM", "Required JWT claim is missing")
                        }
                        AuthError::DatabaseError(_) => {
                            (500, "DATABASE_ERROR", "Authentication service error")
                        }
                        _ => (
                            500,
                            "AUTHENTICATION_ERROR",
                            "An unexpected authentication error occurred",
                        ),
                    };

                    let (req, _) = req.into_parts();
                    let response = HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(status_code).unwrap(),
                    )
                    .json(json!({
                        "error": error_code,
                        "message": message,
                        "request_id": request_id
                    }));
                    Ok(ServiceResponse::new(req, response))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn test_localhost_bypass() {
        // This test verifies that localhost requests bypass authentication
        // In a real integration test, we would set up the full middleware stack
        let connection = ConnectionInfo {
            remote_addr: Some("127.0.0.1".to_string()),
        };

        assert!(connection.is_localhost());
    }

    #[actix_web::test]
    async fn test_missing_authorization_header() {
        // This would require a full integration test setup
        // For now, we verify the error response structure
        let error = AuthError::MissingAuthorization;
        assert_eq!(format!("{}", error), "Missing authorization header");
    }
}
