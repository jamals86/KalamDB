//! Cluster Types
//!
//! Re-export OpenRaft's ServerState and provide a NodeStatus enum for node health tracking.

use openraft::ServerState;
use serde::{Deserialize, Serialize};
use std::fmt;

// Re-export OpenRaft's ServerState as NodeRole for consistency
// ServerState has: Leader, Follower, Learner, Candidate, Shutdown
pub use openraft::ServerState as NodeRole;

/// Helper trait to convert ServerState to string
pub trait ServerStateExt {
    /// Convert to lowercase string representation
    fn as_str(&self) -> &'static str;
}

impl ServerStateExt for ServerState {
    fn as_str(&self) -> &'static str {
        match self {
            ServerState::Leader => "leader",
            ServerState::Follower => "follower",
            ServerState::Learner => "learner",
            ServerState::Candidate => "candidate",
            ServerState::Shutdown => "shutdown",
        }
    }
}

/// Node status in the cluster
///
/// Indicates the health and connectivity state of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    /// Node is active and responsive
    Active,
    /// Node is offline or unreachable
    Offline,
    /// Node is joining the cluster (learner catching up)
    Joining,
    /// Node status is unknown (no metrics available)
    Unknown,
}

impl NodeStatus {
    /// Create from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "active" => NodeStatus::Active,
            "offline" => NodeStatus::Offline,
            "joining" => NodeStatus::Joining,
            "unknown" => NodeStatus::Unknown,
            _ => NodeStatus::Unknown,
        }
    }

    /// Convert to string for display
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeStatus::Active => "active",
            NodeStatus::Offline => "offline",
            NodeStatus::Joining => "joining",
            NodeStatus::Unknown => "unknown",
        }
    }
}

impl fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for NodeStatus {
    fn default() -> Self {
        NodeStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_state_as_str() {
        assert_eq!(ServerState::Leader.as_str(), "leader");
        assert_eq!(ServerState::Follower.as_str(), "follower");
        assert_eq!(ServerState::Learner.as_str(), "learner");
        assert_eq!(ServerState::Candidate.as_str(), "candidate");
        assert_eq!(ServerState::Shutdown.as_str(), "shutdown");
    }

    #[test]
    fn test_node_status_from_str() {
        assert_eq!(NodeStatus::from_str("active"), NodeStatus::Active);
        assert_eq!(NodeStatus::from_str("ACTIVE"), NodeStatus::Active);
        assert_eq!(NodeStatus::from_str("offline"), NodeStatus::Offline);
        assert_eq!(NodeStatus::from_str("joining"), NodeStatus::Joining);
        assert_eq!(NodeStatus::from_str("unknown"), NodeStatus::Unknown);
        assert_eq!(NodeStatus::from_str("random"), NodeStatus::Unknown);
    }

    #[test]
    fn test_node_status_as_str() {
        assert_eq!(NodeStatus::Active.as_str(), "active");
        assert_eq!(NodeStatus::Offline.as_str(), "offline");
        assert_eq!(NodeStatus::Joining.as_str(), "joining");
        assert_eq!(NodeStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_node_status_display() {
        assert_eq!(format!("{}", NodeStatus::Active), "active");
        assert_eq!(format!("{}", NodeStatus::Offline), "offline");
    }
}
