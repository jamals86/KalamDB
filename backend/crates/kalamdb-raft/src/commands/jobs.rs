//! Jobs group commands (background job coordination)

use chrono::{DateTime, Utc};
use kalamdb_commons::models::{JobId, JobType, NamespaceId, NodeId, TableName};
use serde::{Deserialize, Serialize};

/// Commands for the jobs coordination Raft group
///
/// Handles: background jobs (flush, compaction, retention, backup)
/// 
/// Key feature: Leader-only job execution via ClaimJob/ReleaseJob
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobsCommand {
    /// Create a new job
    CreateJob {
        job_id: JobId,
        job_type: JobType,
        namespace_id: Option<NamespaceId>,
        table_name: Option<TableName>,
        config_json: Option<String>,
        created_at: DateTime<Utc>,
    },

    /// Claim a job for execution (leader-only)
    /// 
    /// The node_id indicates which node is running this job.
    /// This ensures only one node executes a job at a time.
    ClaimJob {
        job_id: JobId,
        node_id: NodeId,
        claimed_at: DateTime<Utc>,
    },

    /// Update job status
    UpdateJobStatus {
        job_id: JobId,
        status: String,
        updated_at: DateTime<Utc>,
    },

    /// Complete a job successfully
    CompleteJob {
        job_id: JobId,
        result_json: Option<String>,
        completed_at: DateTime<Utc>,
    },

    /// Fail a job
    FailJob {
        job_id: JobId,
        error_message: String,
        failed_at: DateTime<Utc>,
    },

    /// Release a claimed job (on failure or leader change)
    ReleaseJob {
        job_id: JobId,
        reason: String,
        released_at: DateTime<Utc>,
    },

    /// Cancel a job
    CancelJob {
        job_id: JobId,
        reason: String,
        cancelled_at: DateTime<Utc>,
    },

    /// Create a scheduled job
    CreateSchedule {
        schedule_id: String,
        job_type: JobType,
        cron_expression: String,
        config_json: Option<String>,
        created_at: DateTime<Utc>,
    },

    /// Delete a scheduled job
    DeleteSchedule {
        schedule_id: String,
    },
}

/// Response from job commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum JobsResponse {
    #[default]
    Ok,
    JobCreated {
        job_id: JobId,
    },
    JobClaimed {
        job_id: JobId,
        node_id: NodeId,
    },
    Error {
        message: String,
    },
}

impl JobsResponse {
    /// Create an error response with the given message
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error { message: msg.into() }
    }

    /// Returns true if this is not an error response
    pub fn is_ok(&self) -> bool {
        !matches!(self, Self::Error { .. })
    }
}