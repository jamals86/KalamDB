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
use std::sync::Arc;

use crate::applier::UsersApplier;
use crate::{GroupId, RaftError, UsersCommand, UsersResponse};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};
use kalamdb_commons::types::User;

/// Snapshot data for UsersStateMachine
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UsersSnapshot {
    /// All users keyed by user_id
    users: HashMap<String, User>,
}

/// State machine for user operations
///
/// Handles commands in the MetaUsers Raft group:
/// - CreateUser, UpdateUser, DeleteUser
/// - RecordLogin, SetLocked
pub struct UsersStateMachine {
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    /// Cached user states
    users: RwLock<HashMap<String, User>>,
    /// Optional applier for persisting to providers
    applier: RwLock<Option<Arc<dyn UsersApplier>>>,
}

impl std::fmt::Debug for UsersStateMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsersStateMachine")
            .field("last_applied_index", &self.last_applied_index.load(Ordering::Relaxed))
            .field("last_applied_term", &self.last_applied_term.load(Ordering::Relaxed))
            .field("approximate_size", &self.approximate_size.load(Ordering::Relaxed))
            .field("has_applier", &self.applier.read().is_some())
            .finish()
    }
}

impl UsersStateMachine {
    /// Create a new UsersStateMachine
    pub fn new() -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            users: RwLock::new(HashMap::new()),
            applier: RwLock::new(None),
        }
    }

    pub fn set_applier(&self, applier: Arc<dyn UsersApplier>) {
        let mut guard = self.applier.write();
        *guard = Some(applier);
        log::info!("UsersStateMachine: Applier registered for persistence");
    }
    
    /// Apply a users command
    async fn apply_command(&self, cmd: UsersCommand) -> Result<UsersResponse, RaftError> {
        let applier = {
            let guard = self.applier.read();
            guard.clone()
        };

        match cmd {
            UsersCommand::CreateUser { user } => {
                log::debug!("UsersStateMachine: CreateUser {:?} ({})", user.id, user.username);

                if let Some(ref a) = applier {
                    a.create_user(&user).await?;
                }

                {
                    let mut users = self.users.write();
                    users.insert(user.id.as_str().to_string(), user.clone());
                }

                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(UsersResponse::UserCreated { user_id: user.id })
            }
            
            UsersCommand::UpdateUser { user } => {
                log::debug!("UsersStateMachine: UpdateUser {:?}", user.id);

                if let Some(ref a) = applier {
                    a.update_user(&user).await?;
                }

                {
                    let mut users = self.users.write();
                    users.insert(user.id.as_str().to_string(), user);
                }

                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::DeleteUser { user_id, deleted_at } => {
                log::debug!("UsersStateMachine: DeleteUser {:?}", user_id);

                if let Some(ref a) = applier {
                    a.delete_user(&user_id, deleted_at.timestamp_millis()).await?;
                }

                {
                    let mut users = self.users.write();
                    if let Some(user) = users.get_mut(user_id.as_str()) {
                        user.deleted_at = Some(deleted_at.timestamp_millis());
                    }
                }

                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::RecordLogin { user_id, logged_in_at } => {
                log::trace!("UsersStateMachine: RecordLogin {:?}", user_id);

                if let Some(ref a) = applier {
                    a.record_login(&user_id, logged_in_at.timestamp_millis()).await?;
                }

                {
                    let mut users = self.users.write();
                    if let Some(user) = users.get_mut(user_id.as_str()) {
                        user.last_login_at = Some(logged_in_at.timestamp_millis());
                        user.updated_at = logged_in_at.timestamp_millis();
                    }
                }

                Ok(UsersResponse::Ok)
            }
            
            UsersCommand::SetLocked { user_id, locked_until, updated_at } => {
                log::debug!("UsersStateMachine: SetLocked {:?} = {:?}", user_id, locked_until);

                if let Some(ref a) = applier {
                    a.set_locked(&user_id, locked_until, updated_at.timestamp_millis()).await?;
                }

                {
                    let mut users = self.users.write();
                    if let Some(user) = users.get_mut(user_id.as_str()) {
                        user.locked_until = locked_until;
                        user.updated_at = updated_at.timestamp_millis();
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
        GroupId::Meta
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

        let applier = {
            let guard = self.applier.read();
            guard.clone()
        };

        if let Some(ref a) = applier {
            // Clone the users to avoid holding the lock across await
            let users_copy: Vec<User> = self.users.read().values().cloned().collect();
            for user in users_copy {
                a.update_user(&user).await?;
            }
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
    use kalamdb_commons::{AuthType, Role, StorageId, StorageMode};

    #[tokio::test]
    async fn test_users_state_machine_create_user() {
        let sm = UsersStateMachine::new();
        
        let cmd = UsersCommand::CreateUser {
            user: User {
                id: UserId::new("user123"),
                username: "testuser".into(),
                password_hash: "hash".to_string(),
                role: Role::User,
                email: None,
                auth_type: AuthType::Password,
                auth_data: None,
                storage_mode: StorageMode::Table,
                storage_id: Some(StorageId::new("local")),
                failed_login_attempts: 0,
                locked_until: None,
                last_login_at: None,
                created_at: chrono::Utc::now().timestamp_millis(),
                updated_at: chrono::Utc::now().timestamp_millis(),
                last_seen: None,
                deleted_at: None,
            },
        };
        let cmd_bytes = encode(&cmd).unwrap();
        
        let result = sm.apply(1, 1, &cmd_bytes).await.unwrap();
        assert!(result.is_ok());
        
        // Check user was added
        let users = sm.users.read();
        assert!(users.contains_key("user123"));
        assert_eq!(users.get("user123").unwrap().username.as_str(), "testuser");
    }
    
    #[tokio::test]
    async fn test_users_state_machine_lock_user() {
        let sm = UsersStateMachine::new();
        
        // Create user first
        let create_cmd = UsersCommand::CreateUser {
            user: User {
                id: UserId::new("user123"),
                username: "testuser".into(),
                password_hash: "hash".to_string(),
                role: Role::User,
                email: None,
                auth_type: AuthType::Password,
                auth_data: None,
                storage_mode: StorageMode::Table,
                storage_id: Some(StorageId::new("local")),
                failed_login_attempts: 0,
                locked_until: None,
                last_login_at: None,
                created_at: chrono::Utc::now().timestamp_millis(),
                updated_at: chrono::Utc::now().timestamp_millis(),
                last_seen: None,
                deleted_at: None,
            },
        };
        sm.apply(1, 1, &encode(&create_cmd).unwrap()).await.unwrap();
        
        // Lock user
        let lock_cmd = UsersCommand::SetLocked {
            user_id: UserId::new("user123"),
            locked_until: Some(chrono::Utc::now().timestamp_millis()),
            updated_at: chrono::Utc::now(),
        };
        sm.apply(2, 1, &encode(&lock_cmd).unwrap()).await.unwrap();
        
        // Check user is locked
        let users = sm.users.read();
        assert!(users.get("user123").unwrap().locked_until.is_some());
    }
}
