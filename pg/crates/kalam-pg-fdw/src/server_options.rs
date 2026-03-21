use kalam_pg_common::{KalamPgError, RemoteServerConfig};
use std::collections::BTreeMap;

/// Parsed foreign-server options for the PostgreSQL extension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerOptions {
    pub remote: Option<RemoteServerConfig>,
}

impl ServerOptions {
    /// Parse typed server options from raw FDW option pairs.
    pub fn parse(options: &BTreeMap<String, String>) -> Result<Self, KalamPgError> {
        let host = options
            .get("host")
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                KalamPgError::Validation(
                    "server option 'host' is required".to_string(),
                )
            })?
            .to_string();

        let port = options
            .get("port")
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                KalamPgError::Validation(
                    "server option 'port' is required".to_string(),
                )
            })?
            .parse::<u16>()
            .map_err(|err| {
                KalamPgError::Validation(format!(
                    "server option 'port' must be a valid u16: {}",
                    err
                ))
            })?;

        Ok(Self {
            remote: Some(RemoteServerConfig { host, port }),
        })
    }
}
