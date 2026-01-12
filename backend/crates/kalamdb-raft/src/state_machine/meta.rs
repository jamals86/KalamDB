//! MetaStateMachine - Unified metadata state machine
//!
//! This state machine handles all metadata operations in a single Raft group:
//! - Namespace CRUD (create, delete)
//! - Table CRUD (create, alter, drop)
//! - Storage registration/unregistration
//! - User CRUD (create, update, delete, login, lock)
//! - Job lifecycle (create, claim, update, complete, fail, release, cancel)
//! - Schedules (create, delete)
//!
//! Runs in the unified Meta Raft group (replaces MetaSystem + MetaUsers + MetaJobs).

use async_trait::async_trait;
use kalamdb_commons::models::{JobId, JobStatus, JobType, LiveQueryId, NamespaceId, NodeId, StorageId, TableId};
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::types::User;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::applier::MetaApplier;
use crate::commands::{MetaCommand, MetaResponse};
use crate::{GroupId, RaftError};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};

// =============================================================================
// Snapshot Structures
// =============================================================================

/// Job state for snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobState {
    job_id: JobId,
    job_type: JobType,
    status: JobStatus,
    claimed_by: Option<NodeId>,
    error_message: Option<String>,
}

/// Schedule state for snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScheduleState {
    schedule_id: String,
    job_type: JobType,
    cron_expr: String,
    enabled: bool,
}

/// Snapshot data for MetaStateMachine
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MetaSnapshot {
    // System metadata
    namespaces: Vec<NamespaceId>,
    tables: Vec<(TableId, TableType, String)>,
    storages: Vec<(StorageId, String)>,
    
    // User metadata
    users: HashMap<String, User>,
    
    // Job metadata
    jobs: HashMap<JobId, JobState>,
    schedules: HashMap<String, ScheduleState>,
    
    // Live query metadata (replicated across cluster)
    live_queries: HashMap<LiveQueryId, LiveQueryState>,
}

/// Live query state for snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveQueryState {
    live_id: LiveQueryId,
    connection_id: String,
    node_id: NodeId,
    created_at: i64,
}

// =============================================================================
// MetaStateMachine
// =============================================================================

/// Unified state machine for all metadata operations
///
/// This replaces SystemStateMachine, UsersStateMachine, and JobsStateMachine
/// with a single state machine that handles all metadata in one totally-ordered log.
///
/// ## Benefits
///
/// - Single `meta_index` for watermark-based data group ordering
/// - No cross-metadata race conditions
/// - Simpler catch-up coordination
pub struct MetaStateMachine {
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    
    /// Cached snapshot data
    snapshot_cache: RwLock<MetaSnapshot>,
    
    /// Optional applier for persisting to providers
    applier: RwLock<Option<Arc<dyn MetaApplier>>>,
}

impl std::fmt::Debug for MetaStateMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetaStateMachine")
            .field("last_applied_index", &self.last_applied_index.load(Ordering::Relaxed))
            .field("last_applied_term", &self.last_applied_term.load(Ordering::Relaxed))
            .field("approximate_size", &self.approximate_size.load(Ordering::Relaxed))
            .field("has_applier", &self.applier.read().is_some())
            .finish()
    }
}

