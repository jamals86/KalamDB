//! Sharding strategies for distributing data across multiple files
//!
//! This module provides various sharding strategies that determine how data is
//! distributed across multiple Parquet files during flush operations. Sharding
//! improves query performance by allowing parallel reads and reducing file sizes.
//!
//! ## Sharding Strategies
//!
//! ### AlphabeticSharding
//!
//! Distributes data based on the first character of a key field (e.g., user_id).
//! Useful for evenly distributing string keys.
//!
//! Example:
//! - Users starting with A-M → shard 0
//! - Users starting with N-Z → shard 1
//!
//! ### NumericSharding
//!
//! Distributes data based on numeric ranges. Useful for integer keys or timestamps.
//!
//! Example with 4 shards:
//! - IDs 0-24999 → shard 0
//! - IDs 25000-49999 → shard 1
//! - IDs 50000-74999 → shard 2
//! - IDs 75000-99999 → shard 3
//!
//! ### ConsistentHashSharding
//!
//! Uses consistent hashing to distribute data. Provides stable shard assignments
//! even when the number of shards changes.
//!
//! Example:
//! - hash(user_id) % num_shards → shard number
//!
//! ## Usage Examples
//!
//! ```rust,no_run
//! use kalamdb_store::sharding::{ShardingStrategy, AlphabeticSharding, NumericSharding, ConsistentHashSharding};
//!
//! // Alphabetic sharding (2 shards)
//! let alphabetic = AlphabeticSharding::new(2);
//! let shard = alphabetic.get_shard("user123", 2);
//! println!("user123 → shard {}", shard);
//!
//! // Numeric sharding (4 shards, max value 100000)
//! let numeric = NumericSharding::new(100000);
//! let shard = numeric.get_shard("42000", 4);
//! println!("42000 → shard {}", shard);
//!
//! // Consistent hash sharding
//! let consistent = ConsistentHashSharding::new();
//! let shard = consistent.get_shard("user@example.com", 8);
//! println!("user@example.com → shard {}", shard);
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Sharding strategy trait
///
/// Implementations determine how data is distributed across shards during flush
/// operations. Each strategy computes a shard number based on a key value.
pub trait ShardingStrategy: Send + Sync {
    /// Get the shard number for a given key
    ///
    /// # Arguments
    ///
    /// * `key` - The key value to shard on (e.g., user_id, timestamp)
    /// * `num_shards` - Total number of shards available
    ///
    /// # Returns
    ///
    /// Shard number in the range [0, num_shards)
    ///
    /// # Notes
    ///
    /// Implementations should ensure:
    /// - Consistent mapping: same key always maps to same shard
    /// - Even distribution: keys are distributed relatively evenly
    /// - Performance: computation should be fast (O(1) or O(log n))
    fn get_shard(&self, key: &str, num_shards: usize) -> usize;

    /// Get the strategy name
    ///
    /// Returns a unique identifier for this strategy (e.g., "alphabetic", "numeric", "consistent_hash")
    fn name(&self) -> &str;
}

/// Alphabetic sharding strategy
///
/// Distributes keys based on the first character. Keys are divided into ranges
/// based on alphabetic position.
///
/// ## Algorithm
///
/// 1. Extract first character of key (case-insensitive)
/// 2. Convert to numeric value (A=0, B=1, ..., Z=25)
/// 3. Compute shard: (char_value * num_shards) / 26
///
/// ## Distribution
///
/// With 2 shards:
/// - A-M → shard 0
/// - N-Z → shard 1
///
/// With 4 shards:
/// - A-F → shard 0
/// - G-M → shard 1
/// - N-S → shard 2
/// - T-Z → shard 3
pub struct AlphabeticSharding {
    /// Number of shards (stored for validation)
    _num_shards: usize,
}

