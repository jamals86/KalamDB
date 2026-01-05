//! SharedDataStateMachine - Handles shared table operations
//!
//! This state machine manages:
//! - Shared table INSERT/UPDATE/DELETE
//!
//! Runs in the DataSharedShard(0) Raft group.
//! Shared tables are not sharded by user - all operations go to shard 0.

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{GroupId, RaftError, SharedDataCommand, DataResponse};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};

/// Row operation tracking (for metrics)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedOperation {
    table_namespace: String,
    table_name: String,
    operation: String, // "insert", "update", "delete"
    row_count: u64,
}

/// Snapshot data for SharedDataStateMachine
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedDataSnapshot {
    /// Total operations count
    total_operations: u64,
    /// Recent operations for metrics
    recent_operations: Vec<SharedOperation>,
}

/// State machine for shared table operations
///
/// Handles commands in DataSharedShard(0) Raft group:
/// - Insert, Update, Delete (shared table data)
///
/// Note: Actual row data is stored in RocksDB/Parquet via the storage layer.
/// The Raft log ensures ordering and consistency of operations.
#[derive(Debug)]
pub struct SharedDataStateMachine {
    /// Shard number (always 0 for shared tables in Phase 1)
    shard: u32,
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    /// Total operations processed
    total_operations: AtomicU64,
    /// Recent operations for metrics
    recent_operations: RwLock<Vec<SharedOperation>>,
}

impl SharedDataStateMachine {
    /// Create a new SharedDataStateMachine
    pub fn new(shard: u32) -> Self {
        Self {
            shard,
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            total_operations: AtomicU64::new(0),
            recent_operations: RwLock::new(Vec::new()),
        }
    }
    
    /// Create the default shared shard (shard 0)
    pub fn default_shard() -> Self {
        Self::new(0)
    }
    
    /// Apply a shared data command
    async fn apply_command(&self, cmd: SharedDataCommand) -> Result<DataResponse, RaftError> {
        match cmd {
            SharedDataCommand::Insert { table_id, rows_data } => {
                log::debug!(
                    "SharedDataStateMachine[{}]: Insert into {:?} ({} bytes)",
                    self.shard, table_id, rows_data.len()
                );
                
                // Track operation
                let op = SharedOperation {
                    table_namespace: table_id.namespace_id().as_str().to_string(),
                    table_name: table_id.table_name().as_str().to_string(),
                    operation: "insert".to_string(),
                    row_count: 0, // Actual count from storage layer
                };
                
                {
                    let mut ops = self.recent_operations.write();
                    ops.push(op);
                    // Keep only last 100 operations
                    if ops.len() > 100 {
                        ops.remove(0);
                    }
                }
                
                self.total_operations.fetch_add(1, Ordering::Relaxed);
                self.approximate_size.fetch_add(rows_data.len() as u64, Ordering::Relaxed);
                
                Ok(DataResponse::RowsAffected(0))
            }
            
            SharedDataCommand::Update { table_id, .. } => {
                log::debug!("SharedDataStateMachine[{}]: Update {:?}", self.shard, table_id);
                
                let op = SharedOperation {
                    table_namespace: table_id.namespace_id().as_str().to_string(),
                    table_name: table_id.table_name().as_str().to_string(),
                    operation: "update".to_string(),
                    row_count: 0,
                };
                
                {
                    let mut ops = self.recent_operations.write();
                    ops.push(op);
                    if ops.len() > 100 {
                        ops.remove(0);
                    }
                }
                
                self.total_operations.fetch_add(1, Ordering::Relaxed);
                Ok(DataResponse::RowsAffected(0))
            }
            
            SharedDataCommand::Delete { table_id, .. } => {
                log::debug!("SharedDataStateMachine[{}]: Delete from {:?}", self.shard, table_id);
                
                let op = SharedOperation {
                    table_namespace: table_id.namespace_id().as_str().to_string(),
                    table_name: table_id.table_name().as_str().to_string(),
                    operation: "delete".to_string(),
                    row_count: 0,
                };
                
                {
                    let mut ops = self.recent_operations.write();
                    ops.push(op);
                    if ops.len() > 100 {
                        ops.remove(0);
                    }
                }
                
                self.total_operations.fetch_add(1, Ordering::Relaxed);
                Ok(DataResponse::RowsAffected(0))
            }
        }
    }
}

impl Default for SharedDataStateMachine {
    fn default() -> Self {
        Self::default_shard()
    }
}

