//! Raft Group ID and ShardRouter re-exports from kalamdb-sharding.
//!
//! This module re-exports the core sharding types from `kalamdb-sharding`
//! and provides a Raft-specific `ShardRouter` wrapper that returns `GroupId`.
//!
//! KalamDB uses Multi-Raft with 34 groups (post-018 consolidation):
//! - 1 unified metadata group (Meta)
//! - 32 user data shards (user tables + live_queries)
//! - 1 shared data shard (shared tables)

use kalamdb_commons::models::UserId;
use kalamdb_commons::TableId;
use kalamdb_sharding::ShardRouter as CoreShardRouter;

// Re-export GroupId and constants from kalamdb-sharding
pub use kalamdb_sharding::{GroupId, DEFAULT_USER_SHARDS, DEFAULT_SHARED_SHARDS};

/// Routes operations to the correct Raft group based on user_id or table_id.
///
/// This is a Raft-specific wrapper around `kalamdb_sharding::ShardRouter` that
/// returns `GroupId` instead of `Shard` for Raft group assignment.
///
/// # User Data Sharding
///
/// User table operations are sharded by user_id:
/// ```text
/// shard_index = hash(user_id) % num_user_shards
/// ```
///
/// This ensures all data for a single user lives in one shard, enabling:
/// - Efficient user-scoped queries
/// - Live query subscriptions in the same shard as user data
/// - Atomic operations on a user's data
#[derive(Debug, Clone)]
pub struct ShardRouter {
    inner: CoreShardRouter,
}

impl ShardRouter {
    /// Create a new shard router with the given configuration
    pub fn new(num_user_shards: u32, num_shared_shards: u32) -> Self {
        Self {
            inner: CoreShardRouter::new(num_user_shards, num_shared_shards),
        }
    }

    /// Create with default shard counts (32 user, 1 shared)
    pub fn default_config() -> Self {
        Self {
            inner: CoreShardRouter::default_config(),
        }
    }

    /// Route a user operation to the appropriate Raft group
    ///
    /// Uses a simple hash of the user_id to determine shard assignment.
    pub fn route_user(&self, user_id: &UserId) -> GroupId {
        let shard = self.inner.route_user(user_id);
        GroupId::DataUserShard(shard.id())
    }

    /// Route a shared table operation to the appropriate Raft group
    ///
    /// Phase 1: All shared tables go to shard 0
    /// Future: Could shard by table_id or row key
    pub fn route_shared(&self, table_id: &TableId) -> GroupId {
        let shard = self.inner.route_shared(table_id);
        GroupId::DataSharedShard(shard.id())
    }

    /// Get all group IDs this router manages
    pub fn all_groups(&self) -> Vec<GroupId> {
        GroupId::all_groups(self.inner.num_user_shards(), self.inner.num_shared_shards())
    }

    /// Number of user shards
    pub fn num_user_shards(&self) -> u32 {
        self.inner.num_user_shards()
    }

    /// Number of shared shards
    pub fn num_shared_shards(&self) -> u32 {
        self.inner.num_shared_shards()
    }

    /// Total number of groups (metadata + data)
    pub fn total_groups(&self) -> usize {
        GroupId::METADATA_GROUP_COUNT
            + self.num_user_shards() as usize
            + self.num_shared_shards() as usize
    }
}

impl Default for ShardRouter {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_id_display() {
        assert_eq!(GroupId::Meta.to_string(), "meta");
        assert_eq!(GroupId::DataUserShard(5).to_string(), "data:user:05");
        assert_eq!(GroupId::DataSharedShard(0).to_string(), "data:shared:00");
    }

    #[test]
    fn test_group_id_roundtrip() {
        for id in [10u64, 100, 115, 131, 200] {
            let group = GroupId::from_u64(id).unwrap();
            assert_eq!(group.as_u64(), id);
        }
    }

    #[test]
    fn test_all_groups_count() {
        let groups = GroupId::all_groups(32, 1);
        // 1 metadata (Meta) + 32 user + 1 shared = 34
        assert_eq!(groups.len(), 34);
    }

    #[test]
    fn test_meta_group() {
        assert!(GroupId::Meta.is_metadata());
        assert!(!GroupId::Meta.is_data());
    }

    #[test]
    fn test_shard_router_consistency() {
        let router = ShardRouter::default_config();

        // Same user_id should always route to same shard
        let user_id = UserId::new("user_123".to_string());
        let shard1 = router.route_user(&user_id);
        let shard2 = router.route_user(&user_id);
        assert_eq!(shard1, shard2);
    }

    #[test]
    fn test_shard_router_distribution() {
        let router = ShardRouter::default_config();

        // Different users should distribute across shards
        let mut shards = std::collections::HashSet::new();
        for i in 0..100 {
            let user_id = UserId::new(format!("user_{}", i));
            if let GroupId::DataUserShard(shard) = router.route_user(&user_id) {
                shards.insert(shard);
            }
        }

        // With 100 users and 32 shards, we should hit multiple shards
        assert!(shards.len() > 10, "Expected better distribution");
    }

    #[test]
    fn test_partition_prefix() {
        assert_eq!(GroupId::Meta.partition_prefix(), "raft_meta");
        assert_eq!(
            GroupId::DataUserShard(5).partition_prefix(),
            "raft_data_user_05"
        );
        assert_eq!(
            GroupId::DataSharedShard(0).partition_prefix(),
            "raft_data_shared_00"
        );
    }
}
