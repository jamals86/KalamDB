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
//! ## Example Usage
//!
//! ```rust
//! use kalamdb_commons::models::{UserId, NamespaceId, TableName};
//!
//! let user_id = UserId::new("user_123");
//! let namespace_id = NamespaceId::new("default");
//! let table_name = TableName::new("conversations");
//!
//! // Convert to string
//! let id_str: &str = user_id.as_str();
//! ```

pub mod models;
pub mod constants;
pub mod errors;
pub mod config;

// Re-export commonly used types at crate root
pub use models::{UserId, NamespaceId, TableName, TableType};
pub use constants::{SYSTEM_TABLES, COLUMN_FAMILIES};
pub use errors::{CommonError, Result};
