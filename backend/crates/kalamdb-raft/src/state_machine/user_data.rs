//! UserDataStateMachine - Handles per-user data operations
//!
//! This state machine manages:
//! - User table INSERT/UPDATE/DELETE
//! - Live query registrations for users in this shard
//!
//! Runs in DataUserShard(N) Raft groups where N = user_id % 32.

use async_trait::async_trait;
use kalamdb_commons::models::NodeId;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{GroupId, RaftError, UserDataCommand, DataResponse};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};

/// Live query state for this shard
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveQueryState {
    subscription_id: String,
    user_id: String,
    table_namespace: String,
    table_name: String,
    query_hash: String,
    node_id: NodeId,
}

/// Row operation tracking (for replay/snapshot)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RowOperation {
    table_namespace: String,
    table_name: String,
    operation: String, // "insert", "update", "delete"
    row_count: u64,
}

/// Snapshot data for UserDataStateMachine
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserDataSnapshot {
    /// Shard number
    shard: u32,
    /// Active live queries in this shard
    live_queries: HashMap<String, LiveQueryState>,
    /// Recent row operations (for metrics, not for replay)
    /// Actual data is in RocksDB/Parquet, not replicated via Raft
    recent_operations: Vec<RowOperation>,
    /// Total operations count
    total_operations: u64,
}

/// State machine for user data operations
///
/// Handles commands in DataUserShard(N) Raft groups:
/// - Insert, Update, Delete (user table data)
/// - RegisterLiveQuery, UnregisterLiveQuery
/// - CleanupNodeSubscriptions, PingLiveQuery
///
/// Note: Actual row data is stored in RocksDB/Parquet via the storage layer.
/// The Raft log ensures ordering and consistency of operations, but the
/// state machine delegates actual writes to the storage backend.
#[derive(Debug)]
pub struct UserDataStateMachine {
    /// Which shard this state machine handles (0-31)
    shard: u32,
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    /// Total operations processed
    total_operations: AtomicU64,
    /// Active live queries
    live_queries: RwLock<HashMap<String, LiveQueryState>>,
}

impl UserDataStateMachine {
    /// Create a new UserDataStateMachine for the specified shard
    pub fn new(shard: u32) -> Self {
        assert!(shard < 32, "Shard must be 0-31");
        Self {
            shard,
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            total_operations: AtomicU64::new(0),
            live_queries: RwLock::new(HashMap::new()),
        }
    }
    
