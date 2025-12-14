use serde::{Deserialize, Serialize};

/// Google Cloud Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GcsStorageConfig {
    /// Optional service account JSON.
    ///
    /// If omitted, the object_store implementation may fall back to ADC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_account_json: Option<String>,
}