impl MetaStateMachine {
    /// Create a new MetaStateMachine without an applier
    ///
    /// Use `set_applier` to inject persistence after construction.
    pub fn new() -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            snapshot_cache: RwLock::new(MetaSnapshot::default()),
            applier: RwLock::new(None),
        }
    }
    
    /// Create a new MetaStateMachine with an applier
    pub fn with_applier(applier: Arc<dyn MetaApplier>) -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            snapshot_cache: RwLock::new(MetaSnapshot::default()),
            applier: RwLock::new(Some(applier)),
        }
    }
    
    /// Set the applier for persisting to providers
    ///
    /// This is called after RaftManager creation once providers are available.
    pub fn set_applier(&self, applier: Arc<dyn MetaApplier>) {
        let mut guard = self.applier.write();
        *guard = Some(applier);
        log::debug!("MetaStateMachine: Applier registered for persistence");
    }
    
    /// Get the current last applied index
    ///
    /// This is used by data groups to capture the watermark at proposal time.
    pub fn last_applied_index(&self) -> u64 {
        self.last_applied_index.load(Ordering::Acquire)
    }
    
    /// Apply a meta command
    async fn apply_command(&self, cmd: MetaCommand) -> Result<MetaResponse, RaftError> {
        let applier = {
            let guard = self.applier.read();
            guard.clone()
        };

        if applier.is_none() {
            // Without an applier, Raft metadata operations will update only the in-memory snapshot
            // cache and will NOT be persisted into providers (system tables, schema registry, etc.).
            // This can make DDL appear to succeed while subsequent queries cannot see the change.
            log::warn!(
                "MetaStateMachine: No applier registered; applying MetaCommand without persistence"
            );
        }
        
        match cmd {
            // =================================================================
            // Namespace Operations
            // =================================================================
            MetaCommand::CreateNamespace { namespace_id, created_by } => {
                log::debug!("MetaStateMachine: CreateNamespace {:?} by {:?}", namespace_id, created_by);
                
                if let Some(ref a) = applier {
                    a.create_namespace(&namespace_id, created_by.as_ref()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.namespaces.push(namespace_id.clone());
                }
                self.approximate_size.fetch_add(100, Ordering::Relaxed);
                Ok(MetaResponse::NamespaceCreated { namespace_id })
            }
            
            MetaCommand::DeleteNamespace { namespace_id } => {
                log::debug!("MetaStateMachine: DeleteNamespace {:?}", namespace_id);
                
                if let Some(ref a) = applier {
                    a.delete_namespace(&namespace_id).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.namespaces.retain(|ns| ns != &namespace_id);
                }
                Ok(MetaResponse::Ok)
            }

            // =================================================================
            // Table Operations
            // =================================================================
            MetaCommand::CreateTable { table_id, table_type, schema_json } => {
                log::debug!("MetaStateMachine: CreateTable {:?} (type: {})", table_id, table_type);
                
                if let Some(ref a) = applier {
                    a.create_table(&table_id, table_type, &schema_json).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.tables.push((table_id.clone(), table_type, schema_json.clone()));
                }
                self.approximate_size.fetch_add(schema_json.len() as u64 + 200, Ordering::Relaxed);
                Ok(MetaResponse::TableCreated { table_id })
            }
            
            MetaCommand::AlterTable { table_id, schema_json } => {
                log::debug!("MetaStateMachine: AlterTable {:?}", table_id);
                
                if let Some(ref a) = applier {
                    a.alter_table(&table_id, &schema_json).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    for (tid, _, schema) in &mut cache.tables {
                        if tid == &table_id {
                            *schema = schema_json.clone();
                            break;
                        }
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::DropTable { table_id } => {
                log::debug!("MetaStateMachine: DropTable {:?}", table_id);
                
                if let Some(ref a) = applier {
                    a.drop_table(&table_id).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.tables.retain(|(tid, _, _)| tid != &table_id);
                }
                Ok(MetaResponse::Ok)
            }

            // =================================================================
            // Storage Operations
            // =================================================================
            MetaCommand::RegisterStorage { storage_id, config_json } => {
                log::debug!("MetaStateMachine: RegisterStorage {:?}", storage_id);
                
                if let Some(ref a) = applier {
                    a.register_storage(&storage_id, &config_json).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.storages.push((storage_id, config_json.clone()));
                }
                self.approximate_size.fetch_add(config_json.len() as u64 + 100, Ordering::Relaxed);
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::UnregisterStorage { storage_id } => {
                log::debug!("MetaStateMachine: UnregisterStorage {:?}", storage_id);
                
                if let Some(ref a) = applier {
                    a.unregister_storage(&storage_id).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.storages.retain(|(sid, _)| sid != &storage_id);
                }
                Ok(MetaResponse::Ok)
            }

            // =================================================================
            // User Operations
            // =================================================================
            MetaCommand::CreateUser { user } => {
                log::debug!("MetaStateMachine: CreateUser {:?} ({})", user.id, user.username);
                
                if let Some(ref a) = applier {
                    a.create_user(&user).await?;
                }
                
                let user_id = user.id.clone();
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.users.insert(user.id.as_str().to_string(), user);
                }
                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(MetaResponse::UserCreated { user_id })
            }
            
            MetaCommand::UpdateUser { user } => {
                log::debug!("MetaStateMachine: UpdateUser {:?}", user.id);
                
                if let Some(ref a) = applier {
                    a.update_user(&user).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.users.insert(user.id.as_str().to_string(), user);
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::DeleteUser { user_id, deleted_at } => {
                log::debug!("MetaStateMachine: DeleteUser {:?}", user_id);
                
                if let Some(ref a) = applier {
                    a.delete_user(&user_id, deleted_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(user) = cache.users.get_mut(user_id.as_str()) {
                        user.deleted_at = Some(deleted_at.timestamp_millis());
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::RecordLogin { user_id, logged_in_at } => {
                log::trace!("MetaStateMachine: RecordLogin {:?}", user_id);
                
                if let Some(ref a) = applier {
                    a.record_login(&user_id, logged_in_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(user) = cache.users.get_mut(user_id.as_str()) {
                        user.last_login_at = Some(logged_in_at.timestamp_millis());
                        user.updated_at = logged_in_at.timestamp_millis();
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::SetUserLocked { user_id, locked_until, updated_at } => {
                log::debug!("MetaStateMachine: SetUserLocked {:?} = {:?}", user_id, locked_until);
                
                if let Some(ref a) = applier {
                    a.set_user_locked(&user_id, locked_until, updated_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(user) = cache.users.get_mut(user_id.as_str()) {
                        user.locked_until = locked_until;
                        user.updated_at = updated_at.timestamp_millis();
                    }
                }
                Ok(MetaResponse::Ok)
            }

            // =================================================================
            // Job Operations
            // =================================================================
            MetaCommand::CreateJob { job_id, job_type, status, parameters_json, idempotency_key, max_retries, queue, priority, node_id, created_at } => {
                log::debug!("MetaStateMachine: CreateJob {} (type: {})", job_id, job_type);
                
                if let Some(ref a) = applier {
                    a.create_job(
                        &job_id,
                        job_type.clone(),
                        status.clone(),
                        parameters_json.as_deref(),
                        idempotency_key.as_deref(),
                        max_retries,
                        queue.as_deref(),
                        priority,
                        node_id.clone(),
                        created_at.timestamp_millis(),
                    ).await?;
                }
                
                let job_state = JobState {
                    job_id: job_id.clone(),
                    job_type,
                    status: status.clone(),
                    claimed_by: None,
                    error_message: None,
                };
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.jobs.insert(job_id.clone(), job_state);
                }
                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(MetaResponse::JobCreated { job_id })
            }
            
            MetaCommand::ClaimJob { job_id, node_id, claimed_at } => {
                log::debug!("MetaStateMachine: ClaimJob {} by node {}", job_id, node_id);
                
                // Check if already claimed
                {
                    let cache = self.snapshot_cache.read();
                    if let Some(job) = cache.jobs.get(&job_id) {
                        if let Some(ref claimed_node) = job.claimed_by {
                            return Ok(MetaResponse::Error {
                                message: format!("Job {} already claimed by node {}", job_id, claimed_node),
                            });
                        }
                    }
                }
                
                if let Some(ref a) = applier {
                    a.claim_job(&job_id, node_id.clone(), claimed_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(job) = cache.jobs.get_mut(&job_id) {
                        job.claimed_by = Some(node_id.clone());
                        job.status = JobStatus::Running;
                    }
                }
                Ok(MetaResponse::JobClaimed { job_id, node_id })
            }
            
            MetaCommand::UpdateJobStatus { job_id, status, updated_at } => {
                log::debug!("MetaStateMachine: UpdateJobStatus {} -> {:?}", job_id, status);
                
                if let Some(ref a) = applier {
                    a.update_job_status(&job_id, status, updated_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(job) = cache.jobs.get_mut(&job_id) {
                        job.status = status;
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::CompleteJob { job_id, result_json, completed_at } => {
                log::debug!("MetaStateMachine: CompleteJob {}", job_id);
                
                if let Some(ref a) = applier {
                    a.complete_job(&job_id, result_json.as_deref(), completed_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(job) = cache.jobs.get_mut(&job_id) {
                        job.status = JobStatus::Completed;
                        job.claimed_by = None;
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::FailJob { job_id, error_message, failed_at } => {
                log::warn!("MetaStateMachine: FailJob {}: {}", job_id, error_message);
                
                if let Some(ref a) = applier {
                    a.fail_job(&job_id, &error_message, failed_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(job) = cache.jobs.get_mut(&job_id) {
                        job.status = JobStatus::Failed;
                        job.error_message = Some(error_message);
                        job.claimed_by = None;
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::ReleaseJob { job_id, reason, released_at } => {
                log::debug!("MetaStateMachine: ReleaseJob {}: {}", job_id, reason);
                
                if let Some(ref a) = applier {
                    a.release_job(&job_id, &reason, released_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(job) = cache.jobs.get_mut(&job_id) {
                        job.status = JobStatus::Queued;
                        job.claimed_by = None;
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::CancelJob { job_id, reason, cancelled_at } => {
                log::debug!("MetaStateMachine: CancelJob {}: {}", job_id, reason);
                
                if let Some(ref a) = applier {
                    a.cancel_job(&job_id, &reason, cancelled_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    if let Some(job) = cache.jobs.get_mut(&job_id) {
                        job.status = JobStatus::Cancelled;
                        job.claimed_by = None;
                    }
                }
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::CreateSchedule { schedule_id, job_type, cron_expression, config_json: _, created_at: _ } => {
                log::debug!("MetaStateMachine: CreateSchedule {} (type: {})", schedule_id, job_type);
                
                if let Some(ref a) = applier {
                    // Note: Passing placeholder timestamps - the applier handles actual persistence
                    a.create_schedule(&schedule_id, job_type.clone(), &cron_expression, None, 0).await?;
                }
                
                let schedule = ScheduleState {
                    schedule_id: schedule_id.clone(),
                    job_type,
                    cron_expr: cron_expression,
                    enabled: true,
                };
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.schedules.insert(schedule_id, schedule);
                }
                self.approximate_size.fetch_add(150, Ordering::Relaxed);
                Ok(MetaResponse::Ok)
            }
            
            MetaCommand::DeleteSchedule { schedule_id } => {
                log::debug!("MetaStateMachine: DeleteSchedule {}", schedule_id);
                
                if let Some(ref a) = applier {
                    a.delete_schedule(&schedule_id).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.schedules.remove(&schedule_id);
                }
                Ok(MetaResponse::Ok)
            }

            // =================================================================
            // Live Query Operations
            // =================================================================
            MetaCommand::CreateLiveQuery { 
                live_id, connection_id, namespace_id, table_name, user_id, 
                query, options_json, node_id, subscription_id, created_at 
            } => {
                log::debug!("MetaStateMachine: CreateLiveQuery {} on node {}", live_id, node_id);
                
                if let Some(ref a) = applier {
                    a.create_live_query(
                        &live_id,
                        &connection_id,
                        &namespace_id,
                        &table_name,
                        &user_id,
                        &query,
                        options_json.as_deref(),
                        node_id.clone(),
                        &subscription_id,
                        created_at.timestamp_millis(),
                    ).await?;
                }
                
                let lq_state = LiveQueryState {
                    live_id: live_id.clone(),
                    connection_id,
                    node_id,
                    created_at: created_at.timestamp_millis(),
                };
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.live_queries.insert(live_id.clone(), lq_state);
                }
                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(MetaResponse::LiveQueryCreated { live_id })
            }

            MetaCommand::UpdateLiveQuery { live_id, last_update, changes } => {
                log::trace!("MetaStateMachine: UpdateLiveQuery {} (changes={})", live_id, changes);
                
                if let Some(ref a) = applier {
                    a.update_live_query(&live_id, last_update.timestamp_millis(), changes).await?;
                }
                
                // Live query state in cache is minimal - full state is in provider
                Ok(MetaResponse::Ok)
            }

            MetaCommand::DeleteLiveQuery { live_id, deleted_at } => {
                log::debug!("MetaStateMachine: DeleteLiveQuery {}", live_id);
                
                if let Some(ref a) = applier {
                    a.delete_live_query(&live_id, deleted_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.live_queries.remove(&live_id);
                }
                Ok(MetaResponse::Ok)
            }

            MetaCommand::DeleteLiveQueriesByConnection { connection_id, deleted_at } => {
                log::debug!("MetaStateMachine: DeleteLiveQueriesByConnection {}", connection_id);
                
                if let Some(ref a) = applier {
                    a.delete_live_queries_by_connection(&connection_id, deleted_at.timestamp_millis()).await?;
                }
                
                {
                    let mut cache = self.snapshot_cache.write();
                    cache.live_queries.retain(|_, lq| lq.connection_id != connection_id);
                }
                Ok(MetaResponse::Ok)
            }
        }
    }
}

impl Default for MetaStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KalamStateMachine for MetaStateMachine {
    fn group_id(&self) -> GroupId {
        GroupId::Meta
    }
    
    async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
        // Idempotency check
        let last_applied = self.last_applied_index.load(Ordering::Acquire);
        if index <= last_applied {
            log::debug!(
                "MetaStateMachine: Skipping already applied entry {} (last_applied={})",
                index, last_applied
            );
            return Ok(ApplyResult::NoOp);
        }
        
        // Deserialize command
        let cmd: MetaCommand = decode(command)?;
        
        log::debug!(
            "MetaStateMachine: Applying entry {} (term={}, category={})",
            index, term, cmd.category()
        );
        
        // Apply command
        let response = self.apply_command(cmd).await?;
        
        // Update last applied
        self.last_applied_index.store(index, Ordering::Release);
        self.last_applied_term.store(term, Ordering::Release);
        
        // Notify data shards that meta has advanced (for watermark draining)
        super::get_coordinator().advance(index);
        
        // Serialize response
        let response_bytes = encode(&response)?;
        Ok(ApplyResult::ok_with_data(response_bytes))
    }
    
    fn last_applied_index(&self) -> u64 {
        self.last_applied_index.load(Ordering::Acquire)
    }
    
    fn last_applied_term(&self) -> u64 {
        self.last_applied_term.load(Ordering::Acquire)
    }
    
    async fn snapshot(&self) -> Result<StateMachineSnapshot, RaftError> {
        let cache = self.snapshot_cache.read().clone();
        let snapshot_bytes = encode(&cache)?;
        
        Ok(StateMachineSnapshot::new(
            GroupId::Meta,
            self.last_applied_index.load(Ordering::Acquire),
            self.last_applied_term.load(Ordering::Acquire),
            snapshot_bytes,
        ))
    }
    
    async fn restore(&self, snapshot: StateMachineSnapshot) -> Result<(), RaftError> {
        let cache: MetaSnapshot = decode(&snapshot.data)?;
        
        {
            let mut guard = self.snapshot_cache.write();
            *guard = cache;
        }
        
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "MetaStateMachine: Restored snapshot at index {} term {}",
            snapshot.last_applied_index,
            snapshot.last_applied_term
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
    
    #[test]
    fn test_meta_state_machine_new() {
        let sm = MetaStateMachine::new();
        assert_eq!(sm.last_applied_index(), 0);
        assert_eq!(sm.group_id(), GroupId::Meta);
    }
}
