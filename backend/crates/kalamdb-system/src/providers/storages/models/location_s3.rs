use serde::{Deserialize, Serialize};

/// S3 (or S3-compatible) storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct S3StorageConfig {
    /// AWS region (e.g., "us-east-1").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// Optional custom endpoint for S3-compatible providers.
    /// Example: "http://localhost:9000" (MinIO).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// If true, allow plain HTTP for custom endpoints.
    #[serde(default)]
    pub allow_http: bool,

    /// Optional explicit access key id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_key_id: Option<String>,

    /// Optional explicit secret access key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,

    /// Optional session token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}