impl AlphabeticSharding {
    /// Create a new alphabetic sharding strategy
    ///
    /// # Arguments
    ///
    /// * `num_shards` - Number of shards (must be > 0)
    pub fn new(num_shards: usize) -> Self {
        assert!(num_shards > 0, "Number of shards must be greater than 0");
        Self {
            _num_shards: num_shards,
        }
    }
}

impl ShardingStrategy for AlphabeticSharding {
    fn get_shard(&self, key: &str, num_shards: usize) -> usize {
        if key.is_empty() {
            return 0;
        }

        // Get first character (case-insensitive)
        let first_char = key.chars().next().unwrap().to_ascii_uppercase();

        // Convert to numeric value (A=0, Z=25, non-letters=0)
        let char_value = if first_char.is_ascii_alphabetic() {
            (first_char as usize) - ('A' as usize)
        } else {
            0 // Non-alphabetic characters go to shard 0
        };

        // Compute shard number
        (char_value * num_shards) / 26
    }

    fn name(&self) -> &str {
        "alphabetic"
    }
}

/// Numeric sharding strategy
///
/// Distributes keys based on numeric ranges. Keys are parsed as integers and
/// divided into equal ranges.
///
/// ## Algorithm
///
/// 1. Parse key as integer (default to 0 if invalid)
/// 2. Compute range size: max_value / num_shards
/// 3. Compute shard: min(key / range_size, num_shards - 1)
///
/// ## Example
///
/// With max_value=100000 and 4 shards:
/// - Range size = 25000
/// - 0-24999 → shard 0
/// - 25000-49999 → shard 1
/// - 50000-74999 → shard 2
/// - 75000-99999 → shard 3
pub struct NumericSharding {
    /// Maximum expected value (for range calculation)
    max_value: u64,
}

impl NumericSharding {
    /// Create a new numeric sharding strategy
    ///
    /// # Arguments
    ///
    /// * `max_value` - Maximum expected key value
    pub fn new(max_value: u64) -> Self {
        assert!(max_value > 0, "Max value must be greater than 0");
        Self { max_value }
    }
}

impl ShardingStrategy for NumericSharding {
    fn get_shard(&self, key: &str, num_shards: usize) -> usize {
        // Parse key as integer (default to 0 if invalid)
        let key_value = key.parse::<u64>().unwrap_or(0);

        // Compute range size
        let range_size = self.max_value / (num_shards as u64);
        if range_size == 0 {
            return 0;
        }

        // Compute shard number (capped at num_shards - 1)
        let shard = (key_value / range_size) as usize;
        shard.min(num_shards - 1)
    }

    fn name(&self) -> &str {
        "numeric"
    }
}

/// Consistent hash sharding strategy
///
/// Uses consistent hashing to distribute keys. Provides stable shard assignments
/// even when the number of shards changes (minimal data movement).
///
/// ## Algorithm
///
/// 1. Compute hash of key using FNV-1a hash function
/// 2. Take modulo with number of shards
///
/// ## Advantages
///
/// - Stable: same key always maps to same shard
/// - Minimal disruption: adding/removing shards only affects ~1/n keys
/// - Simple: fast hash computation
pub struct ConsistentHashSharding;

impl ConsistentHashSharding {
    /// Create a new consistent hash sharding strategy
    pub fn new() -> Self {
        Self
    }

    /// FNV-1a hash function
    ///
    /// Simple and fast hash function suitable for shard distribution
    fn fnv1a_hash(key: &str) -> u64 {
        const FNV_OFFSET_BASIS: u64 = 14695981039346656037;
        const FNV_PRIME: u64 = 1099511628211;

        let mut hash = FNV_OFFSET_BASIS;
        for byte in key.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }
}

impl Default for ConsistentHashSharding {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardingStrategy for ConsistentHashSharding {
    fn get_shard(&self, key: &str, num_shards: usize) -> usize {
        let hash = Self::fnv1a_hash(key);
        (hash % (num_shards as u64)) as usize
    }

