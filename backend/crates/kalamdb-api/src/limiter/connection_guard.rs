//! IP-based connection guard for DoS protection
//!
//! Provides multiple layers of protection against denial-of-service attacks:
//! 1. Connection limits per IP
//! 2. Request rate limits per IP (before authentication)
//! 3. Automatic banning of abusive IPs
//! 4. Request body size limits
//!
//! Uses Moka cache for automatic TTL-based cleanup - no manual cleanup needed.

use super::token_bucket::TokenBucket;
use kalamdb_configs::RateLimitSettings;
use moka::sync::Cache;
use parking_lot::Mutex;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Tracks per-IP state for connection protection
///
/// Uses interior mutability via parking_lot::Mutex for fast, fair locking.
/// Atomic counters used where possible to avoid lock contention.
struct IpState {
    /// Current active connection count (atomic for fast inc/dec)
    active_connections: AtomicU32,

    /// Request rate token bucket (needs mutex for refill logic)
    request_bucket: Mutex<TokenBucket>,

    /// Ban expiration time (None if not banned)
    banned_until: Mutex<Option<Instant>>,

    /// Total violations (for escalating bans)
    violation_count: AtomicU32,
}

impl IpState {
    fn new(max_requests_per_sec: u32) -> Self {
        Self {
            active_connections: AtomicU32::new(0),
            request_bucket: Mutex::new(TokenBucket::new(
                max_requests_per_sec,
                max_requests_per_sec,
                Duration::from_secs(1),
            )),
            banned_until: Mutex::new(None),
            violation_count: AtomicU32::new(0),
        }
    }

    /// Check if this IP is currently banned
    #[inline]
    fn is_banned(&self) -> Option<Instant> {
        let mut banned_until = self
            .banned_until
            .lock();
        if let Some(until) = *banned_until {
            if Instant::now() < until {
                return Some(until);
            }
            // Ban expired, clear it
            *banned_until = None;
        }
        None
    }

    /// Set ban expiration
    #[inline]
    fn set_banned_until(&self, until: Instant) {
        *self
            .banned_until
            .lock() = Some(until);
    }

