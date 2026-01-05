//! Raft Log Storage
//!
//! Implements `openraft::storage::RaftLogReader` for log access.
//! The LogStore is used together with the StateMachineStore via the Adaptor pattern.
//!
//! Each Raft group has its own partition: `raft_log_{group_id}`

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;

use openraft::{
    Entry, LogId, OptionalSend,
    RaftTypeConfig, Vote,
};
use openraft::storage::{LogState, RaftLogReader};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::GroupId;
use crate::state_machine::{encode, decode};

/// Type configuration for KalamDB Raft
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd)]
#[derive(Serialize, Deserialize)]
pub struct KalamTypeConfig;

impl RaftTypeConfig for KalamTypeConfig {
    type D = Vec<u8>;           // Log entry data (serialized command)
    type R = Vec<u8>;           // Response data (serialized response)
    type NodeId = u64;
    type Node = KalamNode;
    type Entry = Entry<Self>;
    type SnapshotData = Cursor<Vec<u8>>;
    type AsyncRuntime = openraft::TokioRuntime;
    type Responder = openraft::impls::OneshotResponder<Self>;
}

/// Node information for cluster membership
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KalamNode {
    /// gRPC address for Raft communication
    pub rpc_addr: String,
    /// HTTP address for client requests
    pub api_addr: String,
}

impl std::fmt::Display for KalamNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}|{}", self.rpc_addr, self.api_addr)
    }
}

impl std::error::Error for KalamNode {}

/// In-memory log entry for a single Raft group
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntryData {
    log_id: LogId<u64>,
    payload: Vec<u8>,
}

/// Log storage for a single Raft group
///
/// Uses in-memory storage with periodic persistence via StorageBackend.
/// For production, this should be backed by RocksDB.
#[derive(Debug)]
pub struct LogStore {
    /// Which Raft group this log store belongs to
    group_id: GroupId,
    
    /// In-memory log entries (index -> entry)
    /// BTreeMap maintains ordering for range queries
    log: RwLock<BTreeMap<u64, LogEntryData>>,
    
    /// Current vote (term, voted_for)
    vote: RwLock<Option<Vote<u64>>>,
    
    /// Committed index (for truncation reference)
    committed: RwLock<Option<LogId<u64>>>,
    
    /// Last purged log ID (entries before this have been removed)
    last_purged: RwLock<Option<LogId<u64>>>,
}

impl LogStore {
    /// Create a new log store for the specified Raft group
    pub fn new(group_id: GroupId) -> Self {
        Self {
            group_id,
            log: RwLock::new(BTreeMap::new()),
            vote: RwLock::new(None),
            committed: RwLock::new(None),
            last_purged: RwLock::new(None),
        }
    }
    
    /// Get the partition name for this log store
    pub fn partition_name(&self) -> String {
        format!("raft_log_{}", self.group_id)
    }
    
    /// Get the log state
    pub fn get_log_state_sync(&self) -> LogState<KalamTypeConfig> {
        let log = self.log.read();
        let last_purged = self.last_purged.read().clone();
        let last_log_id = log.iter().next_back().map(|(_, e)| e.log_id);
        
        LogState {
            last_purged_log_id: last_purged,
            last_log_id,
        }
    }
    
    /// Save vote
    pub fn save_vote_sync(&self, vote: &Vote<u64>) {
        let mut current = self.vote.write();
        *current = Some(vote.clone());
    }
    
    /// Read vote
    pub fn read_vote_sync(&self) -> Option<Vote<u64>> {
        self.vote.read().clone()
    }
    
    /// Append log entries
    pub fn append_sync(&self, entries: impl IntoIterator<Item = Entry<KalamTypeConfig>>) -> Result<(), std::io::Error> {
        let mut log = self.log.write();
        
        for entry in entries {
            let payload = encode(&entry.payload)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            
            log.insert(entry.log_id.index, LogEntryData {
                log_id: entry.log_id,
                payload,
            });
        }
        
        Ok(())
    }
    
    /// Truncate logs from a specific log_id
    pub fn truncate_sync(&self, log_id: LogId<u64>) {
        let mut log = self.log.write();
        
        // Remove all entries with index >= log_id.index
        let keys_to_remove: Vec<u64> = log.range(log_id.index..).map(|(k, _)| *k).collect();
        for key in keys_to_remove {
            log.remove(&key);
        }
    }
    
    /// Purge logs up to a specific log_id
    pub fn purge_sync(&self, log_id: LogId<u64>) {
        let mut log = self.log.write();
        let mut last_purged = self.last_purged.write();
        
        // Remove all entries with index <= log_id.index
        let keys_to_remove: Vec<u64> = log.range(..=log_id.index).map(|(k, _)| *k).collect();
        for key in keys_to_remove {
            log.remove(&key);
        }
        
        *last_purged = Some(log_id);
    }
    
    /// Get log entries in a range
    pub fn get_log_entries(&self, range: impl RangeBounds<u64>) -> Vec<Entry<KalamTypeConfig>> {
        let log = self.log.read();
        log.range(range)
            .map(|(_, entry)| {
                // Deserialize the entry payload
                match decode::<openraft::EntryPayload<KalamTypeConfig>>(&entry.payload) {
                    Ok(payload) => Entry {
                        log_id: entry.log_id,
                        payload,
                    },
                    Err(_) => {
                        // Fallback to blank entry on decode error
                        Entry {
                            log_id: entry.log_id,
                            payload: openraft::EntryPayload::Blank,
                        }
                    }
                }
            })
            .collect()
    }
    
    /// Save committed log id
    pub fn save_committed_sync(&self, committed: Option<LogId<u64>>) {
        let mut c = self.committed.write();
        *c = committed;
    }
    
    /// Read committed log id
    pub fn read_committed_sync(&self) -> Option<LogId<u64>> {
        self.committed.read().clone()
    }
}

impl RaftLogReader<KalamTypeConfig> for LogStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<KalamTypeConfig>>, openraft::StorageError<u64>> {
        Ok(self.get_log_entries(range))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_store_basic() {
        let store = LogStore::new(GroupId::MetaSystem);
        
        // Check initial state
        let state = store.get_log_state_sync();
        assert!(state.last_log_id.is_none());
        assert!(state.last_purged_log_id.is_none());
    }

    #[test]
    fn test_partition_name() {
        let store = LogStore::new(GroupId::MetaSystem);
        assert_eq!(store.partition_name(), "raft_log_meta:system");
        
        let store2 = LogStore::new(GroupId::DataUserShard(5));
        assert_eq!(store2.partition_name(), "raft_log_data:user:05");
    }
    
    #[test]
    fn test_vote_operations() {
        let store = LogStore::new(GroupId::MetaSystem);
        
        assert!(store.read_vote_sync().is_none());
        
        let vote = Vote::new(1, 1);
        store.save_vote_sync(&vote);
        
        assert_eq!(store.read_vote_sync(), Some(vote));
    }
}
