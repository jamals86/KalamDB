use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::UserId;

/// Unique identifier for WebSocket connections.
///
/// Used to track live query subscriptions and route notifications.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConnectionId {
    pub user_id: UserId,
    pub unique_conn_id: String,
}

impl ConnectionId {
    /// Create a new connection ID
    pub fn new(user_id: String, unique_conn_id: String) -> Self {
        Self {
            user_id: UserId::new(user_id),
            unique_conn_id,
        }
    }

    /// Parse from string format: {user_id}-{unique_conn_id}
    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid connection_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}",
                s
            ));
        }
        Ok(Self {
            user_id: UserId::new(parts[0].to_string()),
            unique_conn_id: parts[1].to_string(),
        })
    }

    pub fn user_id(&self) -> &str {
        self.user_id.as_str()
    }
    pub fn unique_conn_id(&self) -> &str {
        &self.unique_conn_id
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.user_id.as_str(), self.unique_conn_id)
    }
}
