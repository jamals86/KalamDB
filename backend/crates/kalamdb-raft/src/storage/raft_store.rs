//! Combined Raft Storage Implementation
//!
//! Implements the combined `RaftStorage` trait (v1 API) which includes:
//! - Log storage (RaftLogReader)
//! - State machine operations
//! - Snapshot building
//!
//! This avoids the sealed `RaftStateMachine` and `RaftLogStorage` v2 traits.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::ops::RangeBounds;
use std::sync::atomic::{AtomicU64, Ordering};

use openraft::{
    Entry, EntryPayload, LogId, OptionalSend, RaftSnapshotBuilder,
    SnapshotMeta, StorageError, StorageIOError, StoredMembership, Vote,
};
use openraft::storage::{LogState, RaftLogReader, RaftStorage, Snapshot};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::GroupId;
use crate::state_machine::{KalamStateMachine, encode, decode};
use crate::storage::types::{KalamNode, KalamTypeConfig};

/// Stored snapshot data
#[derive(Debug, Clone)]
pub struct StoredSnapshot {
    pub meta: SnapshotMeta<u64, KalamNode>,
    pub data: Vec<u8>,
}

/// Log entry stored in memory
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEntryData {
    log_id: LogId<u64>,
    payload: Vec<u8>,
}

/// State machine data that gets serialized to snapshots
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StateMachineData {
    last_applied_log: Option<LogId<u64>>,
    last_membership: StoredMembership<u64, KalamNode>,
    state: Vec<u8>,
}

/// Combined Raft storage implementing both log and state machine storage
///
/// This struct implements `RaftStorage` which combines `RaftLogReader`
/// with log management and state machine operations.
pub struct KalamRaftStorage<SM: KalamStateMachine + Send + Sync + 'static> {
    /// Which Raft group this storage belongs to
    group_id: GroupId,
    
    /// In-memory log entries (index -> entry)
    log: RwLock<BTreeMap<u64, LogEntryData>>,
    
    /// Current vote
    vote: RwLock<Option<Vote<u64>>>,
    
    /// Committed log id
    committed: RwLock<Option<LogId<u64>>>,
    
    /// Last purged log ID
    last_purged: RwLock<Option<LogId<u64>>>,
    
    /// The inner state machine (Arc since apply uses internal synchronization)
    state_machine: std::sync::Arc<SM>,
    
    /// Last applied log id
    last_applied: RwLock<Option<LogId<u64>>>,
    
    /// Last membership
    last_membership: RwLock<StoredMembership<u64, KalamNode>>,
    
    /// Snapshot counter
    snapshot_idx: AtomicU64,
    
    /// Current snapshot
    current_snapshot: RwLock<Option<StoredSnapshot>>,
}

impl<SM: KalamStateMachine + Send + Sync + 'static> Debug for KalamRaftStorage<SM> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KalamRaftStorage")
            .field("group_id", &self.group_id)
            .field("snapshot_idx", &self.snapshot_idx)
            .finish_non_exhaustive()
    }
}

impl<SM: KalamStateMachine + Send + Sync + 'static> KalamRaftStorage<SM> {
    /// Create a new combined Raft storage
    pub fn new(group_id: GroupId, state_machine: SM) -> Self {
        Self {
            group_id,
            log: RwLock::new(BTreeMap::new()),
            vote: RwLock::new(None),
            committed: RwLock::new(None),
            last_purged: RwLock::new(None),
            state_machine: std::sync::Arc::new(state_machine),
            last_applied: RwLock::new(None),
            last_membership: RwLock::new(StoredMembership::default()),
            snapshot_idx: AtomicU64::new(0),
            current_snapshot: RwLock::new(None),
        }
    }
    
    /// Get the partition name for persistence
    pub fn partition_name(&self) -> String {
        format!("raft_{}", self.group_id)
    }
    
    /// Get reference to the state machine
    pub fn state_machine(&self) -> &std::sync::Arc<SM> {
        &self.state_machine
    }
    
    /// Get the group ID
    pub fn group_id(&self) -> GroupId {
        self.group_id.clone()
    }
    
    /// Get log entries in a range (sync helper)
    fn get_log_entries_sync(&self, range: impl RangeBounds<u64>) -> Vec<Entry<KalamTypeConfig>> {
        let log = self.log.read();
        log.range(range)
            .filter_map(|(_, entry)| {
                match decode::<EntryPayload<KalamTypeConfig>>(&entry.payload) {
                    Ok(payload) => Some(Entry {
                        log_id: entry.log_id,
                        payload,
                    }),
                    Err(e) => {
                        log::warn!("Failed to decode log entry: {:?}", e);
                        Some(Entry {
                            log_id: entry.log_id,
                            payload: EntryPayload::Blank,
                        })
                    }
                }
            })
            .collect()
    }
}

