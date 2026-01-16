mod group_id;

use kalamdb_commons::models::{TableId, UserId};
use std::hash::{Hash, Hasher};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// Re-export GroupId and related types
pub use group_id::{GroupId, DEFAULT_USER_SHARDS, DEFAULT_SHARED_SHARDS};
// Re-export cluster config types for shared consumption
pub use kalamdb_configs::{ClusterConfig, PeerConfig};

/// Shard kind used across stream and data shards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ShardKind {
    User,
    Shared,
    Stream,
}

/// Shard identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Shard {
    kind: ShardKind,
    id: u32,
}

impl Shard {
    pub fn new(kind: ShardKind, id: u32) -> Self {
        Self { kind, id }
    }

    pub fn kind(&self) -> ShardKind {
        self.kind
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    /// Folder name used in storage paths.
    pub fn folder_name(&self) -> String {
        format!("shard_{}", self.id)
    }
}

/// Routes operations to the correct shard based on user_id or table_id.
#[derive(Debug, Clone)]
pub struct ShardRouter {
    num_user_shards: u32,
    num_shared_shards: u32,
}

impl ShardRouter {
    pub fn new(num_user_shards: u32, num_shared_shards: u32) -> Self {
        Self {
            num_user_shards,
            num_shared_shards,
        }
    }

    pub fn default_config() -> Self {
        Self::new(32, 1)
    }

    pub fn route_user(&self, user_id: &UserId) -> Shard {
        let shard = self.hash_to_shard(user_id.as_str(), self.num_user_shards.max(1));
        Shard::new(ShardKind::User, shard)
    }

    pub fn route_stream_user(&self, user_id: &UserId) -> Shard {
        let shard = self.hash_to_shard(user_id.as_str(), self.num_user_shards.max(1));
        Shard::new(ShardKind::Stream, shard)
    }

    pub fn route_shared(&self, _table_id: &TableId) -> Shard {
        let shard = if self.num_shared_shards == 0 { 0 } else { 0 };
        Shard::new(ShardKind::Shared, shard)
    }

    pub fn num_user_shards(&self) -> u32 {
        self.num_user_shards
    }

    pub fn num_shared_shards(&self) -> u32 {
        self.num_shared_shards
    }

    fn hash_to_shard(&self, key: &str, num_shards: u32) -> u32 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        (hash % num_shards as u64) as u32
    }
}
