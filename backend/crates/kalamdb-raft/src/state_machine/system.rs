//! SystemStateMachine - Handles metadata operations
//!
//! This state machine manages:
//! - Namespace CRUD (create, delete)
//! - Table CRUD (create, alter, drop)
//! - Storage registration/unregistration
//!
//! Runs in the MetaSystem Raft group.

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::applier::SystemApplier;
use crate::{GroupId, RaftError, SystemCommand, SystemResponse};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};

/// Snapshot data for SystemStateMachine
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemSnapshot {
    /// All namespace IDs
    namespaces: Vec<String>,
    /// All table definitions (namespace_id, table_name, schema_json)
    tables: Vec<(String, String, String)>,
    /// All storage configs (storage_id, config_json)
    storages: Vec<(String, String)>,
}

/// State machine for system metadata operations
///
/// Handles commands in the MetaSystem Raft group:
/// - CreateNamespace, DeleteNamespace
/// - CreateTable, AlterTable, DropTable
/// - RegisterStorage, UnregisterStorage
pub struct SystemStateMachine {
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    /// Cached snapshot data (refreshed on apply)
    snapshot_cache: RwLock<Option<SystemSnapshot>>,
    /// Optional applier for persisting to providers
    applier: RwLock<Option<Arc<dyn SystemApplier>>>,
}

impl std::fmt::Debug for SystemStateMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemStateMachine")
            .field("last_applied_index", &self.last_applied_index.load(Ordering::Relaxed))
            .field("last_applied_term", &self.last_applied_term.load(Ordering::Relaxed))
            .field("approximate_size", &self.approximate_size.load(Ordering::Relaxed))
            .field("has_applier", &self.applier.read().is_some())
            .finish()
    }
}

