//! System.users table module
//!
//! This module contains all components for the system.users table:
//! - Table schema definition with OnceLock caching
//! - SystemTableStore wrapper for type-safe storage
//! - Secondary index for username lookups
//! - TableProvider for DataFusion integration

pub mod users_provider;
pub mod users_store;
pub mod users_table;
pub mod users_username_index;

pub use users_provider::UsersTableProvider;
pub use users_store::{new_users_store, UsersStore};
pub use users_table::UsersTableSchema;
pub use users_username_index::{new_username_index, UsernameIndex, UsernameIndexExt};
