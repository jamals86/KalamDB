//! Error types for the Raft layer

use thiserror::Error;

/// Result type for Raft operations
pub type Result<T> = std::result::Result<T, RaftError>;

/// Errors that can occur in the Raft layer
#[derive(Debug, Error)]
pub enum RaftError {
    /// The node is not the leader for this group
    #[error("Not leader for group {group}: leader is node {leader:?}")]
    NotLeader {
        group: String,
        leader: Option<u64>,
    },

    /// Raft group not found
    #[error("Raft group not found: {0}")]
    GroupNotFound(String),

    /// Invalid Raft group (e.g., invalid shard number)
    #[error("Invalid Raft group: {0}")]
    InvalidGroup(String),

    /// Raft group not started
    #[error("Raft group not started: {0}")]
    NotStarted(String),

    /// Failed to apply command to state machine
    #[error("Failed to apply command: {0}")]
    ApplyFailed(String),

    /// Failed to serialize/deserialize
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Proposal rejected (e.g., not leader, timeout)
    #[error("Proposal rejected: {0}")]
    Proposal(String),

    /// Network error during Raft communication
    #[error("Network error: {0}")]
    Network(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Command timeout
    #[error("Command timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Replication timeout - command committed but not all nodes applied
    #[error("Replication timeout for group {group}: committed at {committed_log_id} but not all nodes applied within {timeout_ms}ms")]
    ReplicationTimeout {
        group: String,
        committed_log_id: String,
        timeout_ms: u64,
    },

    /// Raft is shutting down
    #[error("Raft is shutting down")]
    Shutdown,

    /// Invalid state
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Provider error (from underlying data layer)
    #[error("Provider error: {0}")]
    Provider(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl RaftError {
    /// Create a NotLeader error
    pub fn not_leader(group: impl Into<String>, leader: Option<u64>) -> Self {
        RaftError::NotLeader {
            group: group.into(),
            leader,
        }
    }

    /// Create an ApplyFailed error
    pub fn apply_failed(msg: impl Into<String>) -> Self {
        RaftError::ApplyFailed(msg.into())
    }

    /// Create a Storage error
    pub fn storage(msg: impl Into<String>) -> Self {
        RaftError::Storage(msg.into())
    }

    /// Create a Provider error
    pub fn provider(msg: impl Into<String>) -> Self {
        RaftError::Provider(msg.into())
    }

    /// Returns true if retrying might succeed
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            RaftError::NotLeader { .. } 
            | RaftError::Timeout(_) 
            | RaftError::Network(_)
            | RaftError::ReplicationTimeout { .. }
        )
    }

    /// Returns the leader hint if this is a NotLeader error
    pub fn leader_hint(&self) -> Option<u64> {
        if let RaftError::NotLeader { leader, .. } = self {
            *leader
        } else {
            None
        }
    }
}

impl From<bincode::error::EncodeError> for RaftError {
    fn from(err: bincode::error::EncodeError) -> Self {
        RaftError::Serialization(err.to_string())
    }
}

impl From<bincode::error::DecodeError> for RaftError {
    fn from(err: bincode::error::DecodeError) -> Self {
        RaftError::Serialization(err.to_string())
    }
}

impl From<std::io::Error> for RaftError {
    fn from(err: std::io::Error) -> Self {
        RaftError::Storage(err.to_string())
    }
}