#[async_trait]
impl KalamStateMachine for SharedDataStateMachine {
    fn group_id(&self) -> GroupId {
        GroupId::DataSharedShard(self.shard)
    }
    
    async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
        // Idempotency check
        let last_applied = self.last_applied_index.load(Ordering::Acquire);
        if index <= last_applied {
            log::debug!(
                "SharedDataStateMachine[{}]: Skipping already applied entry {} (last_applied={})",
                self.shard, index, last_applied
            );
            return Ok(ApplyResult::NoOp);
        }
        
        // Deserialize command
        let cmd: SharedDataCommand = decode(command)?;
        
        // Apply command
        let response = self.apply_command(cmd).await?;
        
        // Update last applied
        self.last_applied_index.store(index, Ordering::Release);
        self.last_applied_term.store(term, Ordering::Release);
        
        // Serialize response
        let response_data = encode(&response)?;
        
        Ok(ApplyResult::ok_with_data(response_data))
    }
    
    fn last_applied_index(&self) -> u64 {
        self.last_applied_index.load(Ordering::Acquire)
    }
    
    fn last_applied_term(&self) -> u64 {
        self.last_applied_term.load(Ordering::Acquire)
    }
    
    async fn snapshot(&self) -> Result<StateMachineSnapshot, RaftError> {
        let recent_operations = self.recent_operations.read().clone();
        
        let snapshot = SharedDataSnapshot {
            total_operations: self.total_operations.load(Ordering::Relaxed),
            recent_operations,
        };
        
        let data = encode(&snapshot)?;
        
        Ok(StateMachineSnapshot::new(
            self.group_id(),
            self.last_applied_index(),
            self.last_applied_term(),
            data,
        ))
    }
    
    async fn restore(&self, snapshot: StateMachineSnapshot) -> Result<(), RaftError> {
        let data: SharedDataSnapshot = decode(&snapshot.data)?;
        
        {
            let mut ops = self.recent_operations.write();
            *ops = data.recent_operations;
        }
        
        self.total_operations.store(data.total_operations, Ordering::Release);
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "SharedDataStateMachine[{}]: Restored from snapshot at index {}, term {}",
            self.shard, snapshot.last_applied_index, snapshot.last_applied_term
        );
        
        Ok(())
    }
    
    fn approximate_size(&self) -> usize {
        self.approximate_size.load(Ordering::Relaxed) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::TableId;
    use kalamdb_commons::models::NamespaceId;

    #[tokio::test]
    async fn test_shared_data_state_machine_insert() {
        let sm = SharedDataStateMachine::default();
        
        let cmd = SharedDataCommand::Insert {
            table_id: TableId::new(NamespaceId::new("default"), "config".into()),
            rows_data: vec![1, 2, 3, 4, 5],
        };
        let cmd_bytes = encode(&cmd).unwrap();
        
        let result = sm.apply(1, 1, &cmd_bytes).await.unwrap();
        assert!(result.is_ok());
        assert_eq!(sm.last_applied_index(), 1);
        assert_eq!(sm.total_operations.load(Ordering::Relaxed), 1);
    }
    
    #[tokio::test]
    async fn test_shared_data_state_machine_multiple_ops() {
        let sm = SharedDataStateMachine::default();
        
        // Insert
        let insert = SharedDataCommand::Insert {
            table_id: TableId::new(NamespaceId::new("default"), "settings".into()),
            rows_data: vec![1, 2, 3],
        };
        sm.apply(1, 1, &encode(&insert).unwrap()).await.unwrap();
        
        // Update
        let update = SharedDataCommand::Update {
            table_id: TableId::new(NamespaceId::new("default"), "settings".into()),
            updates_data: vec![4, 5, 6],
            filter_data: None,
        };
        sm.apply(2, 1, &encode(&update).unwrap()).await.unwrap();
        
        // Delete
        let delete = SharedDataCommand::Delete {
            table_id: TableId::new(NamespaceId::new("default"), "settings".into()),
            filter_data: None,
        };
        sm.apply(3, 1, &encode(&delete).unwrap()).await.unwrap();
        
        assert_eq!(sm.total_operations.load(Ordering::Relaxed), 3);
        assert_eq!(sm.last_applied_index(), 3);
        
        // Check recent operations
        {
            let ops = sm.recent_operations.read();
            assert_eq!(ops.len(), 3);
            assert_eq!(ops[0].operation, "insert");
            assert_eq!(ops[1].operation, "update");
            assert_eq!(ops[2].operation, "delete");
        }
    }
}
