//! DDL statement definitions shared across KalamDB components.
//!
//! This module consolidates the CREATE and DROP statement parsers so they can be
//! reused without depending on `kalamdb-core`.

pub mod create_namespace;
pub mod create_user_table;
pub mod drop_table;

/// Result type used by the DDL parsers.
pub type DdlResult<T> = anyhow::Result<T>;

pub use create_namespace::CreateNamespaceStatement;
pub use create_user_table::{CreateUserTableStatement, StorageLocation, UserTableFlushPolicy};
pub use drop_table::{DropTableStatement, TableKind};
