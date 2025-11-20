//! System table models for KalamDB.
//!
//! **CRITICAL**: This module is the SINGLE SOURCE OF TRUTH for all system table models.
//! DO NOT create duplicate model definitions elsewhere in the codebase.
//!
//! This module contains strongly-typed models for all system tables:
//! - `User`: System users (authentication, authorization)
//! - `Job`: Background jobs (flush, retention, cleanup)
//! - `Namespace`: Database namespaces
//! - `SystemTable`: Table metadata registry
//! - `LiveQuery`: Active WebSocket subscriptions
//! - `InformationSchemaTable`: SQL standard table metadata
//! - `UserTableCounter`: Per-user table flush tracking
//!
//! All system table models are serialized with bincode for performance.
//! These models are re-exported at the crate root for easy access.
//!
//! ## Architecture
//!
//! - **Location**: `kalamdb-commons/src/models/types/` (split into individual files)
//! - **Purpose**: Canonical definitions for system table rows
//! - **Serialization**: Bincode (for RocksDB storage) + Serde JSON (for API responses)
//! - **Usage**: Import from `kalamdb_commons::types::*` or via re-exports at crate root
//!
//! ## Example
//!
//! ```rust
//! use kalamdb_commons::types::{User, Job, LiveQuery};
//! use kalamdb_commons::{UserId, Role, AuthType, StorageMode, StorageId, JobType, JobStatus, NamespaceId, TableName};
//!
//! let user = User {
//!     id: UserId::new("u_123"),
//!     username: "alice".into(),
//!     password_hash: "$2b$12$...".to_string(),
//!     role: Role::User,
//!     email: Some("alice@example.com".to_string()),
//!     auth_type: AuthType::Password,
//!     auth_data: None,
//!     storage_mode: StorageMode::Table,
//!     storage_id: Some(StorageId::new("storage_1")),
//!     created_at: 1730000000000,
//!     updated_at: 1730000000000,
//!     last_seen: None,
//!     deleted_at: None,
//! };
//! ```

mod audit_log;
mod job;
mod live_query;
mod manifest;
mod namespace;
mod storage;
mod user;
mod user_table_counter;

pub use audit_log::AuditLogEntry;
pub use job::{Job, JobFilter, JobOptions, JobSortField, SortOrder};
pub use live_query::LiveQuery;
pub use manifest::{
    BatchFileEntry, BatchStatus, ManifestCacheEntry, ManifestFile, RowGroupPruningStats, SyncState,
};
pub use namespace::Namespace;
pub use storage::Storage;
pub use user::User;
pub use user_table_counter::UserTableCounter;

// Re-export for backward compatibility (legacy imports from `system` module)
// Users can now import from either:
// - `kalamdb_commons::types::User` (new)
// - `kalamdb_commons::system::User` (legacy, via crate root re-export)
