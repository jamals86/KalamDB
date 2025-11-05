//! Type-safe wrapper types for KalamDB identifiers and enums.
//!
//! This module provides newtype wrappers around String to enforce type safety
//! at compile time, preventing accidental mixing of user IDs, namespace IDs,
//! and table names.
//!
//! ## System Table Models
//!
//! The `types` submodule contains the SINGLE SOURCE OF TRUTH for all system table models.
//! Import from `kalamdb_commons::types::*` to use these models.
//!
//! ## Examples
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//! use kalamdb_commons::types::{User, Job, LiveQuery};
//!
//! let user_id = UserId::new("user_123");
//! let namespace_id = NamespaceId::new("default");
//! let table_name = TableName::new("conversations");
//!
//! // Type safety prevents mixing
//! // let wrong: UserId = namespace_id; // Compile error!
//!
//! // Conversion to string
//! let id_str: &str = user_id.as_str();
//! let owned: String = user_id.into_string();
//! ```

// Submodules organized into logical groups
pub mod ids;        // Type-safe identifier wrappers
pub mod datatypes;  // Unified data type system (KalamDataType)
pub mod schemas;    // Table and column schema definitions
pub mod types;      // System table models (User, Job, Namespace, etc.)

// Standalone type modules (not IDs, not system tables)
mod audit_log_key;
mod table_name;
mod user_name;
mod storage_type;
mod job_status;
mod job_type;
mod role;
mod auth_type;
mod storage_mode;
mod table_access;
mod storage_config;

// Re-export all types from submodules for convenience
pub use ids::*;
pub use audit_log_key::AuditLogKey;
pub use table_name::TableName;
pub use user_name::UserName;
pub use storage_type::StorageType;
pub use job_status::JobStatus;
pub use job_type::JobType;
pub use role::Role;
pub use auth_type::AuthType;
pub use storage_mode::StorageMode;
pub use table_access::TableAccess;
pub use storage_config::StorageConfig;

// Legacy compatibility: re-export system types as `system` module
// This allows existing code using `kalamdb_commons::system::User` to continue working
pub use types as system;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_type_conversion() {
        assert_eq!(StorageType::from("filesystem"), StorageType::Filesystem);
        assert_eq!(StorageType::from("s3"), StorageType::S3);
        assert_eq!(StorageType::from("S3"), StorageType::S3);
        assert_eq!(StorageType::from("unknown"), StorageType::Filesystem);
    }

    #[test]
    fn test_storage_id_default() {
        let id = StorageId::default();
        assert_eq!(id.as_str(), "local");
    }
}
