//! Health response model

use serde::Serialize;

/// Standard health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Health status: "ok", "degraded", or "unhealthy"
    pub status: String,
    /// Service version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Additional details about health checks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HealthDetails>,
}

/// Detailed health check results for readiness probes
#[derive(Debug, Serialize)]
pub struct HealthDetails {
    /// Database connection status
    pub database: bool,
    /// Raft cluster status (if in cluster mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raft: Option<bool>,
}

impl HealthResponse {
    /// Create a healthy response with just status
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
            version: None,
            details: None,
        }
    }

    /// Create a healthy response with version
    pub fn ok_with_version(version: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            version: Some(version.into()),
            details: None,
        }
    }

    /// Create a response with detailed health checks
    pub fn with_details(database: bool, raft: Option<bool>) -> Self {
        let all_ok = database && raft.unwrap_or(true);
        Self {
            status: if all_ok { "ok" } else { "degraded" }.to_string(),
            version: None,
            details: Some(HealthDetails { database, raft }),
        }
    }
}
