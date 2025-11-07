//! Storage configuration entity for system.storages table.

use crate::models::ids::StorageId;
use serde::{Deserialize, Serialize};

/// Storage configuration in system_storages table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storage {
    pub storage_id: StorageId, // PK
    pub storage_name: String,
    pub description: Option<String>,
    pub storage_type: String, // "filesystem" or "s3"
    pub base_directory: String,
    #[serde(default)]
    pub credentials: Option<String>,
    pub shared_tables_template: String,
    pub user_tables_template: String,
    pub created_at: i64,
    pub updated_at: i64,
}
