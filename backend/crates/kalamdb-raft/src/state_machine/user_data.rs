//! UserDataStateMachine - Handles per-user data operations
//!
//! This state machine manages:
//! - User table INSERT/UPDATE/DELETE
//! - Live query registrations for users in this shard
//!
//! Runs in DataUserShard(N) Raft groups where N = user_id % 32.
//!
//! ## Watermark Synchronization
//!
//! Commands with `required_meta_index > current_meta_index` wait for Meta
//! to catch up before applying. This ensures data operations don't run before
//! their dependent metadata (tables, users) is applied locally.

use async_trait::async_trait;
use kalamdb_commons::models::NodeId;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::applier::UserDataApplier;
use crate::{DataResponse, GroupId, RaftError, UserDataCommand};

use super::{decode, encode, ApplyResult, KalamStateMachine, StateMachineSnapshot};
use super::{PendingBuffer, PendingCommand, get_coordinator};

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
    /// Pending commands waiting for Meta to catch up (for crash recovery)
    #[serde(default)]
    pending_commands: Vec<PendingCommand>,
}

/// State machine for user data operations
///
/// Handles commands in DataUserShard(N) Raft groups:
/// - Insert, Update, Delete (user table data)
/// - RegisterLiveQuery, UnregisterLiveQuery
/// - CleanupNodeSubscriptions, PingLiveQuery
///
/// Note: Row data is persisted via the UserDataApplier after Raft consensus.
/// All nodes (leader and followers) call the applier, ensuring consistent data.
///
/// ## Watermark Synchronization
///
/// Commands with `required_meta_index > current_meta_index` wait for Meta
/// to catch up before applying. Pending commands restored from snapshots
/// are drained once Meta is satisfied.
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
    /// Optional applier for persisting data to providers
    applier: RwLock<Option<Arc<dyn UserDataApplier>>>,
    /// Buffer for commands waiting for Meta to catch up
    pending_buffer: PendingBuffer,
}

