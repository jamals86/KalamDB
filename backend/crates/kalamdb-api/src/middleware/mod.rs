//! Middleware components for KalamDB API
//!
//! This module provides Actix-Web middleware for:
//! - Authentication (HTTP Basic Auth, JWT Bearer tokens)
//! - Request logging and monitoring
//! - Rate limiting

pub mod auth;

pub use auth::AuthMiddleware;
