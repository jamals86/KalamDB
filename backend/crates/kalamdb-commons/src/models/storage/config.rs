use crate::models::{StorageId, StorageType};

/// Storage configuration for a KalamDB storage backend.
///
/// Defines the connection details and path templates for storing table data.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageConfig {
    /// Unique storage identifier (e.g., "local", "s3-prod")
    pub storage_id: StorageId,
    /// Human-readable name (e.g., "Local Filesystem")
    pub storage_name: String,
    /// Optional description
    pub description: Option<String>,
    /// Type of storage (Filesystem or S3)
    pub storage_type: StorageType,
    /// Base directory or S3 bucket path
    pub base_directory: String,
    /// Optional credentials (e.g., AWS access key JSON)
    pub credentials: Option<String>,
    /// Path template for shared tables: `{base_directory}/{shared_tables_template}/{namespace}/{table_name}`
    pub shared_tables_template: String,
    /// Path template for user tables: `{base_directory}/{user_tables_template}/{namespace}/{table_name}/{user_id}`
    pub user_tables_template: String,
    /// Creation timestamp (Unix milliseconds)
    pub created_at: i64,
    /// Last update timestamp (Unix milliseconds)
    pub updated_at: i64,
}

impl StorageConfig {
    /// Create a new storage configuration
    pub fn new(
        storage_id: impl Into<StorageId>,
        storage_type: StorageType,
        base_directory: impl Into<String>,
        shared_tables_template: impl Into<String>,
        user_tables_template: impl Into<String>,
        credentials: Option<String>,
    ) -> Self {
        Self {
            storage_id: storage_id.into(),
            storage_name: String::new(),
            description: None,
            storage_type,
            base_directory: base_directory.into(),
            credentials,
            shared_tables_template: shared_tables_template.into(),
            user_tables_template: user_tables_template.into(),
            created_at: 0,
            updated_at: 0,
        }
    }

    pub fn storage_id(&self) -> &StorageId {
        &self.storage_id
    }

    pub fn storage_type(&self) -> StorageType {
        self.storage_type
    }

    pub fn base_directory(&self) -> &str {
        &self.base_directory
    }

    pub fn shared_tables_template(&self) -> &str {
        &self.shared_tables_template
    }

    pub fn user_tables_template(&self) -> &str {
        &self.user_tables_template
    }

    pub fn credentials(&self) -> Option<&str> {
        self.credentials.as_deref()
    }

    pub fn is_s3(&self) -> bool {
        matches!(self.storage_type, StorageType::S3)
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_id: StorageId::local(),
            storage_name: "Local Filesystem".to_string(),
            description: Some("Default local filesystem storage".to_string()),
            storage_type: StorageType::Filesystem,
            base_directory: "./data".to_string(),
            credentials: None,
            shared_tables_template: "shared".to_string(),
            user_tables_template: "users".to_string(),
            created_at: 0,
            updated_at: 0,
        }
    }
}
