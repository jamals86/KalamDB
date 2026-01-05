//! UsersStateMachine - Handles user operations
//!
//! This state machine manages:
//! - User CRUD (create, update, delete)
//! - Login tracking
//! - Lock/unlock operations
//!
//! Runs in the MetaUsers Raft group.

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{GroupId, RaftError, UsersCommand, UsersResponse};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};

/// User state for snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserState {
    user_id: String,
    username: String,
    role: String,
    locked: bool,
    deleted: bool,
}

/// Snapshot data for UsersStateMachine
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsersSnapshot {
    /// All users keyed by user_id
    users: HashMap<String, UserState>,
}

/// State machine for user operations
///
/// Handles commands in the MetaUsers Raft group:
/// - CreateUser, UpdateUser, DeleteUser
/// - RecordLogin, SetLocked
#[derive(Debug)]
pub struct UsersStateMachine {
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    /// Cached user states
    users: RwLock<HashMap<String, UserState>>,
}

impl UsersStateMachine {
    /// Create a new UsersStateMachine
    pub fn new() -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            users: RwLock::new(HashMap::new()),
        }
    }
    
    /// Apply a users command
    async fn apply_command(&self, cmd: UsersCommand) -> Result<UsersResponse, RaftError> {
        match cmd {
            UsersCommand::CreateUser { user_id, username, role, .. } => {
                log::debug!("UsersStateMachine: CreateUser {:?} ({})", user_id, username);
                
                let user_state = UserState {
                    user_id: user_id.as_str().to_string(),
                    username: username.clone(),
                    role,
                    locked: false,
                    deleted: false,
                };
                
                {
                    let mut users = self.users.write();
                    users.insert(user_id.as_str().to_string(), user_state);
                }
                
                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(UsersResponse::UserCreated { user_id })
            }
            
            UsersCommand::UpdateUser { user_id, role, .. } => {
                log::debug!("UsersStateMachine: UpdateUser {:?}", user_id);
                
                {
                    let mut users = self.users.write();
                    if let Some(user) = users.get_mut(user_id.as_str()) {
                        if let Some(new_role) = role {
                            user.role = new_role;
                        }
                    }
                }
                
                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::DeleteUser { user_id, .. } => {
                log::debug!("UsersStateMachine: DeleteUser {:?}", user_id);
                
                {
                    let mut users = self.users.write();
                    if let Some(user) = users.get_mut(user_id.as_str()) {
                        user.deleted = true;
                    }
                }
                
                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::RecordLogin { user_id, .. } => {
                log::trace!("UsersStateMachine: RecordLogin {:?}", user_id);
                // Login tracking is recorded but doesn't change user state significantly
                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::SetLocked { user_id, locked, .. } => {
                log::debug!("UsersStateMachine: SetLocked {:?} = {}", user_id, locked);
                
                {
                    let mut users = self.users.write();
                    if let Some(user) = users.get_mut(user_id.as_str()) {
                        user.locked = locked;
                    }
                }
                
                Ok(UsersResponse::Ok)
            }
        }
    }
}

impl Default for UsersStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KalamStateMachine for UsersStateMachine {
    fn group_id(&self) -> GroupId {
        GroupId::MetaUsers
    }
    
    async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
        // Idempotency check
        let last_applied = self.last_applied_index.load(Ordering::Acquire);
        if index <= last_applied {
            log::debug!(
                "UsersStateMachine: Skipping already applied entry {} (last_applied={})",
                index, last_applied
            );
            return Ok(ApplyResult::NoOp);
        }
        
        // Deserialize command
        let cmd: UsersCommand = decode(command)?;
        
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
        let users = self.users.read().clone();
        let snapshot = UsersSnapshot { users };
        
        let data = encode(&snapshot)?;
        
        Ok(StateMachineSnapshot::new(
            self.group_id(),
            self.last_applied_index(),
            self.last_applied_term(),
            data,
        ))
    }
    
    async fn restore(&self, snapshot: StateMachineSnapshot) -> Result<(), RaftError> {
        let data: UsersSnapshot = decode(&snapshot.data)?;
        
        {
            let mut users = self.users.write();
            *users = data.users;
        }
        
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "UsersStateMachine: Restored from snapshot at index {}, term {}",
            snapshot.last_applied_index, snapshot.last_applied_term
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
    use kalamdb_commons::UserId;

    #[tokio::test]
    async fn test_users_state_machine_create_user() {
        let sm = UsersStateMachine::new();
        
        let cmd = UsersCommand::CreateUser {
            user_id: UserId::new("user123"),
            username: "testuser".to_string(),
            role: "user".to_string(),
            password_hash: "hash".to_string(),
            created_at: chrono::Utc::now(),
        };
        let cmd_bytes = encode(&cmd).unwrap();
        
        let result = sm.apply(1, 1, &cmd_bytes).await.unwrap();
        assert!(result.is_ok());
        
        // Check user was added
        let users = sm.users.read();
        assert!(users.contains_key("user123"));
        assert_eq!(users.get("user123").unwrap().username, "testuser");
    }
    
    #[tokio::test]
    async fn test_users_state_machine_lock_user() {
        let sm = UsersStateMachine::new();
        
        // Create user first
        let create_cmd = UsersCommand::CreateUser {
            user_id: UserId::new("user123"),
            username: "testuser".to_string(),
            role: "user".to_string(),
            password_hash: "hash".to_string(),
            created_at: chrono::Utc::now(),
        };
        sm.apply(1, 1, &encode(&create_cmd).unwrap()).await.unwrap();
        
        // Lock user
        let lock_cmd = UsersCommand::SetLocked {
            user_id: UserId::new("user123"),
            locked: true,
            updated_at: chrono::Utc::now(),
        };
        sm.apply(2, 1, &encode(&lock_cmd).unwrap()).await.unwrap();
        
        // Check user is locked
        let users = sm.users.read();
        assert!(users.get("user123").unwrap().locked);
    }
}
