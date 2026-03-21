use serde::{Deserialize, Serialize};

/// Remote server configuration consumed by the PostgreSQL extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 50051,
        }
    }
}
