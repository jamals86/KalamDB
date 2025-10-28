//! Domain models for KalamDB.
//!
//! This module provides strongly-typed models for KalamDB entities.
//!
//! ## System Tables (imported from `kalamdb_commons::system`)
//! - `User`: System users with authentication credentials
//! - `Job`: Background jobs (flush, retention, cleanup)
//! - `Namespace`: Database namespaces
//! - `SystemTable`: Table metadata registry
//! - `LiveQuery`: Active WebSocket subscriptions
//! - `InformationSchemaTable`: SQL standard table metadata
//! - `UserTableCounter`: Per-user table flush tracking
//!
//! **CRITICAL**: All system table models are now defined in `kalamdb_commons::system`.
//! This is the SINGLE SOURCE OF TRUTH. DO NOT create duplicates.
//!
//! ## Table Rows (in `tables.rs`)
//! - `UserTableRow`: Rows in user tables (user-owned data)
//! - `SharedTableRow`: Rows in shared tables (cross-user data)
//! - `StreamTableRow`: Rows in stream tables (ephemeral events)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use kalamdb_core::models::{User, Job, Namespace, UserTableRow};
//!
//! // System table models (re-exported from kalamdb_commons)
//! let user = User { ... };
//! let job = Job { ... };
//! let namespace = Namespace { ... };
//!
//! // Table row models
//! let row = UserTableRow { ... };
//! ```

pub mod tables;

// Re-export system table models from kalamdb_commons (SINGLE SOURCE OF TRUTH)
pub use kalamdb_commons::system::{
    InformationSchemaTable, Job, LiveQuery, Namespace, SystemTable, User, UserTableCounter,
};

// Re-export table row models
pub use tables::{SharedTableRow, StreamTableRow, UserTableRow};
