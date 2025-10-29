//! System.users table module
//!
//! This module contains all components for the system.users table:
//! - Table schema definition with OnceLock caching
//! - SystemTableStore wrapper for type-safe storage
//! - Secondary indexes for username, role, and deleted_at lookups
//! - Index manager to coordinate all indexes
//! - TableProvider for DataFusion integration

pub mod users_deleted_at_index;
pub mod users_index_manager;
pub mod users_provider;
pub mod users_role_index;
pub mod users_store;
pub mod users_table;
pub mod users_username_index;

pub use users_deleted_at_index::DeletedAtIndex;
pub use users_index_manager::UserIndexManager;
pub use users_provider::UsersTableProvider;
pub use users_role_index::RoleIndex;
pub use users_store::{new_users_store, UsersStore};
pub use users_table::UsersTableSchema;
pub use users_username_index::{new_username_index, UsernameIndex, UsernameIndexExt};
