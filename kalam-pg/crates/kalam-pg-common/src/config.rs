use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Minimal embedded runtime configuration consumed by the PostgreSQL extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddedRuntimeConfig {
    /// Base directory for embedded state and cold storage files.
    pub storage_base_path: PathBuf,
    /// Explicit node id override used for embedded runtimes.
    pub node_id: String,
}

impl Default for EmbeddedRuntimeConfig {
    fn default() -> Self {
        Self {
            storage_base_path: PathBuf::from("data/embedded"),
            node_id: "pg-embedded".to_string(),
        }
    }
}
