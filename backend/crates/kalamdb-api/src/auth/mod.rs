//! Authentication and authorization module
//!
//! This module provides JWT token validation and user identity extraction
//! for securing REST API and WebSocket connections.

pub mod jwt;

pub use jwt::{Claims, JwtAuth, JwtError};