impl SystemStateMachine {
    /// Create a new SystemStateMachine without an applier
    ///
    /// Use `set_applier` to inject persistence after construction.
    pub fn new() -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            snapshot_cache: RwLock::new(None),
            applier: RwLock::new(None),
        }
    }
    
    /// Create a new SystemStateMachine with an applier
    pub fn with_applier(applier: Arc<dyn SystemApplier>) -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            snapshot_cache: RwLock::new(None),
            applier: RwLock::new(Some(applier)),
        }
    }
    
    /// Set the applier for persisting to providers
    ///
    /// This is called after RaftManager creation once providers are available.
    pub fn set_applier(&self, applier: Arc<dyn SystemApplier>) {
        let mut guard = self.applier.write();
        *guard = Some(applier);
        log::info!("SystemStateMachine: Applier registered for persistence");
    }
    
    /// Apply a system command
    async fn apply_command(&self, cmd: SystemCommand) -> Result<SystemResponse, RaftError> {
        // Get applier reference
        let applier = {
            let guard = self.applier.read();
            guard.clone()
        };
        
        match cmd {
            SystemCommand::CreateNamespace { namespace_id, created_by } => {
                log::debug!(
                    "SystemStateMachine: CreateNamespace {:?} by {:?}",
                    namespace_id, created_by
                );
                
                // Persist to provider if applier is set
                if let Some(ref a) = applier {
                    a.create_namespace(
                        namespace_id.as_str(),
                        created_by.as_deref(),
                    ).await?;
                }
                
                // Track in snapshot cache
                {
                    let mut cache = self.snapshot_cache.write();
                    let cache_ref = cache.get_or_insert_with(|| SystemSnapshot {
                        namespaces: Vec::new(),
                        tables: Vec::new(),
                        storages: Vec::new(),
                    });
                    cache_ref.namespaces.push(namespace_id.as_str().to_string());
                }
                self.approximate_size.fetch_add(100, Ordering::Relaxed);
                Ok(SystemResponse::NamespaceCreated { namespace_id })
            }
            
            SystemCommand::DeleteNamespace { namespace_id } => {
                log::debug!("SystemStateMachine: DeleteNamespace {:?}", namespace_id);
                
                if let Some(ref a) = applier {
                    a.delete_namespace(namespace_id.as_str()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(ref mut snapshot) = *cache {
                        snapshot.namespaces.retain(|ns| ns != namespace_id.as_str());
                    }
                }
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::CreateTable { table_id, table_type, schema_json } => {
                log::debug!(
                    "SystemStateMachine: CreateTable {:?} (type: {})",
                    table_id, table_type
                );
                
                if let Some(ref a) = applier {
                    a.create_table(
                        table_id.namespace_id().as_str(),
                        table_id.table_name().as_str(),
                        &table_type,
                        &schema_json,
                    ).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    let cache_ref = cache.get_or_insert_with(|| SystemSnapshot {
                        namespaces: Vec::new(),
                        tables: Vec::new(),
                        storages: Vec::new(),
                    });
                    cache_ref.tables.push((
                        table_id.namespace_id().as_str().to_string(),
                        table_id.table_name().as_str().to_string(),
                        schema_json.clone(),
                    ));
                }
                self.approximate_size.fetch_add(schema_json.len() as u64 + 200, Ordering::Relaxed);
                Ok(SystemResponse::TableCreated { table_id })
            }
            
            SystemCommand::AlterTable { table_id, schema_json } => {
                log::debug!("SystemStateMachine: AlterTable {:?}", table_id);
                
                if let Some(ref a) = applier {
                    a.alter_table(
                        table_id.namespace_id().as_str(),
                        table_id.table_name().as_str(),
                        &schema_json,
                    ).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(ref mut snapshot) = *cache {
                        // Update existing table schema
                        for (ns, tbl, schema) in &mut snapshot.tables {
                            if ns == table_id.namespace_id().as_str() && tbl == table_id.table_name().as_str() {
                                *schema = schema_json.clone();
                                break;
                            }
                        }
                    }
                }
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::DropTable { table_id } => {
                log::debug!("SystemStateMachine: DropTable {:?}", table_id);
                
                if let Some(ref a) = applier {
                    a.drop_table(
                        table_id.namespace_id().as_str(),
                        table_id.table_name().as_str(),
                    ).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(ref mut snapshot) = *cache {
                        snapshot.tables.retain(|(ns, tbl, _)| {
                            !(ns == table_id.namespace_id().as_str() && tbl == table_id.table_name().as_str())
                        });
                    }
                }
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::RegisterStorage { storage_id, config_json } => {
                log::debug!("SystemStateMachine: RegisterStorage {}", storage_id);
                
                if let Some(ref a) = applier {
                    a.register_storage(&storage_id, &config_json).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    let cache_ref = cache.get_or_insert_with(|| SystemSnapshot {
                        namespaces: Vec::new(),
                        tables: Vec::new(),
                        storages: Vec::new(),
                    });
                    cache_ref.storages.push((storage_id.clone(), config_json.clone()));
                }
                self.approximate_size.fetch_add(config_json.len() as u64 + 100, Ordering::Relaxed);
                Ok(SystemResponse::Ok)
            }
            
            SystemCommand::UnregisterStorage { storage_id } => {
                log::debug!("SystemStateMachine: UnregisterStorage {}", storage_id);
                
                if let Some(ref a) = applier {
                    a.unregister_storage(&storage_id).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(ref mut snapshot) = *cache {
                        snapshot.storages.retain(|(id, _)| id != &storage_id);
                    }
                }
                Ok(SystemResponse::Ok)
            }
        }
    }
}

impl Default for SystemStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KalamStateMachine for SystemStateMachine {
    fn group_id(&self) -> GroupId {
        GroupId::MetaSystem
    }
    
    async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
        // Idempotency check
        let last_applied = self.last_applied_index.load(Ordering::Acquire);
        if index <= last_applied {
            log::debug!(
                "SystemStateMachine: Skipping already applied entry {} (last_applied={})",
                index, last_applied
            );
            return Ok(ApplyResult::NoOp);
        }
        
        // Deserialize command
        let cmd: SystemCommand = decode(command)?;
        
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
        let cache = self.snapshot_cache.read();
        let data = match &*cache {
            Some(snapshot) => encode(snapshot)?,
            None => {
                // Empty snapshot
                let empty = SystemSnapshot {
                    namespaces: Vec::new(),
                    tables: Vec::new(),
                    storages: Vec::new(),
                };
                encode(&empty)?
            }
        };
        
        Ok(StateMachineSnapshot::new(
            self.group_id(),
            self.last_applied_index(),
            self.last_applied_term(),
            data,
        ))
    }
    
    async fn restore(&self, snapshot: StateMachineSnapshot) -> Result<(), RaftError> {
        // Deserialize snapshot data
        let data: SystemSnapshot = decode(&snapshot.data)?;
        
        // Update state
        {
            let mut cache = self.snapshot_cache.write();
            *cache = Some(data);
        }
        
        // Update indices
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "SystemStateMachine: Restored from snapshot at index {}, term {}",
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
    use kalamdb_commons::models::NamespaceId;

    #[tokio::test]
    async fn test_system_state_machine_create_namespace() {
        let sm = SystemStateMachine::new();
        
        // Create command
        let cmd = SystemCommand::CreateNamespace {
            namespace_id: NamespaceId::new("test_ns"),
            created_by: None,
        };
        let cmd_bytes = encode(&cmd).unwrap();
        
        // Apply
        let result = sm.apply(1, 1, &cmd_bytes).await.unwrap();
        assert!(result.is_ok());
        
        // Check last applied
        assert_eq!(sm.last_applied_index(), 1);
        assert_eq!(sm.last_applied_term(), 1);
    }
    
    #[tokio::test]
    async fn test_system_state_machine_idempotency() {
        let sm = SystemStateMachine::new();
        
        let cmd = SystemCommand::CreateNamespace {
            namespace_id: NamespaceId::new("test_ns"),
            created_by: None,
        };
        let cmd_bytes = encode(&cmd).unwrap();
        
        // Apply first time
        let result1 = sm.apply(1, 1, &cmd_bytes).await.unwrap();
        assert!(matches!(result1, ApplyResult::Ok(_)));
        
        // Apply same index again (should be no-op)
        let result2 = sm.apply(1, 1, &cmd_bytes).await.unwrap();
        assert!(matches!(result2, ApplyResult::NoOp));
    }
    
    #[tokio::test]
    async fn test_system_state_machine_snapshot_restore() {
        let sm1 = SystemStateMachine::new();
        
        // Apply some commands
        let cmd1 = SystemCommand::CreateNamespace {
            namespace_id: NamespaceId::new("ns1"),
            created_by: None,
        };
        sm1.apply(1, 1, &encode(&cmd1).unwrap()).await.unwrap();
        
        let cmd2 = SystemCommand::CreateNamespace {
            namespace_id: NamespaceId::new("ns2"),
            created_by: None,
        };
        sm1.apply(2, 1, &encode(&cmd2).unwrap()).await.unwrap();
        
        // Create snapshot
        let snapshot = sm1.snapshot().await.unwrap();
        assert_eq!(snapshot.last_applied_index, 2);
        
        // Restore to new state machine
        let sm2 = SystemStateMachine::new();
        sm2.restore(snapshot).await.unwrap();
        
        assert_eq!(sm2.last_applied_index(), 2);
        assert_eq!(sm2.last_applied_term(), 1);
    }
}
