//! Raft Group ID definitions
//!
//! KalamDB uses Multi-Raft with 36 groups:
//! - 3 metadata groups (system, users, jobs)
//! - 32 user data shards (user tables + live_queries)
//! - 1 shared data shard (shared tables)

use serde::{Deserialize, Serialize};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Default number of user data shards
pub const DEFAULT_USER_SHARDS: u32 = 32;

/// Default number of shared data shards (future: increase for scale)
pub const DEFAULT_SHARED_SHARDS: u32 = 1;

/// Identifies a Raft group in the Multi-Raft architecture.
///
/// Each group has its own Raft instance with independent leader election.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GroupId {
    // === Metadata Groups (3) ===
    /// System metadata: namespaces, tables, storages
    MetaSystem,
    /// User accounts and authentication
    MetaUsers,
    /// Background jobs coordination
    MetaJobs,

    // === Data Groups (33) ===
    /// User table data shard (0..31)
    /// Routes by: user_id % num_user_shards
    DataUserShard(u32),
    /// Shared table data shard (0 for Phase 1)
    /// Future: shard by table_id or row key
    DataSharedShard(u32),
}

impl GroupId {
    /// Total number of metadata groups
    pub const METADATA_GROUP_COUNT: usize = 3;

    /// Returns true if this is a metadata group
    pub fn is_metadata(&self) -> bool {
        matches!(self, GroupId::MetaSystem | GroupId::MetaUsers | GroupId::MetaJobs)
    }

    /// Returns true if this is a data group
    pub fn is_data(&self) -> bool {
        matches!(self, GroupId::DataUserShard(_) | GroupId::DataSharedShard(_))
    }

    /// Returns the partition prefix for RocksDB column family naming
    pub fn partition_prefix(&self) -> String {
        match self {
            GroupId::MetaSystem => "raft_meta_system".to_string(),
            GroupId::MetaUsers => "raft_meta_users".to_string(),
            GroupId::MetaJobs => "raft_meta_jobs".to_string(),
            GroupId::DataUserShard(id) => format!("raft_data_user_{:02}", id),
            GroupId::DataSharedShard(id) => format!("raft_data_shared_{:02}", id),
        }
    }

    /// Returns a numeric ID for openraft (must be unique across all groups)
    pub fn as_u64(&self) -> u64 {
        match self {
            GroupId::MetaSystem => 0,
            GroupId::MetaUsers => 1,
            GroupId::MetaJobs => 2,
            GroupId::DataUserShard(id) => 100 + (*id as u64),
            GroupId::DataSharedShard(id) => 200 + (*id as u64),
        }
    }

    /// Create from numeric ID
    pub fn from_u64(id: u64) -> Option<Self> {
        match id {
            0 => Some(GroupId::MetaSystem),
            1 => Some(GroupId::MetaUsers),
            2 => Some(GroupId::MetaJobs),
            100..=131 => Some(GroupId::DataUserShard((id - 100) as u32)),
            200..=231 => Some(GroupId::DataSharedShard((id - 200) as u32)),
            _ => None,
        }
    }

    /// Returns all metadata group IDs
    pub fn all_metadata() -> Vec<GroupId> {
        vec![GroupId::MetaSystem, GroupId::MetaUsers, GroupId::MetaJobs]
    }

    /// Returns all user data shard IDs for given shard count
    pub fn all_user_shards(num_shards: u32) -> Vec<GroupId> {
        (0..num_shards).map(GroupId::DataUserShard).collect()
    }

    /// Returns all shared data shard IDs for given shard count
    pub fn all_shared_shards(num_shards: u32) -> Vec<GroupId> {
        (0..num_shards).map(GroupId::DataSharedShard).collect()
    }

    /// Returns all group IDs for given configuration
    pub fn all_groups(num_user_shards: u32, num_shared_shards: u32) -> Vec<GroupId> {
        let mut groups = Self::all_metadata();
        groups.extend(Self::all_user_shards(num_user_shards));
        groups.extend(Self::all_shared_shards(num_shared_shards));
        groups
    }
}

impl fmt::Display for GroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupId::MetaSystem => write!(f, "meta:system"),
            GroupId::MetaUsers => write!(f, "meta:users"),
            GroupId::MetaJobs => write!(f, "meta:jobs"),
            GroupId::DataUserShard(id) => write!(f, "data:user:{:02}", id),
            GroupId::DataSharedShard(id) => write!(f, "data:shared:{:02}", id),
        }
    }
}

