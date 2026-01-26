//! Type-safe wrapper types for KalamDB identifiers and enums.
//!
//! This module provides newtype wrappers around String to enforce type safety
//! at compile time, preventing accidental mixing of user IDs, namespace ID,
//! and table names.
//!
//! ## System Table Models
//!
//! System table models live in `kalamdb-system`.
//! Import from `kalamdb_system::*` to use these models.
//!
//! ## Examples
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//! use kalamdb_system::{User, Job, LiveQuery};
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
pub mod datatypes; // Unified data type system (KalamDataType)
pub mod ids; // Type-safe identifier wrappers
pub mod schemas; // Table and column schema definitions
pub mod storage; // Storage backend configuration types

// Standalone type modules (not IDs, not system tables)
mod auth_type;
mod connection;
mod read_context;
mod role;
mod user_name;

// Row types only available with full feature (datafusion dependency)
#[cfg(feature = "full")]
pub mod rows;

// Re-export all types from submodules for convenience
pub use auth_type::AuthType;
pub use ids::*;
pub use read_context::ReadContext;
pub use role::Role;
pub use schemas::{TableAccess, TableName};
pub use user_name::UserName;

#[cfg(feature = "full")]
pub use rows::{KTableRow, StreamTableRow, UserTableRow};

pub use connection::ConnectionInfo;
pub use storage::{
    AzureStorageConfig, GcsStorageConfig, LocalStorageConfig, S3StorageConfig, StorageConfig,
    StorageLocationConfig, StorageLocationConfigError, StorageMode, StorageType,
};


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
