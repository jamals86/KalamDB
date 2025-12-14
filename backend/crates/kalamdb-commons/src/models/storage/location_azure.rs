use serde::{Deserialize, Serialize};

/// Azure Blob Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AzureStorageConfig {
    /// Optional account name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,

    /// Optional access key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_key: Option<String>,

    /// Optional SAS token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sas_token: Option<String>,
}
