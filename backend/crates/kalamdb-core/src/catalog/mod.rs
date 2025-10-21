//! Catalog module for namespace and table metadata management
//!
//! This module manages the catalog of namespaces, tables, and their associated metadata.

pub mod catalog_store;
pub mod namespace;
pub mod namespace_id;
pub mod storage_location;
pub mod table_cache;
pub mod table_metadata;
pub mod table_name;
pub mod table_type;
pub mod user_id;

pub use catalog_store::CatalogStore;
pub use namespace::Namespace;
pub use namespace_id::NamespaceId;
pub use storage_location::{LocationType, StorageLocation};
pub use table_cache::TableCache;
pub use table_metadata::TableMetadata;
pub use table_name::TableName;
pub use table_type::TableType;
pub use user_id::UserId;
