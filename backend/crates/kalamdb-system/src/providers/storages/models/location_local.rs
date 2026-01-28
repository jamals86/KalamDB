use serde::{Deserialize, Serialize};

/// Local filesystem storage configuration.
///
/// Note: For local storage, KalamDB still uses regular paths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LocalStorageConfig {
    /// Optional root directory hint.
    ///
    /// This is informational for now; the effective base directory is still taken
    /// from `system.storages.base_directory` for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}
