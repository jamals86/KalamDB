use kalam_pg_common::{EmbeddedRuntimeConfig, KalamPgError};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Parsed foreign-server options for the embedded PostgreSQL extension mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    pub embedded_runtime: EmbeddedRuntimeConfig,
}

impl ServerOptions {
    /// Parse typed embedded server options from raw FDW option pairs.
    pub fn parse(options: &BTreeMap<String, String>) -> Result<Self, KalamPgError> {
        let storage_base_path = options
            .get("storage_base_path")
            .map(PathBuf::from)
            .ok_or_else(|| {
                KalamPgError::Validation(
                    "server option 'storage_base_path' is required in embedded mode".to_string(),
                )
            })?;

        let node_id = options
            .get("node_id")
            .cloned()
            .unwrap_or_else(|| EmbeddedRuntimeConfig::default().node_id);

        Ok(Self {
            embedded_runtime: EmbeddedRuntimeConfig {
                storage_base_path,
                node_id,
            },
        })
    }
}
