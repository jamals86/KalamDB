//! Rate limiting module for REST API and WebSocket connections
//!
//! This module provides token bucket-based rate limiting to prevent abuse
//! and ensure fair resource allocation across users and connections.

use kalamdb_commons::models::UserId;
use kalamdb_core::live::ConnectionId;
use std::collections::HashMap;
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
}
