//! System.namespaces table module (system_namespaces in RocksDB)

pub mod models;
pub mod namespaces_provider;
pub mod namespaces_store;
pub mod namespaces_table;

pub use models::Namespace;
pub use namespaces_provider::NamespacesTableProvider;
pub use namespaces_store::{new_namespaces_store, NamespacesStore};
pub use namespaces_table::NamespacesTableSchema;
