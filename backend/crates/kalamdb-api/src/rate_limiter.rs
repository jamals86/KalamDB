//! Rate limiting module for REST API and WebSocket connections
//!
//! This module provides token bucket-based rate limiting to prevent abuse
//! and ensure fair resource allocation across users and connections.
//!
//! ## Protection Layers
//!
//! 1. **IP-based protection** (pre-auth): Limits connections and requests per IP
//! 2. **User-based rate limiting** (post-auth): Limits queries per authenticated user
//! 3. **Connection-based rate limiting**: Limits WebSocket messages per connection
//!
//! ## DoS Prevention
//!
//! - Per-IP connection limits prevent connection exhaustion
//! - Per-IP request rate limits prevent unauthenticated floods
//! - Automatic IP banning for persistent abusers
//! - Request body size limits prevent memory exhaustion

use kalamdb_commons::models::UserId;
use kalamdb_core::live::ConnectionId;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Rate limit configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum queries per second per user
    pub max_queries_per_user: u32,

    /// Maximum subscriptions per user
    pub max_subscriptions_per_user: u32,

    /// Maximum messages per second per connection
    pub max_messages_per_connection: u32,

    /// Time window for rate limiting (default: 1 second)
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_queries_per_user: 10,         // 10 queries/second per user
            max_subscriptions_per_user: 100,  // 100 total subscriptions per user
            max_messages_per_connection: 100, // 100 messages/second per connection
            window: Duration::from_secs(1),
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Maximum tokens in the bucket
    capacity: u32,

    /// Current number of tokens
    tokens: u32,

    /// Last refill time
    last_refill: Instant,

    /// Refill rate (tokens per window)
    refill_rate: u32,

    /// Refill window duration
    window: Duration,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: u32, window: Duration) -> Self {
        Self {
            capacity,
            tokens: capacity,
            last_refill: Instant::now(),
            refill_rate,
            window,
        }
    }

    /// Try to consume tokens
    /// Returns true if successful, false if insufficient tokens
    fn try_consume(&mut self, tokens: u32) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        // Refill tokens continuously based on elapsed time (not just when window expires)
        // Example: 100 tokens/sec, elapsed 0.1s → refill 10 tokens
        let elapsed_secs = elapsed.as_secs_f64();
        let tokens_per_sec = self.refill_rate as f64 / self.window.as_secs_f64();
        let tokens_to_add = (tokens_per_sec * elapsed_secs) as u32;

        if tokens_to_add > 0 {
            self.tokens = self.capacity.min(self.tokens + tokens_to_add);
            self.last_refill = now;
        }
    }

    /// Get current token count (after refill)
    fn available_tokens(&mut self) -> u32 {
        self.refill();
        self.tokens
    }
}

/// Rate limiter for users and connections
pub struct RateLimiter {
    config: RateLimitConfig,
    user_query_buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
    user_subscription_counts: Arc<RwLock<HashMap<String, u32>>>,
    connection_message_buckets: Arc<RwLock<HashMap<ConnectionId, TokenBucket>>>,
}

impl RateLimiter {
    /// Create a new rate limiter with default config
    pub fn new() -> Self {
        Self::with_config(RateLimitConfig::default())
    }