/// Log reader implementation that shares access to the storage
pub struct KalamLogReader<SM: KalamStateMachine + Send + Sync + 'static> {
    /// Reference to the underlying storage
    storage: std::sync::Arc<KalamRaftStorage<SM>>,
}

impl<SM: KalamStateMachine + Send + Sync + 'static> Clone for KalamLogReader<SM> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

impl<SM: KalamStateMachine + Send + Sync + 'static> RaftLogReader<KalamTypeConfig> for KalamLogReader<SM> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<KalamTypeConfig>>, StorageError<u64>> {
        Ok(self.storage.get_log_entries_sync(range))
    }
}

/// Snapshot builder that can build snapshots from the state machine
pub struct KalamSnapshotBuilder<SM: KalamStateMachine + Send + Sync + 'static> {
    storage: std::sync::Arc<KalamRaftStorage<SM>>,
}

impl<SM: KalamStateMachine + Send + Sync + 'static> RaftSnapshotBuilder<KalamTypeConfig> for KalamSnapshotBuilder<SM> {
    async fn build_snapshot(&mut self) -> Result<Snapshot<KalamTypeConfig>, StorageError<u64>> {
        let last_applied = self.storage.last_applied.read().clone();
        let last_membership = self.storage.last_membership.read().clone();
        
        // For now, we only serialize the metadata (last_applied, membership).
        // The actual state machine state can be rebuilt by replaying logs.
        // A full implementation would serialize the state machine state here.
        let data = StateMachineData {
            last_applied_log: last_applied,
            last_membership: last_membership.clone(),
            state: Vec::new(), // State machine state would go here
        };
        
        let serialized = encode(&data).map_err(|e| StorageIOError::read_state_machine(&e))?;
        
        let snapshot_idx = self.storage.snapshot_idx.fetch_add(1, Ordering::Relaxed) + 1;
        let snapshot_id = if let Some(last) = last_applied {
            format!("{}-{}-{}", last.leader_id, last.index, snapshot_idx)
        } else {
            format!("--{}", snapshot_idx)
        };
        
        let meta = SnapshotMeta {
            last_log_id: last_applied,
            last_membership,
            snapshot_id,
        };
        
        {
            let mut current = self.storage.current_snapshot.write();
            *current = Some(StoredSnapshot {
                meta: meta.clone(),
                data: serialized.clone(),
            });
        }
        
        Ok(Snapshot {
            meta,
            snapshot: Box::new(Cursor::new(serialized)),
        })
    }
}

// Implement RaftLogReader for the storage itself (required by RaftStorage)
impl<SM: KalamStateMachine + Send + Sync + 'static> RaftLogReader<KalamTypeConfig> for KalamRaftStorage<SM> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<KalamTypeConfig>>, StorageError<u64>> {
        Ok(self.get_log_entries_sync(range))
    }
}