impl std::fmt::Debug for UserDataStateMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserDataStateMachine")
            .field("shard", &self.shard)
            .field(
                "last_applied_index",
                &self.last_applied_index.load(Ordering::Relaxed),
            )
            .field("has_applier", &self.applier.read().is_some())
            .field("pending_count", &self.pending_buffer.len())
            .finish()
    }
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
            applier: RwLock::new(None),
            pending_buffer: PendingBuffer::new(),
        }
    }

    /// Create a new UserDataStateMachine with an applier
    pub fn with_applier(shard: u32, applier: Arc<dyn UserDataApplier>) -> Self {
        assert!(shard < 32, "Shard must be 0-31");
        Self {
            shard,
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            total_operations: AtomicU64::new(0),
            live_queries: RwLock::new(HashMap::new()),
            applier: RwLock::new(Some(applier)),
            pending_buffer: PendingBuffer::new(),
        }
    }

    /// Set the applier for persisting data to providers
    ///
    /// This is called after RaftManager creation once providers are available.
    pub fn set_applier(&self, applier: Arc<dyn UserDataApplier>) {
        let mut guard = self.applier.write();
        *guard = Some(applier);
        log::debug!(
            "UserDataStateMachine[{}]: Applier registered for data persistence",
            self.shard
        );
    }
    
    /// Drain and apply any buffered commands that are now satisfied
    ///
    /// Called after Meta advances to check if any pending commands can now run.
    async fn drain_pending(&self) -> Result<(), RaftError> {
        let current_meta = get_coordinator().current_index();
        let drained = self.pending_buffer.drain_satisfied(current_meta);
        
        if !drained.is_empty() {
            log::info!(
                "UserDataStateMachine[{}]: Draining {} buffered commands (meta_index={})",
                self.shard, drained.len(), current_meta
            );
        }
        
        for pending in drained {
            // Deserialize and apply the command
            let cmd: UserDataCommand = decode(&pending.command_bytes)?;
            let _ = self.apply_command(cmd).await?;
            log::debug!(
                "UserDataStateMachine[{}]: Applied buffered command log_index={}",
                self.shard, pending.log_index
            );
        }
        
        Ok(())
    }
    
    /// Get the number of pending commands
    pub fn pending_count(&self) -> usize {
        self.pending_buffer.len()
    }

    /// Apply a user data command
    /// Note: user_id is extracted from inside each command variant
    async fn apply_command(
        &self,
        cmd: UserDataCommand,
    ) -> Result<DataResponse, RaftError> {
        // Get applier reference
        let applier = {
            let guard = self.applier.read();
            guard.clone()
        };

        match cmd {
            UserDataCommand::Insert {
                table_id,
                user_id,
                rows,
                ..
            } => {
                log::debug!(
                    "UserDataStateMachine[{}]: Insert into {:?} ({} rows)",
                    self.shard,
                    table_id,
                    rows.len()
                );

                // Persist data via applier if available
                let rows_affected = if let Some(ref a) = applier {
                    match a.insert(&table_id, &user_id, &rows).await {
                        Ok(count) => count,
                        Err(e) => {
                            // Convert applier errors to DataResponse::Error
                            // This is required because RaftError cannot be serialized
                            log::warn!(
                                "UserDataStateMachine[{}]: Insert failed: {}",
                                self.shard,
                                e
                            );
                            return Ok(DataResponse::error(e.to_string()));
                        }
                    }
                } else {
                    log::warn!(
                        "UserDataStateMachine[{}]: No applier set, data not persisted!",
                        self.shard
                    );
                    0
                };

                self.total_operations.fetch_add(1, Ordering::Relaxed);
                self.approximate_size
                    .fetch_add(rows.len() as u64, Ordering::Relaxed);

                Ok(DataResponse::RowsAffected(rows_affected))
            }

            UserDataCommand::Update {
                table_id,
                user_id,
                updates,
                filter,
                ..
            } => {
                log::debug!(
                    "UserDataStateMachine[{}]: Update {:?}",
                    self.shard,
                    table_id
                );

                let rows_affected = if let Some(ref a) = applier {
                    match a.update(
                        &table_id,
                        &user_id,
                        &updates,
                        filter.as_deref(),
                    )
                    .await {
                        Ok(count) => count,
                        Err(e) => {
                            log::warn!(
                                "UserDataStateMachine[{}]: Update failed: {}",
                                self.shard,
                                e
                            );
                            return Ok(DataResponse::error(e.to_string()));
                        }
                    }
                } else {
                    log::warn!(
                        "UserDataStateMachine[{}]: No applier set, update not persisted!",
                        self.shard
                    );
                    0
                };

                self.total_operations.fetch_add(1, Ordering::Relaxed);
                Ok(DataResponse::RowsAffected(rows_affected))
            }

            UserDataCommand::Delete {
                table_id,
                user_id,
                pk_values,
                ..
            } => {
                log::debug!(
                    "UserDataStateMachine[{}]: Delete from {:?}",
                    self.shard,
                    table_id
                );

                let rows_affected = if let Some(ref a) = applier {
                    match a.delete(&table_id, &user_id, pk_values.as_deref()).await {
                        Ok(count) => count,
                        Err(e) => {
                            log::warn!(
                                "UserDataStateMachine[{}]: Delete failed: {}",
                                self.shard,
                                e
                            );
                            return Ok(DataResponse::error(e.to_string()));
                        }
                    }
                } else {
                    log::warn!(
                        "UserDataStateMachine[{}]: No applier set, delete not persisted!",
                        self.shard
                    );
                    0
                };

                self.total_operations.fetch_add(1, Ordering::Relaxed);
                Ok(DataResponse::RowsAffected(rows_affected))
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
            
            UserDataCommand::UnregisterLiveQuery { subscription_id, user_id, .. } => {
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
            
            UserDataCommand::CleanupNodeSubscriptions { user_id, failed_node_id, .. } => {
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
        
        // Deserialize command to check watermark
        let cmd: UserDataCommand = decode(command)?;
        let required_meta = cmd.required_meta_index();
        let current_meta = get_coordinator().current_index();
        
        // Check watermark: if Meta is behind, wait for it to catch up
        if required_meta > current_meta {
            log::debug!(
                "UserDataStateMachine[{}]: Waiting for meta (required_meta={} > current_meta={})",
                self.shard, required_meta, current_meta
            );
            get_coordinator().wait_for(required_meta).await;
        }
        
        // First drain any pending commands that are now satisfied
        self.drain_pending().await?;
        
        // Apply current command
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
        let live_queries = self.live_queries.read().clone();
        let pending_commands = self.pending_buffer.get_all();
        
        let snapshot = UserDataSnapshot {
            shard: self.shard,
            live_queries,
            recent_operations: Vec::new(), // Not tracking for now
            total_operations: self.total_operations.load(Ordering::Relaxed),
            pending_commands,
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
        
        // Restore pending buffer from snapshot
        self.pending_buffer.load_from(data.pending_commands);
        
        self.total_operations.store(data.total_operations, Ordering::Release);
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "UserDataStateMachine[{}]: Restored from snapshot at index {}, term {}, {} pending commands",
            self.shard, snapshot.last_applied_index, snapshot.last_applied_term,
            self.pending_buffer.len()
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
            rows: vec![],
            required_meta_index: 0,
        };
        
        let payload = encode(&cmd).unwrap();
        
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
            required_meta_index: 0,
        };
        
        let payload = encode(&cmd).unwrap();
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
            required_meta_index: 0,
        };
        let payload2 = encode(&cleanup_cmd).unwrap();
        sm.apply(2, 1, &payload2).await.unwrap();
        
        // Check live query removed
        {
            let lqs = sm.live_queries.read();
            assert!(!lqs.contains_key("sub-001"));
        }
    }
}
