//! DDL (Data Definition Language) statement parsers
//!
//! This module contains parsers for DDL statements like CREATE, ALTER, DROP, and SHOW.

pub mod create_namespace;
pub mod show_namespaces;
pub mod alter_namespace;
pub mod drop_namespace;

pub use create_namespace::CreateNamespaceStatement;
pub use show_namespaces::ShowNamespacesStatement;
pub use alter_namespace::AlterNamespaceStatement;
pub use drop_namespace::DropNamespaceStatement;
