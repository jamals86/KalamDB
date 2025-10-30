//! # kalamdb-commons
//!
//! Shared types, constants, and utilities for KalamDB.
//!
//! This crate provides foundational types and constants used across all KalamDB crates
//! (kalamdb-core, kalamdb-sql, kalamdb-store, kalamdb-api, kalamdb-live). It has zero
//! external dependencies to prevent circular dependency issues.
//!
//! ## Type-Safe Wrappers
//!
//! The crate provides type-safe wrappers for common identifiers:
//! - `UserId`: User identifier wrapper
//! - `NamespaceId`: Namespace identifier wrapper
//! - `TableName`: Table name wrapper
//! - `TableType`: Enum for USER/SHARED/STREAM tables
//!
//! ## System Table Models
//!
//! The `system` module contains the SINGLE SOURCE OF TRUTH for all system table models:
//! - `User`: System users (authentication, authorization)
//! - `Job`: Background jobs (flush, retention, cleanup)
//! - `Namespace`: Database namespaces
//! - `SystemTable`: Table metadata registry
//! - `LiveQuery`: Active WebSocket subscriptions
//! - `InformationSchemaTable`: SQL standard table metadata
//! - `UserTableCounter`: Per-user table flush tracking
//!
//! **CRITICAL**: DO NOT create duplicate model definitions elsewhere in the codebase.
//! Always import from `kalamdb_commons::system::*`.
//!
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//! use kalamdb_commons::system::{User, Job, LiveQuery};
//!
//! let user_id = UserId::new("user_123");
//! let namespace_id = NamespaceId::new("default");
//! let table_name = TableName::new("conversations");
//!
//! // Convert to string
//! let id_str: &str = user_id.as_str();
//! ```

pub mod config;
pub mod constants;
pub mod errors;
pub mod models;
pub mod storage;
pub mod system_tables;
pub mod websocket;

// Re-export commonly used types at crate root
pub use constants::{COLUMN_FAMILIES, SYSTEM_TABLES};
pub use errors::{CommonError, Result};
pub use models::{
    system, AuditLogId, AuthType, JobId, JobStatus, JobType, LiveQueryId, NamespaceId, Role,
    StorageId, StorageMode, TableAccess, TableId, TableName, TableType, UserId, UserName,
};
pub use storage::{Operation, Partition, StorageBackend, StorageError};
pub use system_tables::{StoragePartition, SystemTable};
pub use websocket::{ChangeType as WsChangeType, Notification, WebSocketMessage};
