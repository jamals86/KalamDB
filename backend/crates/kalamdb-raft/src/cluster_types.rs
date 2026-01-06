//! Cluster Types
//!
//! Type-safe enums for cluster node roles and status, derived from OpenRaft metrics.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Node role in the Raft cluster
///
/// Derived from OpenRaft's ServerState:
/// - Leader: Currently leading and can accept write requests
/// - Follower: Following a leader, can serve reads (if linearizable reads enabled)
/// - Learner: Non-voting member, catching up with the log
/// - Candidate: Attempting to become leader (during election)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    /// Currently the leader of the Raft group
    Leader,
    /// Following the leader
    Follower,
    /// Non-voting member (learner/replica catching up)
    Learner,
    /// Attempting to become leader
    Candidate,
}

impl NodeRole {
    /// Create from OpenRaft ServerState string representation
    pub fn from_server_state_str(state: &str) -> Self {
        match state.to_lowercase().as_str() {
            "leader" => NodeRole::Leader,
            "follower" => NodeRole::Follower,
            "learner" => NodeRole::Learner,
            "candidate" => NodeRole::Candidate,
            _ => NodeRole::Follower, // Default to follower if unknown
        }
    }

    /// Convert to string for display
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeRole::Leader => "leader",
            NodeRole::Follower => "follower",
            NodeRole::Learner => "learner",
            NodeRole::Candidate => "candidate",
        }
    }
}

impl fmt::Display for NodeRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Default for NodeRole {
    fn default() -> Self {
        NodeRole::Follower
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
    fn test_node_role_from_server_state() {
        assert_eq!(NodeRole::from_server_state_str("leader"), NodeRole::Leader);
        assert_eq!(NodeRole::from_server_state_str("LEADER"), NodeRole::Leader);
        assert_eq!(NodeRole::from_server_state_str("follower"), NodeRole::Follower);
        assert_eq!(NodeRole::from_server_state_str("learner"), NodeRole::Learner);
        assert_eq!(NodeRole::from_server_state_str("candidate"), NodeRole::Candidate);
        assert_eq!(NodeRole::from_server_state_str("unknown"), NodeRole::Follower);
    }

    #[test]
    fn test_node_role_as_str() {
        assert_eq!(NodeRole::Leader.as_str(), "leader");
        assert_eq!(NodeRole::Follower.as_str(), "follower");
        assert_eq!(NodeRole::Learner.as_str(), "learner");
        assert_eq!(NodeRole::Candidate.as_str(), "candidate");
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
    fn test_node_role_display() {
        assert_eq!(format!("{}", NodeRole::Leader), "leader");
        assert_eq!(format!("{}", NodeRole::Follower), "follower");
    }

    #[test]
    fn test_node_status_display() {
        assert_eq!(format!("{}", NodeStatus::Active), "active");
        assert_eq!(format!("{}", NodeStatus::Offline), "offline");
    }
}