/// Routes operations to the correct shard based on user_id or table_id.
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
    num_user_shards: u32,
    num_shared_shards: u32,
}

impl ShardRouter {
    /// Create a new shard router with the given configuration
    pub fn new(num_user_shards: u32, num_shared_shards: u32) -> Self {
        Self {
            num_user_shards,
            num_shared_shards,
        }
    }

    /// Create with default shard counts (32 user, 1 shared)
    pub fn default_config() -> Self {
        Self::new(DEFAULT_USER_SHARDS, DEFAULT_SHARED_SHARDS)
    }

    /// Route a user operation to the appropriate shard
    ///
    /// Uses a simple hash of the user_id to determine shard assignment.
    pub fn route_user(&self, user_id: &str) -> GroupId {
        let shard = self.hash_to_shard(user_id, self.num_user_shards);
        GroupId::DataUserShard(shard)
    }

    /// Route a shared table operation to the appropriate shard
    ///
    /// Phase 1: All shared tables go to shard 0
    /// Future: Could shard by table_id or row key
    pub fn route_shared(&self, _table_id: &str) -> GroupId {
        // Phase 1: Single shared shard
        GroupId::DataSharedShard(0)
    }

    /// Get all group IDs this router manages
    pub fn all_groups(&self) -> Vec<GroupId> {
        GroupId::all_groups(self.num_user_shards, self.num_shared_shards)
    }

    /// Number of user shards
    pub fn num_user_shards(&self) -> u32 {
        self.num_user_shards
    }

    /// Number of shared shards
    pub fn num_shared_shards(&self) -> u32 {
        self.num_shared_shards
    }

    /// Total number of groups (metadata + data)
    pub fn total_groups(&self) -> usize {
        GroupId::METADATA_GROUP_COUNT 
            + self.num_user_shards as usize 
            + self.num_shared_shards as usize
    }

    /// Hash a string key to a shard index
    fn hash_to_shard(&self, key: &str, num_shards: u32) -> u32 {
        // Use a simple but consistent hash
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        (hash % num_shards as u64) as u32
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
        assert_eq!(GroupId::MetaSystem.to_string(), "meta:system");
        assert_eq!(GroupId::MetaUsers.to_string(), "meta:users");
        assert_eq!(GroupId::MetaJobs.to_string(), "meta:jobs");
        assert_eq!(GroupId::DataUserShard(5).to_string(), "data:user:05");
        assert_eq!(GroupId::DataSharedShard(0).to_string(), "data:shared:00");
    }

    #[test]
    fn test_group_id_roundtrip() {
        for id in [0u64, 1, 2, 100, 115, 131, 200] {
            let group = GroupId::from_u64(id).unwrap();
            assert_eq!(group.as_u64(), id);
        }
    }

    #[test]
    fn test_all_groups_count() {
        let groups = GroupId::all_groups(32, 1);
        assert_eq!(groups.len(), 36); // 3 metadata + 32 user + 1 shared
    }

    #[test]
    fn test_shard_router_consistency() {
        let router = ShardRouter::default_config();
        
        // Same user_id should always route to same shard
        let shard1 = router.route_user("user_123");
        let shard2 = router.route_user("user_123");
        assert_eq!(shard1, shard2);
    }

    #[test]
    fn test_shard_router_distribution() {
        let router = ShardRouter::default_config();
        
        // Different users should distribute across shards
        let mut shards = std::collections::HashSet::new();
        for i in 0..100 {
            let user_id = format!("user_{}", i);
            if let GroupId::DataUserShard(shard) = router.route_user(&user_id) {
                shards.insert(shard);
            }
        }
        
        // With 100 users and 32 shards, we should hit multiple shards
        assert!(shards.len() > 10, "Expected better distribution");
    }

    #[test]
    fn test_partition_prefix() {
        assert_eq!(GroupId::MetaSystem.partition_prefix(), "raft_meta_system");
        assert_eq!(GroupId::DataUserShard(5).partition_prefix(), "raft_data_user_05");
        assert_eq!(GroupId::DataSharedShard(0).partition_prefix(), "raft_data_shared_00");
    }
}
