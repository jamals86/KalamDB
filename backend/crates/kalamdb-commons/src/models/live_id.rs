use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::models::{ConnectionId, UserId};

/// Unique identifier for live query subscriptions.
/// Format: {user_id}-{unique_conn_id}-{table_name}-{query_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LiveId {
    pub connection_id: ConnectionId,
    pub table_name: String, //TODO: Use TableName
    pub query_id: String,
}

impl LiveId {
    pub fn new(connection_id: ConnectionId, table_name: String, query_id: String) -> Self {
        Self { connection_id, table_name, query_id }
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(4, '-').collect();
        if parts.len() != 4 {
            return Err(format!(
                "Invalid live_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}-{{table_name}}-{{query_id}}",
                s
            ));
        }
        Ok(Self {
            connection_id: ConnectionId { user_id: UserId::new(parts[0].to_string()), unique_conn_id: parts[1].to_string() },
            table_name: parts[2].to_string(),
            query_id: parts[3].to_string(),
        })
    }

    pub fn connection_id(&self) -> &ConnectionId { &self.connection_id }
    pub fn table_name(&self) -> &str { &self.table_name }
    pub fn query_id(&self) -> &str { &self.query_id }
    pub fn user_id(&self) -> &str { self.connection_id.user_id.as_str() }
}

impl fmt::Display for LiveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.connection_id.user_id.as_str(),
            self.connection_id.unique_conn_id,
            self.table_name,
            self.query_id
        )
    }
}
