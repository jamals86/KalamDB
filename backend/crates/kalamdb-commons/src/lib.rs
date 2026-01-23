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

pub mod constants;
pub mod conversions; // Centralized datatype and value conversion utilities (see conversions/mod.rs)
pub mod errors;
pub mod helpers;
pub mod ids;
pub mod models;
pub mod storage; // Storage backend abstraction (Partition, StorageError, etc.)
pub mod storage_key; // StorageKey trait for type-safe key serialization
pub mod system_tables; // System table enumeration (SystemTable, StoragePartition)
pub mod websocket;

// Re-export commonly used types at crate root
pub use constants::{ANONYMOUS_USER_ID, MAX_SQL_QUERY_LENGTH, RESERVED_NAMESPACE_NAMES};
pub use conversions::{
    as_f64,
    encode_pk_value,
    estimate_scalar_value_size,
    scalar_to_f64,
    scalar_to_i64,
    scalar_to_pk_string,
    scalar_value_to_bytes,
};
pub use errors::{CommonError, Result};
pub use helpers::arrow_utils;
pub use helpers::arrow_utils::{empty_batch, RecordBatchBuilder};
pub use helpers::file_helpers;
pub use helpers::security;
pub use helpers::string_interner;
pub use models::{
    // Phase 15 (008-schema-consolidation): Re-export schema types
    datatypes,
    schemas,
    system,
    types,
    AuditLogId,
    AuthType,
    JobId,
    JobStatus,
    JobType,
    LiveQueryId,
    ManifestId,
    NamespaceId,
    NodeId,
    Role,
    StorageId,
    StorageMode,
    TableAccess,
    TableId,
    TableName,
    UserId,
    UserName,
};
pub use schemas::TableType;
pub use storage_key::{decode_key, encode_key, encode_prefix, next_storage_key_bytes, StorageKey};
pub use string_interner::{intern, stats as interner_stats, SystemColumns, SYSTEM_COLUMNS};
pub use system_tables::{StoragePartition, SystemTable};
pub use websocket::{ChangeNotification, ChangeType as WsChangeType, Notification, WebSocketMessage};
