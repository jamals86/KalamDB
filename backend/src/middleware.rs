//! Server-wide middleware configuration helpers.
//!
//! Keeps the Actix application setup focused by providing
//! reusable constructors for CORS, logging, and connection protection layers.
//!
//! ## Protection Middleware Stack (applied in order)
//!
//! 1. **ConnectionProtection**: First line of defense - drops abusive IPs early
//! 2. **CORS**: Cross-origin resource sharing policy
//! 3. **Logger**: Request/response logging
//!
//! ## DoS Protection Features
//!
//! - Per-IP connection limits
//! - Per-IP request rate limits (pre-authentication)
//! - Request body size limits
//! - Automatic IP banning for persistent abusers

use actix_cors::Cors;
use actix_web::body::{BoxBody, EitherBody};
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::http::StatusCode;
use actix_web::middleware;
use actix_web::{Error, HttpResponse};
use futures_util::future::LocalBoxFuture;
use kalamdb_api::rate_limiter::{ConnectionGuard, ConnectionGuardConfig, ConnectionGuardResult};
use log::warn;
use std::future::{ready, Ready};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

/// Build the CORS policy used by the server.
pub fn build_cors() -> Cors {
    Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .supports_credentials()
        .max_age(3600)
}

/// Build the request logger middleware.
pub fn request_logger() -> middleware::Logger {
    middleware::Logger::default()
}

// ============================================================================
// Connection Protection Middleware
// ============================================================================

/// Connection protection middleware factory.
///
/// This middleware provides the first line of defense against DoS attacks
/// by checking requests BEFORE any expensive processing happens.
///
/// Protection includes:
/// - Per-IP connection limits
/// - Per-IP request rate limits
/// - Request body size limits
/// - Automatic IP banning for abusive clients
#[derive(Clone)]
pub struct ConnectionProtection {
    guard: Arc<ConnectionGuard>,
}

impl ConnectionProtection {
    /// Create connection protection with default settings
    pub fn new() -> Self {
        Self {
            guard: Arc::new(ConnectionGuard::new()),
        }
    }

    /// Create connection protection with custom configuration
    pub fn with_config(config: ConnectionGuardConfig) -> Self {
        Self {
            guard: Arc::new(ConnectionGuard::with_config(config)),
        }
    }

    /// Create connection protection from server config
    pub fn from_server_config(config: &kalamdb_commons::config::ServerConfig) -> Self {
        let guard_config = ConnectionGuardConfig {
            max_connections_per_ip: config.rate_limit.max_connections_per_ip,
            max_requests_per_ip_per_sec: config.rate_limit.max_requests_per_ip_per_sec,
            request_body_limit_bytes: config.rate_limit.request_body_limit_bytes,
            ban_duration: Duration::from_secs(config.rate_limit.ban_duration_seconds),
            enabled: config.rate_limit.enable_connection_protection,
        };
        Self::with_config(guard_config)
    }

    /// Get the underlying connection guard (for monitoring/management)
    pub fn guard(&self) -> &Arc<ConnectionGuard> {
        &self.guard
    }
}

impl Default for ConnectionProtection {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, B> Transform<S, ServiceRequest> for ConnectionProtection
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B, BoxBody>>;
    type Error = Error;
    type InitError = ();
    type Transform = ConnectionProtectionMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ConnectionProtectionMiddleware {
            service,
            guard: self.guard.clone(),
        }))
    }
}

/// The actual middleware service that checks each request.
pub struct ConnectionProtectionMiddleware<S> {
    service: S,
    guard: Arc<ConnectionGuard>,
}

impl<S, B> Service<ServiceRequest> for ConnectionProtectionMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B, BoxBody>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Extract client IP
        let client_ip = extract_client_ip(&req);

        // Get content-length for body size check
        let content_length = req
            .headers()
            .get("content-length")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        // Check the request against the connection guard
        let result = self.guard.check_request(client_ip, content_length);

        match result {
            ConnectionGuardResult::Allowed => {
                // Request is allowed, proceed with the service
                let fut = self.service.call(req);
                Box::pin(async move {
                    let res = fut.await?;
                    Ok(res.map_into_left_body())
                })
            }

            ConnectionGuardResult::Banned { until: _ } => {
                warn!(
                    "[CONN_PROTECT] Rejected banned IP: {} path={}",
                    client_ip,
                    req.path()
                );

                let response = HttpResponse::build(StatusCode::TOO_MANY_REQUESTS)
                    .insert_header(("Retry-After", "300"))
                    .insert_header(("X-RateLimit-Reset", "300"))
                    .json(serde_json::json!({
                        "error": "IP_BANNED",
                        "message": "Your IP has been temporarily banned due to excessive requests",
                        "retry_after_seconds": 300
                    }));

                Box::pin(async move {
                    Ok(req.into_response(response).map_into_right_body())
                })
            }

            ConnectionGuardResult::TooManyConnections { current, max } => {
                warn!(
                    "[CONN_PROTECT] Connection limit exceeded: IP={} current={} max={} path={}",
                    client_ip,
                    current,
                    max,
                    req.path()
                );

                let response = HttpResponse::build(StatusCode::SERVICE_UNAVAILABLE)
                    .insert_header(("Retry-After", "5"))
                    .json(serde_json::json!({
                        "error": "TOO_MANY_CONNECTIONS",
                        "message": "Too many connections from your IP address",
                        "current": current,
                        "max": max
                    }));

                Box::pin(async move {
                    Ok(req.into_response(response).map_into_right_body())
                })
            }

            ConnectionGuardResult::RateLimitExceeded => {
                warn!(
                    "[CONN_PROTECT] Rate limit exceeded: IP={} path={}",
                    client_ip,
                    req.path()
                );

                let response = HttpResponse::build(StatusCode::TOO_MANY_REQUESTS)
                    .insert_header(("Retry-After", "1"))
                    .insert_header(("X-RateLimit-Reset", "1"))
                    .json(serde_json::json!({
                        "error": "RATE_LIMIT_EXCEEDED",
                        "message": "Too many requests from your IP address",
                        "retry_after_seconds": 1
                    }));

                Box::pin(async move {
                    Ok(req.into_response(response).map_into_right_body())
                })
            }

            ConnectionGuardResult::BodyTooLarge { size, max } => {
                warn!(
                    "[CONN_PROTECT] Request body too large: IP={} size={} max={} path={}",
                    client_ip,
                    size,
                    max,
                    req.path()
                );

                let response = HttpResponse::build(StatusCode::PAYLOAD_TOO_LARGE)
                    .json(serde_json::json!({
                        "error": "BODY_TOO_LARGE",
                        "message": "Request body exceeds maximum allowed size",
                        "size_bytes": size,
                        "max_bytes": max
                    }));

                Box::pin(async move {
                    Ok(req.into_response(response).map_into_right_body())
                })
            }
        }
    }
}

/// Extract client IP from request, handling proxies
fn extract_client_ip(req: &ServiceRequest) -> IpAddr {
    // Try X-Forwarded-For header first (for reverse proxies)
    if let Some(forwarded) = req.headers().get("X-Forwarded-For") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            // X-Forwarded-For can contain multiple IPs: "client, proxy1, proxy2"
            // The first one is the original client
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }

    // Try X-Real-IP header (nginx style)
    if let Some(real_ip) = req.headers().get("X-Real-IP") {
        if let Ok(real_ip_str) = real_ip.to_str() {
            if let Ok(ip) = real_ip_str.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // Fall back to peer address
    req.peer_addr()
        .map(|addr| addr.ip())
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap())
}
