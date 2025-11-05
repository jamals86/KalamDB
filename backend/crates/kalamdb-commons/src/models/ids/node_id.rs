//! Node identifier type for cluster deployments
//!
//! Each KalamDB server instance has a unique node ID used for:
//! - Job assignment and tracking
//! - Live query routing
//! - Distributed coordination

use std::fmt;
use serde::{Deserialize, Serialize};
use bincode::{Decode, Encode};

/// Node identifier for cluster deployments
///
/// Configured via config.toml `[server] node_id = "node1"`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode)]
pub struct NodeId(String);

impl NodeId {
    /// Create a new node ID
    pub fn new(id: String) -> Self {
        Self(id)
    }

    /// Get the node ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Create default node ID ("node1")
    pub fn default_node() -> Self {
        Self("node1".to_string())
    }
}

impl AsRef<str> for NodeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::default_node()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_creation() {
        let node_id = NodeId::new("test_node".to_string());
        assert_eq!(node_id.as_str(), "test_node");
    }

    #[test]
    fn test_node_id_display() {
        let node_id = NodeId::new("node123".to_string());
        assert_eq!(format!("{}", node_id), "node123");
    }

    #[test]
    fn test_node_id_from_str() {
        let node_id = NodeId::from("node456");
        assert_eq!(node_id.as_str(), "node456");
    }

    #[test]
    fn test_node_id_default() {
        let node_id = NodeId::default();
        assert_eq!(node_id.as_str(), "node1");
    }

    #[test]
    fn test_node_id_equality() {
        let node1 = NodeId::new("node1".to_string());
        let node2 = NodeId::new("node1".to_string());
        let node3 = NodeId::new("node2".to_string());
        
        assert_eq!(node1, node2);
        assert_ne!(node1, node3);
    }
}
