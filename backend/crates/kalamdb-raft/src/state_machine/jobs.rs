//! JobsStateMachine - Handles job operations
//!
//! This state machine manages:
//! - Job creation and lifecycle
//! - Job claiming (leader-only execution)
//! - Status updates
//! - Job schedules
//!
//! Runs in the MetaJobs Raft group.

use async_trait::async_trait;
use kalamdb_commons::models::{JobId, JobType, NodeId};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{GroupId, RaftError, JobsCommand, JobsResponse};
use super::{ApplyResult, KalamStateMachine, StateMachineSnapshot, encode, decode};

/// Job state for snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobState {
    job_id: JobId,
    job_type: JobType,
    status: String,
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

/// Snapshot data for JobsStateMachine
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobsSnapshot {
    /// Active jobs keyed by job_id
    jobs: HashMap<JobId, JobState>,
    /// Schedules keyed by schedule_id
    schedules: HashMap<String, ScheduleState>,
}

/// State machine for job operations
///
/// Handles commands in the MetaJobs Raft group:
/// - CreateJob, ClaimJob, UpdateJobStatus, CompleteJob, FailJob
/// - ReleaseJob, CancelJob
/// - CreateSchedule, DeleteSchedule
#[derive(Debug)]
pub struct JobsStateMachine {
    /// Last applied log index (for idempotency)
    last_applied_index: AtomicU64,
    /// Last applied log term
    last_applied_term: AtomicU64,
    /// Approximate data size in bytes
    approximate_size: AtomicU64,
    /// Active jobs keyed by JobId
    jobs: RwLock<HashMap<JobId, JobState>>,
    /// Schedules
    schedules: RwLock<HashMap<String, ScheduleState>>,
}

impl JobsStateMachine {
    /// Create a new JobsStateMachine
    pub fn new() -> Self {
        Self {
            last_applied_index: AtomicU64::new(0),
            last_applied_term: AtomicU64::new(0),
            approximate_size: AtomicU64::new(0),
            jobs: RwLock::new(HashMap::new()),
            schedules: RwLock::new(HashMap::new()),
        }
    }
    
    /// Apply a jobs command
    async fn apply_command(&self, cmd: JobsCommand) -> Result<JobsResponse, RaftError> {
        match cmd {
            JobsCommand::CreateJob { job_id, job_type, .. } => {
                log::debug!("JobsStateMachine: CreateJob {} (type: {})", job_id, job_type);
                
                let job_state = JobState {
                    job_id: job_id.clone(),
                    job_type: job_type.clone(),
                    status: "pending".to_string(),
                    claimed_by: None,
                    error_message: None,
                };
                
                {
                    let mut jobs = self.jobs.write();
                    jobs.insert(job_id.clone(), job_state);
                }
                
                self.approximate_size.fetch_add(200, Ordering::Relaxed);
                Ok(JobsResponse::JobCreated { job_id })
            }
            
            JobsCommand::ClaimJob { job_id, node_id, .. } => {
                log::debug!("JobsStateMachine: ClaimJob {} by node {}", job_id, node_id);
                
                {
                    let mut jobs = self.jobs.write();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        if let Some(ref claimed_node) = job.claimed_by {
                            // Already claimed - return error since the first claim wins
                            return Ok(JobsResponse::Error { 
                                message: format!("Job {} already claimed by node {}", job_id, claimed_node)
                            });
                        }
                        job.claimed_by = Some(node_id.clone());
                        job.status = "running".to_string();
                    }
                }
                
                Ok(JobsResponse::JobClaimed { job_id, node_id })
            }
            
            JobsCommand::UpdateJobStatus { job_id, status, .. } => {
                log::debug!("JobsStateMachine: UpdateJobStatus {} -> {}", job_id, status);
                
                {
                    let mut jobs = self.jobs.write();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = status;
                    }
                }
                
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::CompleteJob { job_id, .. } => {
                log::debug!("JobsStateMachine: CompleteJob {}", job_id);
                
                {
                    let mut jobs = self.jobs.write();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = "completed".to_string();
                        job.claimed_by = None;
                    }
                }
                
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::FailJob { job_id, error_message, .. } => {
                log::warn!("JobsStateMachine: FailJob {}: {}", job_id, error_message);
                
                {
                    let mut jobs = self.jobs.write();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = "failed".to_string();
                        job.error_message = Some(error_message);
                        job.claimed_by = None;
                    }
                }
                
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::ReleaseJob { job_id, reason, .. } => {
                log::debug!("JobsStateMachine: ReleaseJob {}: {}", job_id, reason);
                
                {
                    let mut jobs = self.jobs.write();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = "pending".to_string();
                        job.claimed_by = None;
                    }
                }
                
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::CancelJob { job_id, reason, .. } => {
                log::debug!("JobsStateMachine: CancelJob {}: {}", job_id, reason);
                
                {
                    let mut jobs = self.jobs.write();
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = "cancelled".to_string();
                        job.claimed_by = None;
                    }
                }
                
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::CreateSchedule { schedule_id, job_type, cron_expression, .. } => {
                log::debug!("JobsStateMachine: CreateSchedule {} (type: {})", schedule_id, job_type);
                
                let schedule = ScheduleState {
                    schedule_id: schedule_id.clone(),
                    job_type,
                    cron_expr: cron_expression,
                    enabled: true,
                };
                
                {
                    let mut schedules = self.schedules.write();
                    schedules.insert(schedule_id, schedule);
                }
                
                self.approximate_size.fetch_add(100, Ordering::Relaxed);
                Ok(JobsResponse::Ok)
            }
            
            JobsCommand::DeleteSchedule { schedule_id } => {
                log::debug!("JobsStateMachine: DeleteSchedule {}", schedule_id);
                
                {
                    let mut schedules = self.schedules.write();
                    schedules.remove(&schedule_id);
                }
                
                Ok(JobsResponse::Ok)
            }
        }
    }
}