// Implement the combined RaftStorage trait
#[allow(deprecated)] // RaftStorage is deprecated in favor of v2 traits, but v2 is sealed
impl<SM: KalamStateMachine + Send + Sync + 'static> RaftStorage<KalamTypeConfig> for std::sync::Arc<KalamRaftStorage<SM>> {
    type LogReader = KalamLogReader<SM>;
    type SnapshotBuilder = KalamSnapshotBuilder<SM>;

    // --- Vote operations ---
    
    async fn save_vote(&mut self, vote: &Vote<u64>) -> Result<(), StorageError<u64>> {
        let mut current = self.vote.write();
        *current = Some(vote.clone());
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<u64>>, StorageError<u64>> {
        Ok(self.vote.read().clone())
    }

    async fn save_committed(&mut self, committed: Option<LogId<u64>>) -> Result<(), StorageError<u64>> {
        let mut c = self.committed.write();
        *c = committed;
        Ok(())
    }

    async fn read_committed(&mut self) -> Result<Option<LogId<u64>>, StorageError<u64>> {
        Ok(self.committed.read().clone())
    }

    // --- Log operations ---

    async fn get_log_state(&mut self) -> Result<LogState<KalamTypeConfig>, StorageError<u64>> {
        let log = self.log.read();
        let last_purged = self.last_purged.read().clone();
        let last_log_id = log.iter().next_back().map(|(_, e)| e.log_id);
        
        Ok(LogState {
            last_purged_log_id: last_purged,
            last_log_id,
        })
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        KalamLogReader {
            storage: self.clone(),
        }
    }

    async fn append_to_log<I>(&mut self, entries: I) -> Result<(), StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<KalamTypeConfig>> + OptionalSend,
    {
        let mut log = self.log.write();
        
        for entry in entries {
            let payload = encode(&entry.payload)
                .map_err(|e| StorageIOError::write_logs(&e))?;
            
            log.insert(entry.log_id.index, LogEntryData {
                log_id: entry.log_id,
                payload,
            });
        }
        
        Ok(())
    }

    async fn delete_conflict_logs_since(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        let mut log = self.log.write();
        
        let keys_to_remove: Vec<u64> = log.range(log_id.index..).map(|(k, _)| *k).collect();
        for key in keys_to_remove {
            log.remove(&key);
        }
        
        Ok(())
    }

    async fn purge_logs_upto(&mut self, log_id: LogId<u64>) -> Result<(), StorageError<u64>> {
        let mut log = self.log.write();
        let mut last_purged = self.last_purged.write();
        
        let keys_to_remove: Vec<u64> = log.range(..=log_id.index).map(|(k, _)| *k).collect();
        for key in keys_to_remove {
            log.remove(&key);
        }
        
        *last_purged = Some(log_id);
        Ok(())
    }

    // --- State Machine operations ---

    async fn last_applied_state(
        &mut self,
    ) -> Result<(Option<LogId<u64>>, StoredMembership<u64, KalamNode>), StorageError<u64>> {
        let last_applied = self.last_applied.read().clone();
        let last_membership = self.last_membership.read().clone();
        Ok((last_applied, last_membership))
    }

    async fn apply_to_state_machine(
        &mut self,
        entries: &[Entry<KalamTypeConfig>],
    ) -> Result<Vec<Vec<u8>>, StorageError<u64>> {
        let mut results = Vec::new();
        
        for entry in entries {
            let log_id = entry.log_id;
            let index = log_id.index;
            let term = log_id.leader_id.term;
            
            // Update last applied
            {
                let mut last = self.last_applied.write();
                *last = Some(log_id);
            }
            
            match &entry.payload {
                EntryPayload::Blank => {
                    results.push(Vec::new());
                }
                EntryPayload::Normal(data) => {
                    // Apply command to state machine
                    // Since state_machine is Arc<SM> and SM uses internal synchronization,
                    // we can safely call apply() without holding any lock across await
                    let sm = self.state_machine.clone();
                    match sm.apply(index, term, data).await {
                        Ok(apply_result) => {
                            match apply_result {
                                crate::state_machine::ApplyResult::Ok(response_data) => {
                                    results.push(response_data);
                                }
                                crate::state_machine::ApplyResult::NoOp => {
                                    results.push(Vec::new());
                                }
                                crate::state_machine::ApplyResult::Error(e) => {
                                    log::error!("State machine apply error at index {}: {}", index, e);
                                    results.push(Vec::new());
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("State machine apply failed at index {}: {:?}", index, e);
                            results.push(Vec::new());
                        }
                    }
                }
                EntryPayload::Membership(mem) => {
                    let mut membership = self.last_membership.write();
                    *membership = StoredMembership::new(Some(log_id), mem.clone());
                    results.push(Vec::new());
                }
            }
        }
        
        Ok(results)
    }

    // --- Snapshot operations ---

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        KalamSnapshotBuilder {
            storage: self.clone(),
        }
    }

    async fn begin_receiving_snapshot(&mut self) -> Result<Box<Cursor<Vec<u8>>>, StorageError<u64>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<u64, KalamNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<u64>> {
        let data = snapshot.into_inner();
        
        // Deserialize
        let _sm_data: StateMachineData = decode(&data)
            .map_err(|e| StorageIOError::read_snapshot(Some(meta.signature()), &e))?;
        
        // Update metadata
        {
            let mut last = self.last_applied.write();
            *last = meta.last_log_id;
        }
        {
            let mut membership = self.last_membership.write();
            *membership = meta.last_membership.clone();
        }
        
        // Store snapshot
        {
            let mut current = self.current_snapshot.write();
            *current = Some(StoredSnapshot {
                meta: meta.clone(),
                data,
            });
        }
        
        // Clear logs up to the snapshot point
        if let Some(last_log_id) = meta.last_log_id {
            let mut log = self.log.write();
            let mut last_purged = self.last_purged.write();
            
            let keys_to_remove: Vec<u64> = log.range(..=last_log_id.index).map(|(k, _)| *k).collect();
            for key in keys_to_remove {
                log.remove(&key);
            }
            *last_purged = Some(last_log_id);
        }
        
        Ok(())
    }

    async fn get_current_snapshot(&mut self) -> Result<Option<Snapshot<KalamTypeConfig>>, StorageError<u64>> {
        let current = self.current_snapshot.read();
        match current.as_ref() {
            Some(snapshot) => Ok(Some(Snapshot {
                meta: snapshot.meta.clone(),
                snapshot: Box::new(Cursor::new(snapshot.data.clone())),
            })),
            None => Ok(None),
        }
    }
}

// Also implement RaftLogReader for Arc<KalamRaftStorage<SM>>
impl<SM: KalamStateMachine + Send + Sync + 'static> RaftLogReader<KalamTypeConfig> for std::sync::Arc<KalamRaftStorage<SM>> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<KalamTypeConfig>>, StorageError<u64>> {
        Ok(self.get_log_entries_sync(range))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::SystemStateMachine;

    #[tokio::test]
    async fn test_storage_creation() {
        let sm = SystemStateMachine::new();
        let storage = std::sync::Arc::new(KalamRaftStorage::new(GroupId::MetaSystem, sm));
        
        let mut storage_clone = storage.clone();
        let (last_applied, _) = storage_clone.last_applied_state().await.unwrap();
        assert!(last_applied.is_none());
    }

    #[tokio::test]
    async fn test_vote_operations() {
        let sm = SystemStateMachine::new();
        let mut storage = std::sync::Arc::new(KalamRaftStorage::new(GroupId::MetaSystem, sm));
        
        assert!(storage.read_vote().await.unwrap().is_none());
        
        let vote = Vote::new(1, 1);
        storage.save_vote(&vote).await.unwrap();
        
        assert_eq!(storage.read_vote().await.unwrap(), Some(vote));
    }

    #[tokio::test]
    async fn test_log_operations() {
        let sm = SystemStateMachine::new();
        let mut storage = std::sync::Arc::new(KalamRaftStorage::new(GroupId::MetaSystem, sm));
        
        let state = storage.get_log_state().await.unwrap();
        assert!(state.last_log_id.is_none());
        
        // Append entries
        let entry = Entry {
            log_id: LogId::new(openraft::CommittedLeaderId::new(1, 1), 1),
            payload: EntryPayload::Blank,
        };
        storage.append_to_log(vec![entry]).await.unwrap();
        
        let state = storage.get_log_state().await.unwrap();
        assert!(state.last_log_id.is_some());
    }
}
