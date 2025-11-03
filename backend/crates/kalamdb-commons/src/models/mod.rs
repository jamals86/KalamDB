//! Type-safe wrapper types for KalamDB identifiers and enums.
//!
//! This module provides newtype wrappers around String to enforce type safety
//! at compile time, preventing accidental mixing of user IDs, namespace IDs,
//! and table names.
//!
//! ## System Table Models
//!
//! The `system` submodule contains the SINGLE SOURCE OF TRUTH for all system table models.
//! Import from `kalamdb_commons::system::*` to use these models.
//!
//! ## Examples
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//! use kalamdb_commons::system::{User, Job, LiveQuery};
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

mod audit_log_id;
mod namespace_id;
mod node_id;
mod storage_id;
pub mod system;
mod table_name;
mod user_id;

// Phase 14: Type-safe key models for EntityStore architecture
mod job_id;
mod live_query_id;
mod row_id;
mod table_id;
mod user_name;
mod user_row_id;

// Phase 15 (008-schema-consolidation): Unified schema and type system
pub mod schemas;
pub mod types;

pub use audit_log_id::AuditLogId;
pub use namespace_id::NamespaceId;
pub use node_id::NodeId;
pub use storage_id::StorageId;
pub use table_name::TableName;
pub use user_id::UserId;

// Phase 14: Export new key types
pub use job_id::JobId;
pub use live_query_id::LiveQueryId;
pub use row_id::RowId;
pub use table_id::TableId;
pub use user_name::UserName;
pub use user_row_id::UserRowId;

// Split out previously inlined types into dedicated modules
mod storage_type;
mod job_status;
mod job_type;
mod role;
mod auth_type;
mod storage_mode;
mod table_access;
mod storage_config;
mod connection_id;
mod live_id;

pub use storage_type::StorageType;
pub use job_status::JobStatus;
pub use job_type::JobType;
pub use role::Role;
pub use auth_type::AuthType;
pub use storage_mode::StorageMode;
pub use table_access::TableAccess;
pub use storage_config::StorageConfig;
pub use connection_id::ConnectionId;
pub use live_id::LiveId;

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