impl Default for JobsStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl KalamStateMachine for JobsStateMachine {
    fn group_id(&self) -> GroupId {
        GroupId::MetaJobs
    }
    
    async fn apply(&self, index: u64, term: u64, command: &[u8]) -> Result<ApplyResult, RaftError> {
        // Idempotency check
        let last_applied = self.last_applied_index.load(Ordering::Acquire);
        if index <= last_applied {
            log::debug!(
                "JobsStateMachine: Skipping already applied entry {} (last_applied={})",
                index, last_applied
            );
            return Ok(ApplyResult::NoOp);
        }
        
        // Deserialize command
        let cmd: JobsCommand = decode(command)?;
        
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
        let jobs = self.jobs.read().clone();
        let schedules = self.schedules.read().clone();
        let snapshot = JobsSnapshot { jobs, schedules };
        
        let data = encode(&snapshot)?;
        
        Ok(StateMachineSnapshot::new(
            self.group_id(),
            self.last_applied_index(),
            self.last_applied_term(),
            data,
        ))
    }
    
    async fn restore(&self, snapshot: StateMachineSnapshot) -> Result<(), RaftError> {
        let data: JobsSnapshot = decode(&snapshot.data)?;
        
        {
            let mut jobs = self.jobs.write();
            *jobs = data.jobs;
        }
        {
            let mut schedules = self.schedules.write();
            *schedules = data.schedules;
        }
        
        self.last_applied_index.store(snapshot.last_applied_index, Ordering::Release);
        self.last_applied_term.store(snapshot.last_applied_term, Ordering::Release);
        
        log::info!(
            "JobsStateMachine: Restored from snapshot at index {}, term {}",
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
    use kalamdb_commons::models::{NamespaceId, TableName};

    #[tokio::test]
    async fn test_jobs_state_machine_create_and_claim() {
        let sm = JobsStateMachine::new();
        
        let job_id = JobId::new("job-001".to_string());
        let node_id = NodeId::new("node-1".to_string());
        
        // Create job
        let create_cmd = JobsCommand::CreateJob {
            job_id: job_id.clone(),
            job_type: "flush".to_string(),
            namespace_id: Some(NamespaceId::new("default")),
            table_name: Some(TableName::new("users")),
            config_json: None,
            created_at: chrono::Utc::now(),
        };
        sm.apply(1, 1, &encode(&create_cmd).unwrap()).await.unwrap();
        
        // Check job created
        {
            let jobs = sm.jobs.read();
            let job = jobs.get(&job_id).unwrap();
            assert_eq!(job.status, "pending");
            assert!(job.claimed_by.is_none());
        }
        
        // Claim job
        let claim_cmd = JobsCommand::ClaimJob {
            job_id: job_id.clone(),
            node_id: node_id.clone(),
            claimed_at: chrono::Utc::now(),
        };
        let result = sm.apply(2, 1, &encode(&claim_cmd).unwrap()).await.unwrap();
        assert!(result.is_ok());
        
        // Check job claimed
        {
            let jobs = sm.jobs.read();
            let job = jobs.get(&job_id).unwrap();
            assert_eq!(job.status, "running");
            assert_eq!(job.claimed_by, Some(node_id));
        }
    }
    
    #[tokio::test]
    async fn test_jobs_state_machine_double_claim() {
        let sm = JobsStateMachine::new();
        
        let job_id = JobId::new("job-002".to_string());
        
        // Create job
        let create_cmd = JobsCommand::CreateJob {
            job_id: job_id.clone(),
            job_type: "flush".to_string(),
            namespace_id: None,
            table_name: None,
            config_json: None,
            created_at: chrono::Utc::now(),
        };
        sm.apply(1, 1, &encode(&create_cmd).unwrap()).await.unwrap();
        
        // First claim
        let claim1 = JobsCommand::ClaimJob {
            job_id: job_id.clone(),
            node_id: NodeId::new("node-1".to_string()),
            claimed_at: chrono::Utc::now(),
        };
        sm.apply(2, 1, &encode(&claim1).unwrap()).await.unwrap();
        
        // Second claim (should return error)
        let claim2 = JobsCommand::ClaimJob {
            job_id: job_id.clone(),
            node_id: NodeId::new("node-2".to_string()),
            claimed_at: chrono::Utc::now(),
        };
        let result = sm.apply(3, 1, &encode(&claim2).unwrap()).await.unwrap();
        
        // Should return an error since job is already claimed
        if let ApplyResult::Ok(data) = result {
            let response: JobsResponse = decode(&data).unwrap();
            assert!(matches!(response, JobsResponse::Error { .. }));
        }
    }
}