    /// Create a new rate limiter with custom config
    pub fn with_config(config: RateLimitConfig) -> Self {
        Self {
            config,
            user_query_buckets: Arc::new(RwLock::new(HashMap::new())),
            user_subscription_counts: Arc::new(RwLock::new(HashMap::new())),
            connection_message_buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a user can execute a query
    /// Returns true if allowed, false if rate limit exceeded
    pub fn check_query_rate(&self, user_id: &UserId) -> bool {
        let user_key = user_id.as_str().to_string();
        let mut buckets = self.user_query_buckets.write().unwrap();

        let bucket = buckets.entry(user_key.clone()).or_insert_with(|| {
            TokenBucket::new(
                self.config.max_queries_per_user,
                self.config.max_queries_per_user,
                self.config.window,
            )
        });

        bucket.try_consume(1)
    }

    /// Check if a user can create a new subscription
    /// Returns true if allowed, false if limit exceeded
    pub fn check_subscription_limit(&self, user_id: &UserId) -> bool {
        let user_key = user_id.as_str().to_string();
        let counts = self.user_subscription_counts.read().unwrap();

        match counts.get(&user_key) {
            Some(&count) => count < self.config.max_subscriptions_per_user,
            None => true, // No subscriptions yet
        }
    }

    /// Increment user subscription count
    pub fn increment_subscription(&self, user_id: &UserId) {
        let user_key = user_id.as_str().to_string();
        let mut counts = self.user_subscription_counts.write().unwrap();
        *counts.entry(user_key).or_insert(0) += 1;
    }

    /// Decrement user subscription count
    pub fn decrement_subscription(&self, user_id: &UserId) {
        let user_key = user_id.as_str().to_string();
        let mut counts = self.user_subscription_counts.write().unwrap();
        if let Some(count) = counts.get_mut(&user_key) {
            *count = count.saturating_sub(1);
        }
    }

    /// Check if a connection can send a message
    /// Returns true if allowed, false if rate limit exceeded
    pub fn check_message_rate(&self, connection_id: &ConnectionId) -> bool {
        let mut buckets = self.connection_message_buckets.write().unwrap();

        let bucket = buckets.entry(connection_id.clone()).or_insert_with(|| {
            TokenBucket::new(
                self.config.max_messages_per_connection,
                self.config.max_messages_per_connection,
                self.config.window,
            )
        });

        bucket.try_consume(1)
    }

    /// Clean up rate limit state for a connection
    pub fn cleanup_connection(&self, connection_id: &ConnectionId) {
        let mut buckets = self.connection_message_buckets.write().unwrap();
        buckets.remove(connection_id);
    }

    /// Get current rate limit stats for a user
    pub fn get_user_stats(&self, user_id: &UserId) -> (u32, u32) {
        let user_key = user_id.as_str().to_string();

        let available_queries = {
            let mut buckets = self.user_query_buckets.write().unwrap();
            buckets
                .get_mut(&user_key)
                .map(|b| b.available_tokens())
                .unwrap_or(self.config.max_queries_per_user)
        };

        let subscription_count = {
            let counts = self.user_subscription_counts.read().unwrap();
            counts.get(&user_key).copied().unwrap_or(0)
        };

        (available_queries, subscription_count)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// IP-Based Connection Guard (DoS Protection)
// ============================================================================

/// Configuration for IP-based connection protection
#[derive(Debug, Clone)]
pub struct ConnectionGuardConfig {
    /// Maximum concurrent connections per IP
    pub max_connections_per_ip: u32,

    /// Maximum requests per second per IP (before authentication)
    pub max_requests_per_ip_per_sec: u32,

    /// Maximum request body size in bytes
    pub request_body_limit_bytes: usize,

    /// Duration to ban abusive IPs
    pub ban_duration: Duration,

    /// Enable protection (can be disabled for testing)
    pub enabled: bool,
}

impl Default for ConnectionGuardConfig {
    fn default() -> Self {
        Self {
            max_connections_per_ip: 100,
            max_requests_per_ip_per_sec: 200,
            request_body_limit_bytes: 10 * 1024 * 1024, // 10MB
            ban_duration: Duration::from_secs(300),     // 5 minutes
            enabled: true,
        }
    }
}

/// Tracks per-IP state for connection protection
struct IpState {
    /// Current active connection count
    active_connections: u32,

    /// Request rate token bucket
    request_bucket: TokenBucket,

    /// Ban expiration time (None if not banned)
    banned_until: Option<Instant>,

    /// Total violations (for escalating bans)
    violation_count: u32,
}

impl IpState {
    fn new(config: &ConnectionGuardConfig) -> Self {
        Self {
            active_connections: 0,
            request_bucket: TokenBucket::new(
                config.max_requests_per_ip_per_sec,
                config.max_requests_per_ip_per_sec,
                Duration::from_secs(1),
            ),
            banned_until: None,
            violation_count: 0,
        }
    }
}

/// Result of checking an IP against the connection guard
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionGuardResult {
    /// Request allowed
    Allowed,

    /// IP is temporarily banned
    Banned { until: Instant },

    /// Too many concurrent connections from this IP
    TooManyConnections { current: u32, max: u32 },

    /// Request rate exceeded for this IP
    RateLimitExceeded,

    /// Request body too large
    BodyTooLarge { size: usize, max: usize },
}

/// IP-based connection guard for DoS protection
///
/// Provides multiple layers of protection:
/// 1. Connection limits per IP
/// 2. Request rate limits per IP (before authentication)
/// 3. Automatic banning of abusive IPs
/// 4. Request body size limits
pub struct ConnectionGuard {
    config: ConnectionGuardConfig,
    ip_states: Arc<RwLock<HashMap<IpAddr, IpState>>>,
}

impl ConnectionGuard {
    /// Create a new connection guard with default config
    pub fn new() -> Self {
        Self::with_config(ConnectionGuardConfig::default())
    }

    /// Create a new connection guard with custom config
    pub fn with_config(config: ConnectionGuardConfig) -> Self {
        Self {
            config,
            ip_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request from the given IP should be allowed
    ///
    /// Call this at the START of request handling, before any work is done.
    /// Returns `Allowed` if the request can proceed.
    ///
    /// Note: Localhost (127.0.0.1, ::1) is exempt from rate limiting to avoid
    /// blocking local development and testing.
    pub fn check_request(&self, ip: IpAddr, body_size: Option<usize>) -> ConnectionGuardResult {
        if !self.config.enabled {
            return ConnectionGuardResult::Allowed;
        }

        // Exempt localhost from rate limiting (for development and testing)
        if ip.is_loopback() {
            // Still check body size for localhost
            if let Some(size) = body_size {
                if size > self.config.request_body_limit_bytes {
                    return ConnectionGuardResult::BodyTooLarge {
                        size,
                        max: self.config.request_body_limit_bytes,
                    };
                }
            }
            return ConnectionGuardResult::Allowed;
        }

        // Check body size first (doesn't need state)
        if let Some(size) = body_size {
            if size > self.config.request_body_limit_bytes {
                return ConnectionGuardResult::BodyTooLarge {
                    size,
                    max: self.config.request_body_limit_bytes,
                };
            }
        }

        let mut states = self.ip_states.write().unwrap();
        let state = states
            .entry(ip)
            .or_insert_with(|| IpState::new(&self.config));

        // Check if IP is banned
        if let Some(banned_until) = state.banned_until {
            if Instant::now() < banned_until {
                return ConnectionGuardResult::Banned { until: banned_until };
            } else {
                // Ban expired, clear it
                state.banned_until = None;
            }
        }

        // Check request rate limit
        if !state.request_bucket.try_consume(1) {
            state.violation_count += 1;

            // Ban IP after repeated violations (escalating ban duration)
            if state.violation_count >= 10 {
                let ban_multiplier = (state.violation_count / 10).min(6); // Max 6x
                let ban_duration = self.config.ban_duration * ban_multiplier;
                state.banned_until = Some(Instant::now() + ban_duration);

                log::warn!(
                    "[CONN_GUARD] IP {} banned for {:?} after {} violations",
                    ip,
                    ban_duration,
                    state.violation_count
                );
            }

            return ConnectionGuardResult::RateLimitExceeded;
        }

        ConnectionGuardResult::Allowed
    }

    /// Register a new connection from an IP
    ///
    /// Call this when a new TCP/WebSocket connection is established.
    /// Returns error if the connection limit is exceeded.
    ///
    /// Note: Localhost is exempt from connection limits.
    pub fn register_connection(&self, ip: IpAddr) -> ConnectionGuardResult {
        if !self.config.enabled {
            return ConnectionGuardResult::Allowed;
        }

        // Exempt localhost from connection limits
        if ip.is_loopback() {
            return ConnectionGuardResult::Allowed;
        }

        let mut states = self.ip_states.write().unwrap();
        let state = states
            .entry(ip)
            .or_insert_with(|| IpState::new(&self.config));

        // Check if IP is banned
        if let Some(banned_until) = state.banned_until {
            if Instant::now() < banned_until {
                return ConnectionGuardResult::Banned { until: banned_until };
            } else {
                state.banned_until = None;
            }
        }

        // Check connection limit
        if state.active_connections >= self.config.max_connections_per_ip {
            state.violation_count += 1;

            // Ban IP after repeated connection limit violations
            if state.violation_count >= 5 {
                let ban_duration = self.config.ban_duration;
                state.banned_until = Some(Instant::now() + ban_duration);

                log::warn!(
                    "[CONN_GUARD] IP {} banned for {:?} - exceeded connection limit {} times",
                    ip,
                    ban_duration,
                    state.violation_count
                );
            }

            return ConnectionGuardResult::TooManyConnections {
                current: state.active_connections,
                max: self.config.max_connections_per_ip,
            };
        }

        state.active_connections += 1;
        ConnectionGuardResult::Allowed
    }

    /// Unregister a connection when it closes
    pub fn unregister_connection(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        let mut states = self.ip_states.write().unwrap();
        if let Some(state) = states.get_mut(&ip) {
            state.active_connections = state.active_connections.saturating_sub(1);
        }
    }

    /// Get the current connection count for an IP
    pub fn get_connection_count(&self, ip: IpAddr) -> u32 {
        let states = self.ip_states.read().unwrap();
        states
            .get(&ip)
            .map(|s| s.active_connections)
            .unwrap_or(0)
    }

    /// Check if an IP is currently banned
    pub fn is_banned(&self, ip: IpAddr) -> bool {
        let states = self.ip_states.read().unwrap();
        if let Some(state) = states.get(&ip) {
            if let Some(banned_until) = state.banned_until {
                return Instant::now() < banned_until;
            }
        }
        false
    }

    /// Manually ban an IP (for external threat detection)
    pub fn ban_ip(&self, ip: IpAddr, duration: Duration) {
        let mut states = self.ip_states.write().unwrap();
        let state = states
            .entry(ip)
            .or_insert_with(|| IpState::new(&self.config));

        state.banned_until = Some(Instant::now() + duration);
        state.violation_count += 10; // Treat manual ban as significant violation

        log::warn!("[CONN_GUARD] IP {} manually banned for {:?}", ip, duration);
    }

    /// Unban an IP
    pub fn unban_ip(&self, ip: IpAddr) {
        let mut states = self.ip_states.write().unwrap();
        if let Some(state) = states.get_mut(&ip) {
            state.banned_until = None;
            log::info!("[CONN_GUARD] IP {} unbanned", ip);
        }
    }

    /// Clean up old IP state entries (call periodically)
    pub fn cleanup_stale_entries(&self) {
        let mut states = self.ip_states.write().unwrap();
        let now = Instant::now();

        states.retain(|ip, state| {
            // Keep if has active connections
            if state.active_connections > 0 {
                return true;
            }

            // Keep if currently banned
            if let Some(banned_until) = state.banned_until {
                if now < banned_until {
                    return true;
                }
            }

            // Keep if has recent violations (may need to track for ban escalation)
            if state.violation_count > 0 {
                // Reset after some time to give IPs a fresh start
                // For now, just log and remove
                log::debug!(
                    "[CONN_GUARD] Cleaning up stale entry for IP {} (violations: {})",
                    ip,
                    state.violation_count
                );
            }

            false
        });
    }

    /// Get statistics for monitoring
    pub fn get_stats(&self) -> ConnectionGuardStats {
        let states = self.ip_states.read().unwrap();

        let total_ips = states.len();
        let banned_ips = states
            .values()
            .filter(|s| s.banned_until.map(|t| Instant::now() < t).unwrap_or(false))
            .count();
        let total_connections: u32 = states.values().map(|s| s.active_connections).sum();
        let total_violations: u32 = states.values().map(|s| s.violation_count).sum();

        ConnectionGuardStats {
            tracked_ips: total_ips,
            banned_ips,
            total_active_connections: total_connections,
            total_violations,
        }
    }
}

impl Default for ConnectionGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics from the connection guard
#[derive(Debug, Clone)]
pub struct ConnectionGuardStats {
    /// Number of IPs being tracked
    pub tracked_ips: usize,

    /// Number of currently banned IPs
    pub banned_ips: usize,

    /// Total active connections across all IPs
    pub total_active_connections: u32,

    /// Total violation count across all IPs
    pub total_violations: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10, 10, Duration::from_secs(1));

        // Should succeed - have 10 tokens
        assert!(bucket.try_consume(5));
        assert_eq!(bucket.available_tokens(), 5);

        // Should succeed - have 5 tokens left
        assert!(bucket.try_consume(5));
        assert_eq!(bucket.available_tokens(), 0);

        // Should fail - no tokens left
        assert!(!bucket.try_consume(1));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10, 10, Duration::from_millis(100));

        // Consume all tokens
        assert!(bucket.try_consume(10));
        assert_eq!(bucket.available_tokens(), 0);

        // Wait for refill
        thread::sleep(Duration::from_millis(150));

        // Should have refilled
        assert!(bucket.try_consume(10));
    }

    #[test]
    fn test_query_rate_limiting() {
        let config = RateLimitConfig {
            max_queries_per_user: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::with_config(config);
        let user_id = UserId::from("user-123");

        // Should allow first 5 queries
        for _ in 0..5 {
            assert!(limiter.check_query_rate(&user_id));
        }

        // 6th query should be denied
        assert!(!limiter.check_query_rate(&user_id));
    }

    #[test]
    fn test_subscription_limit() {
        let config = RateLimitConfig {
            max_subscriptions_per_user: 3,
            ..Default::default()
        };
        let limiter = RateLimiter::with_config(config);
        let user_id = UserId::from("user-123");

        // Should allow first 3 subscriptions
        for _ in 0..3 {
            assert!(limiter.check_subscription_limit(&user_id));
            limiter.increment_subscription(&user_id);
        }

        // 4th subscription should be denied
        assert!(!limiter.check_subscription_limit(&user_id));

        // Decrement and should allow again
        limiter.decrement_subscription(&user_id);
        assert!(limiter.check_subscription_limit(&user_id));
    }

    #[test]
    fn test_message_rate_limiting() {
        let config = RateLimitConfig {
            max_messages_per_connection: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::with_config(config);
        let conn_id = ConnectionId::new("conn-123");

        // Should allow first 5 messages
        for _ in 0..5 {
            assert!(limiter.check_message_rate(&conn_id));
        }

        // 6th message should be denied
        assert!(!limiter.check_message_rate(&conn_id));
    }

    #[test]
    fn test_connection_cleanup() {
        let limiter = RateLimiter::new();
        let conn_id = ConnectionId::new("conn-123");

        // Send messages
        for _ in 0..5 {
            limiter.check_message_rate(&conn_id);
        }

        // Cleanup
        limiter.cleanup_connection(&conn_id);
        // Should be able to send again (new bucket created)
        assert!(limiter.check_message_rate(&conn_id));
    }

    #[test]
    fn test_user_stats() {
        let config = RateLimitConfig {
            max_queries_per_user: 10,
            max_subscriptions_per_user: 5,
            ..Default::default()
        };
        let limiter = RateLimiter::with_config(config);
        let user_id = UserId::from("user-123");

        // Initial stats
        let (queries, subs) = limiter.get_user_stats(&user_id);
        assert_eq!(queries, 10);
        assert_eq!(subs, 0);

        // Consume some queries
        limiter.check_query_rate(&user_id);
        limiter.check_query_rate(&user_id);

        // Add subscriptions
        limiter.increment_subscription(&user_id);
        limiter.increment_subscription(&user_id);

        // Check stats
        let (queries, subs) = limiter.get_user_stats(&user_id);
        assert_eq!(queries, 8);
        assert_eq!(subs, 2);
    }

    // Connection Guard Tests

    #[test]
    fn test_connection_guard_allows_normal_requests() {
        let guard = ConnectionGuard::new();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Should allow normal requests
        assert_eq!(
            guard.check_request(ip, Some(1000)),
            ConnectionGuardResult::Allowed
        );
    }

    #[test]
    fn test_connection_guard_body_size_limit() {
        let config = ConnectionGuardConfig {
            request_body_limit_bytes: 1000,
            ..Default::default()
        };
        let guard = ConnectionGuard::with_config(config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Should reject large body
        let result = guard.check_request(ip, Some(2000));
        assert!(matches!(result, ConnectionGuardResult::BodyTooLarge { .. }));
    }

    #[test]
    fn test_connection_guard_connection_limit() {
        let config = ConnectionGuardConfig {
            max_connections_per_ip: 2,
            ..Default::default()
        };
        let guard = ConnectionGuard::with_config(config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // First two connections should succeed
        assert_eq!(guard.register_connection(ip), ConnectionGuardResult::Allowed);
        assert_eq!(guard.register_connection(ip), ConnectionGuardResult::Allowed);

        // Third should fail
        let result = guard.register_connection(ip);
        assert!(matches!(
            result,
            ConnectionGuardResult::TooManyConnections { .. }
        ));

        // After unregistering one, should succeed again
        guard.unregister_connection(ip);
        assert_eq!(guard.register_connection(ip), ConnectionGuardResult::Allowed);
    }

    #[test]
    fn test_connection_guard_rate_limit() {
        let config = ConnectionGuardConfig {
            max_requests_per_ip_per_sec: 5,
            ..Default::default()
        };
        let guard = ConnectionGuard::with_config(config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // First 5 requests should succeed
        for _ in 0..5 {
            assert_eq!(guard.check_request(ip, None), ConnectionGuardResult::Allowed);
        }

        // 6th should fail
        assert_eq!(
            guard.check_request(ip, None),
            ConnectionGuardResult::RateLimitExceeded
        );
    }

    #[test]
    fn test_connection_guard_ban_after_violations() {
        let config = ConnectionGuardConfig {
            max_requests_per_ip_per_sec: 1,
            ban_duration: Duration::from_millis(100),
            ..Default::default()
        };
        let guard = ConnectionGuard::with_config(config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Cause 10 violations to trigger ban
        for _ in 0..15 {
            guard.check_request(ip, None);
        }

        // Should be banned now
        assert!(guard.is_banned(ip));

        // Wait for ban to expire
        thread::sleep(Duration::from_millis(150));

        // Should no longer be banned
        assert!(!guard.is_banned(ip));
    }

    #[test]
    fn test_connection_guard_manual_ban() {
        let guard = ConnectionGuard::new();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Should not be banned initially
        assert!(!guard.is_banned(ip));

        // Ban the IP
        guard.ban_ip(ip, Duration::from_millis(100));

        // Should be banned
        assert!(guard.is_banned(ip));
        let result = guard.check_request(ip, None);
        assert!(matches!(result, ConnectionGuardResult::Banned { .. }));

        // Unban
        guard.unban_ip(ip);
        assert!(!guard.is_banned(ip));
    }

    #[test]
    fn test_connection_guard_stats() {
        let guard = ConnectionGuard::new();
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        // Register some connections
        guard.register_connection(ip1);
        guard.register_connection(ip1);
        guard.register_connection(ip2);

        let stats = guard.get_stats();
        assert_eq!(stats.tracked_ips, 2);
        assert_eq!(stats.total_active_connections, 3);
    }
}
