use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::models::{ConnectionId, TableId, UserId};

/// Unique identifier for live query subscriptions.
/// Format: {user_id}-{unique_conn_id}-{namespace}:{table_name}-{query_id}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LiveId {
    pub connection_id: ConnectionId,
    pub table_id: TableId,
    pub query_id: String,
}

impl LiveId {
    pub fn new(
        connection_id: ConnectionId,
        table_id: TableId,
        query_id: impl Into<String>,
    ) -> Self {
        Self {
            connection_id,
            table_id,
            query_id: query_id.into(),
        }
    }

    pub fn from_string(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.splitn(4, '-').collect();
        if parts.len() != 4 {
            return Err(format!(
                "Invalid live_id format: {}. Expected: {{user_id}}-{{unique_conn_id}}-{{namespace}}:{{table_name}}-{{query_id}}",
                s
            ));
        }

        // Parse table_id from namespace:table format OR table-only (backward compat)
        let table_id = if parts[2].contains(':') {
            // New format: namespace:table
            TableId::from_storage_key(parts[2].as_bytes())
                .ok_or_else(|| format!("Invalid table_id format in live_id: {}", parts[2]))?
        } else {
            // Old format: table only (default namespace)
            use crate::models::{NamespaceId, TableName};
            TableId::new(NamespaceId::new("default"), TableName::new(parts[2]))
        };

        Ok(Self {
            connection_id: ConnectionId {
                user_id: UserId::new(parts[0].to_string()),
                unique_conn_id: parts[1].to_string(),
            },
            table_id,
            query_id: parts[3].to_string(),
        })
    }

    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }
    pub fn table_id(&self) -> &TableId {
        &self.table_id
    }
    pub fn query_id(&self) -> &str {
        &self.query_id
    }
    pub fn user_id(&self) -> &str {
        self.connection_id.user_id.as_str()
    }
}

impl fmt::Display for LiveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.connection_id.user_id.as_str(),
            self.connection_id.unique_conn_id,
            self.table_id,
            self.query_id
        )
    }
}