    /// Apply a user data command
    async fn apply_command(&self, _user_id: &str, cmd: UserDataCommand) -> Result<DataResponse, RaftError> {
        match cmd {
            UserDataCommand::Insert { table_id, rows_data, .. } => {
                log::debug!(
                    "UserDataStateMachine[{}]: Insert into {:?} ({} bytes)",
                    self.shard, table_id, rows_data.len()
                );
                
                // NOTE: Actual insert is delegated to the storage layer
                // The Raft log entry ensures this operation is ordered consistently
                // The executor will call the actual UserTableStore after consensus
                
                self.total_operations.fetch_add(1, Ordering::Relaxed);
                self.approximate_size.fetch_add(rows_data.len() as u64, Ordering::Relaxed);
                
                // Return placeholder - actual row count comes from storage layer
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::Update { table_id, .. } => {
                log::debug!("UserDataStateMachine[{}]: Update {:?}", self.shard, table_id);
                self.total_operations.fetch_add(1, Ordering::Relaxed);
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::Delete { table_id, .. } => {
                log::debug!("UserDataStateMachine[{}]: Delete from {:?}", self.shard, table_id);
                self.total_operations.fetch_add(1, Ordering::Relaxed);
                Ok(DataResponse::RowsAffected(0))
            }
            
            UserDataCommand::RegisterLiveQuery { 
                subscription_id, 
                user_id, 
                query_hash, 
                table_id, 
                node_id, 
                .. 
            } => {
                log::debug!(
                    "UserDataStateMachine[{}]: RegisterLiveQuery {} for user {:?}",
                    self.shard, subscription_id, user_id
                );
                
                let lq = LiveQueryState {
                    subscription_id: subscription_id.clone(),
                    user_id: user_id.as_str().to_string(),
                    table_namespace: table_id.namespace_id().as_str().to_string(),
                    table_name: table_id.table_name().as_str().to_string(),
                    query_hash,
                    node_id: node_id.clone(),
                };
                
                {
                    let mut live_queries = self.live_queries.write();
                    live_queries.insert(subscription_id.clone(), lq);
                }
                
                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(DataResponse::Subscribed { subscription_id })
            }
            
            UserDataCommand::UnregisterLiveQuery { subscription_id, user_id } => {
                log::debug!(
                    "UserDataStateMachine[{}]: UnregisterLiveQuery {} for user {:?}",
                    self.shard, subscription_id, user_id
                );
                
                {
                    let mut live_queries = self.live_queries.write();
                    live_queries.remove(&subscription_id);
                }
                
                Ok(DataResponse::Ok)
            }
            
            UserDataCommand::CleanupNodeSubscriptions { user_id, failed_node_id } => {
                log::info!(
                    "UserDataStateMachine[{}]: Cleanup subscriptions from node {} for user {:?}",
                    self.shard, failed_node_id, user_id
                );
                
                let removed = {
                    let mut live_queries = self.live_queries.write();
                    let before = live_queries.len();
                    live_queries.retain(|_, lq| lq.node_id != failed_node_id);
                    before - live_queries.len()
                };
                
                Ok(DataResponse::RowsAffected(removed))
            }
            
            UserDataCommand::PingLiveQuery { subscription_id, user_id, .. } => {
                log::trace!(
                    "UserDataStateMachine[{}]: PingLiveQuery {} for user {:?}",
                    self.shard, subscription_id, user_id
                );
                // Ping just updates last_seen timestamp - no state change needed here
                Ok(DataResponse::Ok)
            }
        }
    }
}

#[async_trait]
impl KalamStateMachine for UserDataStateMachine {
    fn group_id(&self) -> GroupId {
        GroupId::DataUserShard(self.shard)
    }
    
    async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
        // Idempotency check
        let last_applied = self.last_applied_index.load(Ordering::Acquire);
        if index <= last_applied {
            log::debug!(
                "UserDataStateMachine[{}]: Skipping already applied entry {} (last_applied={})",
                self.shard, index, last_applied
            );
            return Ok(ApplyResult::NoOp);
        }
        
        // Deserialize: (user_id, command)
        let (user_id, cmd): (String, UserDataCommand) = decode(command)?;
        
        // Apply command
        let response = self.apply_command(&user_id, cmd).await?;
        
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
        let live_queries = self.live_queries.read().clone();
        
        let snapshot = UserDataSnapshot {
            shard: self.shard,
            live_queries,
            recent_operations: Vec::new(), // Not tracking for now
            total_operations: self.total_operations.load(Ordering::Relaxed),
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
        let data: UserDataSnapshot = decode(&snapshot.data)?;
        
        if data.shard != self.shard {
            return Err(RaftError::InvalidState(format!(
                "Snapshot shard {} doesn't match state machine shard {}",
                data.shard, self.shard
            )));
        }
        
        {
            let mut live_queries = self.live_queries.write();
            *live_queries = data.live_queries;
        }
        
        self.total_operations.store(data.total_operations, Ordering::Release);
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "UserDataStateMachine[{}]: Restored from snapshot at index {}, term {}",
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
    use kalamdb_commons::{TableId, UserId};
    use kalamdb_commons::models::NamespaceId;

    #[tokio::test]
    async fn test_user_data_state_machine_insert() {
        let sm = UserDataStateMachine::new(0);
        
        let cmd = UserDataCommand::Insert {
            table_id: TableId::new(NamespaceId::new("default"), "users".into()),
            user_id: UserId::new("user123"),
            rows_data: vec![1, 2, 3, 4],
        };
        
        // Serialize with user_id
        let payload = encode(&("user123".to_string(), cmd)).unwrap();
        
        let result = sm.apply(1, 1, &payload).await.unwrap();
        assert!(result.is_ok());
        assert_eq!(sm.last_applied_index(), 1);
    }
    
    #[tokio::test]
    async fn test_user_data_state_machine_live_query() {
        let sm = UserDataStateMachine::new(5);
        
        let node_id = NodeId::new(1);
        
        let cmd = UserDataCommand::RegisterLiveQuery {
            subscription_id: "sub-001".to_string(),
            user_id: UserId::new("user456"),
            query_hash: "hash123".to_string(),
            table_id: TableId::new(NamespaceId::new("app"), "messages".into()),
            filter_json: None,
            node_id: node_id.clone(),
            created_at: chrono::Utc::now(),
        };
        
        let payload = encode(&("user456".to_string(), cmd)).unwrap();
        sm.apply(1, 1, &payload).await.unwrap();
        
        // Check live query registered
        {
            let lqs = sm.live_queries.read();
            assert!(lqs.contains_key("sub-001"));
            assert_eq!(lqs.get("sub-001").unwrap().node_id, node_id);
        }
        
        // Cleanup node subscriptions
        let cleanup_cmd = UserDataCommand::CleanupNodeSubscriptions {
            user_id: UserId::new("user456"),
            failed_node_id: node_id.clone(),
        };
        let payload2 = encode(&("user456".to_string(), cleanup_cmd)).unwrap();
        sm.apply(2, 1, &payload2).await.unwrap();
        
        // Check live query removed
        {
            let lqs = sm.live_queries.read();
            assert!(!lqs.contains_key("sub-001"));
        }
    }
}
