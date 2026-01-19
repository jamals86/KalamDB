//! Rate limiting and connection guard module
//!
//! This module provides lightweight, zero-copy rate limiting for REST API and WebSocket connections.
//!
//! ## Components
//!
//! - [`RateLimiter`]: User and connection-based rate limiting using token buckets
//! - [`ConnectionGuard`]: IP-based DoS protection with automatic banning
//!
//! ## Design Principles
//!
//! - **Zero-copy**: Uses `Arc` for sharing without cloning data
//! - **Lock-free reads**: Moka cache provides concurrent access without global locks
//! - **Automatic cleanup**: TTL-based eviction eliminates manual cleanup overhead
//! - **Minimal allocations**: Keys are interned, values use interior mutability

mod connection_guard;
mod rate_limiter;
mod token_bucket;

pub use connection_guard::{ConnectionGuard, ConnectionGuardResult, ConnectionGuardStats};
pub use rate_limiter::RateLimiter;

// Re-export config from kalamdb-configs for convenience
pub use kalamdb_configs::RateLimitSettings;
