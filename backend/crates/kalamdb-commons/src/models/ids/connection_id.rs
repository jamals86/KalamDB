use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Unique identifier for WebSocket connections.
///
/// Generated when a WebSocket connection is established, before authentication.
/// Used to track live query subscriptions and route notifications.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", derive(bincode::Encode, bincode::Decode))]
pub struct ConnectionId(String);

impl ConnectionId {
    /// Create a new connection ID from a unique identifier
    #[inline]
    pub fn new(unique_id: impl Into<String>) -> Self {
        Self(unique_id.into())
    }

    /// Parse from string format
    pub fn from_string(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Err("ConnectionId cannot be empty".to_string());
        }
        Ok(Self(s.to_string()))
    }

    /// Get the connection ID as a string slice
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the connection ID as bytes
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
