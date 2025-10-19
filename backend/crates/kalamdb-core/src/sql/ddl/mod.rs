//! DDL (Data Definition Language) statement parsers
//!
//! This module contains parsers for DDL statements like CREATE, ALTER, DROP, and SHOW.

pub mod create_namespace;
pub mod show_namespaces;
pub mod alter_namespace;
pub mod drop_namespace;
pub mod kill_live_query;
pub mod create_user_table;
pub mod create_stream_table;
pub mod drop_table;

pub use create_namespace::CreateNamespaceStatement;
pub use show_namespaces::ShowNamespacesStatement;
pub use alter_namespace::AlterNamespaceStatement;
pub use drop_namespace::DropNamespaceStatement;
pub use kill_live_query::KillLiveQueryStatement;
pub use create_user_table::{CreateUserTableStatement, StorageLocation};
pub use create_stream_table::CreateStreamTableStatement;
pub use drop_table::DropTableStatement;
