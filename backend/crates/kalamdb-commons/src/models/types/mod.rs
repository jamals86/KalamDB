//! System table models for KalamDB.
//!
//! **CRITICAL**: This module is the SINGLE SOURCE OF TRUTH for all system table models.
//! DO NOT create duplicate model definitions elsewhere in the codebase.
//!
//! This module contains strongly-typed models for all system tables:
//! - `User`: System users (authentication, authorization)
//! - `Job`: Background jobs (flush, retention, cleanup) - MOVED to kalamdb-system
//! - `Namespace`: Database namespaces
//! - `SystemTable`: Table metadata registry
//! - `LiveQuery`: Active WebSocket subscriptions - MOVED to kalamdb-system
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
//! use kalamdb_commons::types::{User, Namespace, Storage};
//! use kalamdb_commons::{UserId, Role, AuthType, StorageMode, StorageId, NamespaceId};
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
//!     failed_login_attempts: 0,
//!     locked_until: None,
//!     last_login_at: None,
//!     created_at: 1730000000000,
//!     updated_at: 1730000000000,
//!     last_seen: None,
//!     deleted_at: None,
//! };
//! ```

mod audit_log;
mod file_ref;
mod job_node;
mod manifest;
mod namespace;
mod storage;
mod user;

pub use audit_log::AuditLogEntry;
pub use file_ref::{FileRef, FileSubfolderState};
pub use job_node::JobNode;
pub use manifest::{ColumnStats, Manifest, ManifestCacheEntry, SegmentMetadata, SegmentStatus, SyncState};
pub use namespace::Namespace;
pub use storage::Storage;
pub use user::{User, DEFAULT_LOCKOUT_DURATION_MINUTES, DEFAULT_MAX_FAILED_ATTEMPTS};