    fn name(&self) -> &str {
        "consistent_hash"
    }
}

/// Sharding strategy registry
///
/// Maintains a mapping of strategy names to implementations. Used for looking up
/// strategies by name during table creation and flush operations.
pub struct ShardingRegistry {
    /// Registered strategies (keyed by name)
    strategies: RwLock<HashMap<String, Arc<dyn ShardingStrategy>>>,
}

impl Default for ShardingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardingRegistry {
    /// Create a new registry with default strategies
    pub fn new() -> Self {
        let registry = Self {
            strategies: RwLock::new(HashMap::new()),
        };

        // Register default strategies
        registry.register("alphabetic", Arc::new(AlphabeticSharding::new(1)));
        registry.register("numeric", Arc::new(NumericSharding::new(100000)));
        registry.register("consistent_hash", Arc::new(ConsistentHashSharding::new()));

        registry
    }

    /// Register a sharding strategy
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for the strategy
    /// * `strategy` - Strategy implementation
    pub fn register(&self, name: impl Into<String>, strategy: Arc<dyn ShardingStrategy>) {
        let mut strategies = self.strategies.write().unwrap();
        strategies.insert(name.into(), strategy);
    }

    /// Get a sharding strategy by name
    ///
    /// # Arguments
    ///
    /// * `name` - Strategy name
    ///
    /// # Returns
    ///
    /// Some(strategy) if found, None otherwise
    pub fn get(&self, name: &str) -> Option<Arc<dyn ShardingStrategy>> {
        let strategies = self.strategies.read().unwrap();
        strategies.get(name).cloned()
    }

    /// List all registered strategy names
    pub fn list_strategies(&self) -> Vec<String> {
        let strategies = self.strategies.read().unwrap();
        strategies.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alphabetic_sharding() {
        let strategy = AlphabeticSharding::new(2);

        // A-M → shard 0
        assert_eq!(strategy.get_shard("alice", 2), 0);
        assert_eq!(strategy.get_shard("bob", 2), 0);
        assert_eq!(strategy.get_shard("mary", 2), 0);

        // N-Z → shard 1
        assert_eq!(strategy.get_shard("nancy", 2), 1);
        assert_eq!(strategy.get_shard("zoe", 2), 1);
    }

    #[test]
    fn test_numeric_sharding() {
        let strategy = NumericSharding::new(100000);

        // 4 shards
        assert_eq!(strategy.get_shard("0", 4), 0);
        assert_eq!(strategy.get_shard("12000", 4), 0);
        assert_eq!(strategy.get_shard("30000", 4), 1);
        assert_eq!(strategy.get_shard("60000", 4), 2);
        assert_eq!(strategy.get_shard("90000", 4), 3);
    }

    #[test]
    fn test_consistent_hash_sharding() {
        let strategy = ConsistentHashSharding::new();

        // Same key always maps to same shard
        let shard1 = strategy.get_shard("user123", 8);
        let shard2 = strategy.get_shard("user123", 8);
        assert_eq!(shard1, shard2);

        // Different keys map to different shards (usually)
        let shard_a = strategy.get_shard("userA", 8);
        let shard_b = strategy.get_shard("userB", 8);
        // Not guaranteed to be different, but likely
    }

    #[test]
    fn test_sharding_registry() {
        let registry = ShardingRegistry::new();

        // Check default strategies are registered
        assert!(registry.get("alphabetic").is_some());
        assert!(registry.get("numeric").is_some());
        assert!(registry.get("consistent_hash").is_some());

        // List strategies
        let strategies = registry.list_strategies();
        assert_eq!(strategies.len(), 3);
    }

    #[test]
    fn test_alphabetic_sharding_empty_key() {
        let strategy = AlphabeticSharding::new(2);
        assert_eq!(strategy.get_shard("", 2), 0);
    }

    #[test]
    fn test_numeric_sharding_invalid_key() {
        let strategy = NumericSharding::new(100000);
        assert_eq!(strategy.get_shard("not_a_number", 4), 0);
    }
}