    /// Clear ban
    #[inline]
    fn clear_ban(&self) {
        *self
            .banned_until
            .lock() = None;
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
/// Uses Moka cache with TTL for automatic cleanup of inactive IPs.
/// All per-IP state uses interior mutability for zero-copy access.
pub struct ConnectionGuard {
    /// Maximum concurrent connections per IP
    max_connections_per_ip: u32,
    /// Maximum requests per second per IP
    max_requests_per_ip_per_sec: u32,
    /// Maximum request body size in bytes
    request_body_limit_bytes: usize,
    /// Duration to ban abusive IPs
    ban_duration: Duration,
    /// Enable protection
    enabled: bool,
    /// IP states stored in Moka cache with TTL
    ip_states: Cache<IpAddr, Arc<IpState>>,
}

impl ConnectionGuard {
    /// Create a new connection guard with default config
    pub fn new() -> Self {
        Self::with_config(&RateLimitSettings::default())
    }

    /// Create a new connection guard from config settings
    pub fn with_config(config: &RateLimitSettings) -> Self {
        let cache_ttl = Duration::from_secs(config.cache_ttl_seconds);
        let max_requests = config.max_requests_per_ip_per_sec;

        let ip_states = Cache::builder()
            .max_capacity(config.cache_max_entries)
            .time_to_idle(cache_ttl)
            // Custom eviction: log IPs with active connections
            .eviction_listener(move |_ip, state: Arc<IpState>, _cause| {
                let active = state.active_connections.load(Ordering::Relaxed);
                if active > 0 {
                    log::debug!(
                        "[CONN_GUARD] Evicting IP with {} active connections",
                        active
                    );
                }
            })
            .build();

        Self {
            max_connections_per_ip: config.max_connections_per_ip,
            max_requests_per_ip_per_sec: max_requests,
            request_body_limit_bytes: config.request_body_limit_bytes,
            ban_duration: Duration::from_secs(config.ban_duration_seconds),
            enabled: config.enable_connection_protection,
            ip_states,
        }
    }

    /// Get or create IP state (zero-copy via Arc)
    #[inline]
    fn get_or_create_state(&self, ip: IpAddr) -> Arc<IpState> {
        let max_requests = self.max_requests_per_ip_per_sec;
        self.ip_states
            .get_with(ip, || Arc::new(IpState::new(max_requests)))
    }

    /// Check if a request from the given IP should be allowed
    ///
    /// Call this at the START of request handling, before any work is done.
    /// Returns `Allowed` if the request can proceed.
    ///
    /// Note: Localhost (127.0.0.1, ::1) is exempt from rate limiting.
    pub fn check_request(&self, ip: IpAddr, body_size: Option<usize>) -> ConnectionGuardResult {
        if !self.enabled {
            return ConnectionGuardResult::Allowed;
        }

        // Check body size first (doesn't need state lookup)
        if let Some(size) = body_size {
            if size > self.request_body_limit_bytes {
                return ConnectionGuardResult::BodyTooLarge {
                    size,
                    max: self.request_body_limit_bytes,
                };
            }
        }

        // Exempt localhost from rate limiting (for development and testing)
        if ip.is_loopback() {
            return ConnectionGuardResult::Allowed;
        }

        let state = self.get_or_create_state(ip);

        // Check if IP is banned
        if let Some(banned_until) = state.is_banned() {
            return ConnectionGuardResult::Banned {
                until: banned_until,
            };
        }

        // Check request rate limit
        if !state
            .request_bucket
            .lock()
            .try_consume(1)
        {
            let violations = state.violation_count.fetch_add(1, Ordering::Relaxed) + 1;

            // Ban IP after repeated violations (escalating ban duration)
            if violations >= 10 {
                let ban_multiplier = (violations / 10).min(6); // Max 6x
                let ban_duration = self.ban_duration * ban_multiplier;
                state.set_banned_until(Instant::now() + ban_duration);

                log::warn!(
                    "[CONN_GUARD] IP {} banned for {:?} after {} violations",
                    ip,
                    ban_duration,
                    violations
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
        if !self.enabled {
            return ConnectionGuardResult::Allowed;
        }

        // Exempt localhost from connection limits
        if ip.is_loopback() {
            return ConnectionGuardResult::Allowed;
        }

        let state = self.get_or_create_state(ip);

        // Check if IP is banned
        if let Some(banned_until) = state.is_banned() {
            return ConnectionGuardResult::Banned {
                until: banned_until,
            };
        }

        // Atomically check and increment connection count
        let current = state.active_connections.load(Ordering::Relaxed);
        if current >= self.max_connections_per_ip {
            let violations = state.violation_count.fetch_add(1, Ordering::Relaxed) + 1;

            // Ban IP after repeated connection limit violations
            if violations >= 5 {
                state.set_banned_until(Instant::now() + self.ban_duration);

                log::warn!(
                    "[CONN_GUARD] IP {} banned for {:?} - exceeded connection limit {} times",
                    ip,
                    self.ban_duration,
                    violations
                );
            }

            return ConnectionGuardResult::TooManyConnections {
                current,
                max: self.max_connections_per_ip,
            };
        }

        state.active_connections.fetch_add(1, Ordering::Relaxed);
        ConnectionGuardResult::Allowed
    }

    /// Unregister a connection when it closes
    #[inline]
    pub fn unregister_connection(&self, ip: IpAddr) {
        if !self.enabled {
            return;
        }

        if let Some(state) = self.ip_states.get(&ip) {
            state
                .active_connections
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                    Some(v.saturating_sub(1))
                })
                .ok();
        }
    }

    /// Get the current connection count for an IP
    #[inline]
    pub fn get_connection_count(&self, ip: IpAddr) -> u32 {
        self.ip_states
            .get(&ip)
            .map(|s| s.active_connections.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Check if an IP is currently banned
    #[inline]
    pub fn is_banned(&self, ip: IpAddr) -> bool {
        self.ip_states
            .get(&ip)
            .and_then(|state| state.is_banned())
            .is_some()
    }

    /// Manually ban an IP (for external threat detection)
    pub fn ban_ip(&self, ip: IpAddr, duration: Duration) {
        let state = self.get_or_create_state(ip);
        state.set_banned_until(Instant::now() + duration);
        state.violation_count.fetch_add(10, Ordering::Relaxed); // Treat manual ban as significant

        log::warn!("[CONN_GUARD] IP {} manually banned for {:?}", ip, duration);
    }

    /// Unban an IP
    pub fn unban_ip(&self, ip: IpAddr) {
        if let Some(state) = self.ip_states.get(&ip) {
            state.clear_ban();
            log::info!("[CONN_GUARD] IP {} unbanned", ip);
        }
    }

    /// Get statistics for monitoring
    pub fn get_stats(&self) -> ConnectionGuardStats {
        let total_ips = self.ip_states.entry_count() as usize;
        let mut banned_ips = 0usize;
        let mut total_connections = 0u32;
        let mut total_violations = 0u32;

        // Iterate through cache entries
        // Note: This is O(n) - use sparingly for monitoring
        self.ip_states.iter().for_each(|(_, state)| {
            if state.is_banned().is_some() {
                banned_ips += 1;
            }
            total_connections += state.active_connections.load(Ordering::Relaxed);
            total_violations += state.violation_count.load(Ordering::Relaxed);
        });

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

    fn test_config(
        max_conn: u32,
        max_req: u32,
        body_limit: usize,
        ban_secs: u64,
    ) -> RateLimitSettings {
        RateLimitSettings {
            max_connections_per_ip: max_conn,
            max_requests_per_ip_per_sec: max_req,
            request_body_limit_bytes: body_limit,
            ban_duration_seconds: ban_secs,
            enable_connection_protection: true,
            ..Default::default()
        }
    }

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
        let config = test_config(100, 200, 1000, 300);
        let guard = ConnectionGuard::with_config(&config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Should reject large body
        let result = guard.check_request(ip, Some(2000));
        assert!(matches!(result, ConnectionGuardResult::BodyTooLarge { .. }));
    }

    #[test]
    fn test_connection_guard_connection_limit() {
        let config = test_config(2, 200, 10_000_000, 300);
        let guard = ConnectionGuard::with_config(&config);
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
        let config = test_config(100, 5, 10_000_000, 300);
        let guard = ConnectionGuard::with_config(&config);
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
        let config = test_config(100, 1, 10_000_000, 1); // 1 second ban for fast test
        let guard = ConnectionGuard::with_config(&config);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        // Cause 10 violations to trigger ban
        for _ in 0..15 {
            guard.check_request(ip, None);
        }

        // Should be banned now
        assert!(guard.is_banned(ip));

        // Wait for ban to expire
        thread::sleep(Duration::from_millis(1100));

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

    #[test]
    fn test_connection_guard_localhost_exempt() {
        let config = test_config(100, 1, 10_000_000, 300);
        let guard = ConnectionGuard::with_config(&config);
        let localhost: IpAddr = "127.0.0.1".parse().unwrap();

        // Localhost should always be allowed regardless of rate limit
        for _ in 0..100 {
            assert_eq!(
                guard.check_request(localhost, None),
                ConnectionGuardResult::Allowed
            );
        }
    }

    #[test]
    fn test_connection_guard_concurrent_access() {
        use std::sync::Arc as StdArc;

        let guard = StdArc::new(ConnectionGuard::new());
        let ip: IpAddr = "192.168.1.100".parse().unwrap();

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let guard = guard.clone();
                thread::spawn(move || {
                    for _ in 0..50 {
                        guard.check_request(ip, None);
                        thread::sleep(Duration::from_micros(100));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Should complete without deadlock
    }
}
