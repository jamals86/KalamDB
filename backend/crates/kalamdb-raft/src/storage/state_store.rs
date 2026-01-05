//! Raft State Machine Storage
//!
//! Implements `openraft::storage::RaftStateMachine` wrapping our KalamStateMachine trait.
//!
//! This is used together with LogStore via openraft's Adaptor pattern.

use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use openraft::{
    Entry, EntryPayload, LogId, RaftSnapshotBuilder,
    SnapshotMeta, StorageError, StorageIOError, StoredMembership,
};
use openraft::storage::{RaftStateMachine, Snapshot};
use parking_lot::RwLock as ParkingLotRwLock;
use tokio::sync::RwLock as TokioRwLock;
use serde::{Deserialize, Serialize};

use crate::storage::log_store::{KalamTypeConfig, KalamNode};
use crate::state_machine::{KalamStateMachine, encode, decode};

/// Stored snapshot data
#[derive(Debug)]
pub struct StoredSnapshot {
    pub meta: SnapshotMeta<u64, KalamNode>,
    pub data: Vec<u8>,
}

/// State machine data that gets serialized to snapshots
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StateMachineData {
    /// Last applied log id
    last_applied_log: Option<LogId<u64>>,
    
    /// Last membership configuration
    last_membership: StoredMembership<u64, KalamNode>,
    
    /// The actual state machine state (serialized)
    state: Vec<u8>,
}

/// State machine storage wrapper
///
/// Wraps a KalamStateMachine and provides the openraft RaftStateMachine interface.
/// Uses tokio::sync::RwLock for the inner state machine to allow holding across await.
pub struct StateMachineStore<SM: KalamStateMachine + Send + Sync + 'static> {
    /// The inner state machine (uses tokio RwLock for async compatibility)
    inner: TokioRwLock<SM>,
    
    /// Last applied log id
    last_applied: ParkingLotRwLock<Option<LogId<u64>>>,
    
    /// Last membership
    last_membership: ParkingLotRwLock<StoredMembership<u64, KalamNode>>,
    
    /// Snapshot counter (for generating unique snapshot IDs)
    snapshot_idx: AtomicU64,
    
    /// Current snapshot
    current_snapshot: ParkingLotRwLock<Option<StoredSnapshot>>,
}

impl<SM: KalamStateMachine + Send + Sync + 'static> std::fmt::Debug for StateMachineStore<SM> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateMachineStore")
            .field("snapshot_idx", &self.snapshot_idx)
            .finish_non_exhaustive()
    }
}

impl<SM: KalamStateMachine + Send + Sync + 'static> StateMachineStore<SM> {
    /// Create a new state machine store
    pub fn new(inner: SM) -> Self {
        Self {
            inner: TokioRwLock::new(inner),
            last_applied: ParkingLotRwLock::new(None),
            last_membership: ParkingLotRwLock::new(StoredMembership::default()),
            snapshot_idx: AtomicU64::new(0),
            current_snapshot: ParkingLotRwLock::new(None),
        }
    }
    
    /// Get a reference to the inner state machine
    pub fn inner(&self) -> &TokioRwLock<SM> {
        &self.inner
    }
}

