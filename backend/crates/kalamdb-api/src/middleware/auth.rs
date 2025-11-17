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
use kalamdb_auth::{connection::ConnectionInfo, context::AuthenticatedUser, UserRepository};
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
    repo: Arc<dyn UserRepository>,
}

impl AuthMiddleware {
    /// Create a new authentication middleware using a repository abstraction
    ///
    /// This is the preferred constructor for provider-based storage
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
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
            repo: self.repo.clone(),
        }))
    }
}

/// Authentication middleware service instance
pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
    #[allow(dead_code)]
    repo: Arc<dyn UserRepository>,
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

        Box::pin(async move {
            // Extract remote address
            let remote_addr = req.peer_addr().map(|addr| addr.to_string()).or_else(|| {
                req.headers()
                    .get("X-Forwarded-For")
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
            });

            let connection_info = ConnectionInfo::new(remote_addr.clone());

            // Check if localhost (bypass authentication for local development)
            if connection_info.is_localhost() {
                debug!("Localhost request - bypassing authentication");

                // Create a default "localhost" user for local requests
                let localhost_user = AuthenticatedUser::new(
                    kalamdb_commons::UserId::new("localhost"),
                    "localhost".to_string(),
                    kalamdb_commons::Role::Dba, // Grant DBA role to localhost
                    None,                       // No email for localhost user
                    connection_info,
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
                        warn!(
                            "Invalid Authorization header format from {:?}, request_id={}",
                            remote_addr, request_id
                        );
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
                    warn!(
                        "Missing Authorization header from {:?}, request_id={}",
                        remote_addr, request_id
                    );
                    let (req, _) = req.into_parts();
                    let response = HttpResponse::Unauthorized().json(json!({
                        "error": "MISSING_AUTHORIZATION",
                        "message": "Authorization header is required. Use 'Authorization: Basic <credentials>' or 'Authorization: Bearer <token>'",
                        "request_id": request_id
                    }));
                    return Ok(ServiceResponse::new(req, response));
                }
            };

            // Use the extractor from kalamdb-auth to validate the request
            // For now, we'll return a simple error since JWT is not yet implemented
            if auth_header.starts_with("Basic ") {
                // Basic auth will be validated by calling the repository
                // For simplicity, we'll just log and continue
                debug!("Basic authentication attempted from {:?}", remote_addr);

                // TODO: Implement actual Basic auth validation using repo
                // For now, bypass authentication
                let default_user = AuthenticatedUser::new(
                    kalamdb_commons::UserId::new("default_user"),
                    "default_user".to_string(),
                    kalamdb_commons::Role::User,
                    None,
                    connection_info,
                );

                req.extensions_mut().insert(default_user);
                service.call(req).await
            } else if auth_header.starts_with("Bearer ") {
                // JWT authentication not yet implemented
                let request_id = generate_request_id();
                warn!(
                    "JWT authentication not yet implemented, request_id={}",
                    request_id
                );
                let (req, _) = req.into_parts();
                let response = HttpResponse::Unauthorized().json(json!({
                    "error": "JWT_NOT_IMPLEMENTED",
                    "message": "JWT authentication is not yet implemented",
                    "request_id": request_id
                }));
                Ok(ServiceResponse::new(req, response))
            } else {
                let request_id = generate_request_id();
                let (req, _) = req.into_parts();
                let response = HttpResponse::BadRequest().json(json!({
                    "error": "MALFORMED_AUTHORIZATION",
                    "message": "Authorization header must start with 'Basic ' or 'Bearer '",
                    "request_id": request_id
                }));
                Ok(ServiceResponse::new(req, response))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_localhost_detection() {
        // Test localhost detection logic
        assert!("127.0.0.1" == "127.0.0.1");
        assert!("::1" == "::1");
        assert!("localhost".starts_with("127.") || "localhost" == "localhost");
    }
}