impl<SM: KalamStateMachine + Send + Sync + 'static> RaftSnapshotBuilder<KalamTypeConfig> for Arc<StateMachineStore<SM>> {
    async fn build_snapshot(&mut self) -> Result<Snapshot<KalamTypeConfig>, StorageError<u64>> {
        // Get current state
        let last_applied = self.last_applied.read().clone();
        let last_membership = self.last_membership.read().clone();
        
        // Serialize the state machine - snapshot() is async, use tokio RwLock
        let state = {
            let sm = self.inner.read().await;
            let snapshot_result = sm.snapshot().await
                .map_err(|e| StorageIOError::read_state_machine(&e))?;
            encode(&snapshot_result.data).map_err(|e| StorageIOError::read_state_machine(&e))?
        };
        
        let data = StateMachineData {
            last_applied_log: last_applied,
            last_membership: last_membership.clone(),
            state,
        };
        
        let serialized = encode(&data).map_err(|e| StorageIOError::read_state_machine(&e))?;
        
        // Generate snapshot ID
        let snapshot_idx = self.snapshot_idx.fetch_add(1, Ordering::Relaxed) + 1;
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
        
        // Store the snapshot
        {
            let mut current = self.current_snapshot.write();
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

impl<SM: KalamStateMachine + Send + Sync + 'static> RaftStateMachine<KalamTypeConfig> for Arc<StateMachineStore<SM>> {
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<u64>>, StoredMembership<u64, KalamNode>), StorageError<u64>> {
        let last_applied = self.last_applied.read().clone();
        let last_membership = self.last_membership.read().clone();
        Ok((last_applied, last_membership))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<Vec<u8>>, StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<KalamTypeConfig>> + Send,
    {
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
            
            match entry.payload {
                EntryPayload::Blank => {
                    results.push(Vec::new());
                }
                EntryPayload::Normal(ref data) => {
                    // Apply the command to the state machine
                    // Note: apply() is async, use tokio RwLock
                    let result = {
                        let sm = self.inner.read().await;
                        sm.apply(index, term, data).await
                    };
                    
                    match result {
                        Ok(response) => {
                            let encoded = encode(&response)
                                .map_err(|e| StorageIOError::write_state_machine(&e))?;
                            results.push(encoded);
                        }
                        Err(e) => {
                            // Log error but continue (state machine errors shouldn't stop Raft)
                            log::error!("State machine apply error: {:?}", e);
                            results.push(Vec::new());
                        }
                    }
                }
                EntryPayload::Membership(ref mem) => {
                    let mut membership = self.last_membership.write();
                    *membership = StoredMembership::new(Some(log_id), mem.clone());
                    results.push(Vec::new());
                }
            }
        }
        
        Ok(results)
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<u64>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<u64, KalamNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<u64>> {
        let data = snapshot.into_inner();
        
        // Deserialize the state machine data
        let sm_data: StateMachineData = decode(&data)
            .map_err(|e| StorageIOError::read_snapshot(Some(meta.signature()), &e))?;
        
        // Restore the state machine - restore() is async, use tokio RwLock
        {
            let sm = self.inner.read().await;
            let snapshot_data: Vec<u8> = decode(&sm_data.state)
                .map_err(|e| StorageIOError::read_snapshot(Some(meta.signature()), &e))?;
            
            // Get index and term from the log_id
            let (index, term) = meta.last_log_id
                .map(|id| (id.index, id.leader_id.term))
                .unwrap_or((0, 0));
            
            let sm_snapshot = crate::state_machine::StateMachineSnapshot {
                group_id: crate::GroupId::MetaSystem, // We don't store group_id in snapshot data, default to MetaSystem
                last_applied_index: index,
                last_applied_term: term,
                data: snapshot_data,
            };
            
            sm.restore(sm_snapshot).await
                .map_err(|e| StorageIOError::read_snapshot(Some(meta.signature()), &e))?;
        }
        
        // Update metadata
        {
            let mut last = self.last_applied.write();
            *last = meta.last_log_id;
        }
        {
            let mut membership = self.last_membership.write();
            *membership = meta.last_membership.clone();
        }
        
        // Store the snapshot
        {
            let mut current = self.current_snapshot.write();
            *current = Some(StoredSnapshot {
                meta: meta.clone(),
                data,
            });
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

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::SystemStateMachine;

    #[tokio::test]
    async fn test_state_machine_store_creation() {
        let sm = SystemStateMachine::new();
        let store = Arc::new(StateMachineStore::new(sm));
        
        let mut store_clone = store.clone();
        let (last_applied, _membership) = store_clone.applied_state().await.unwrap();
        assert!(last_applied.is_none());
    }
}
