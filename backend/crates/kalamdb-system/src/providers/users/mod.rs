//! System.users table module
//!
//! This module contains all components for the system.users table:
//! - Table schema definition with OnceLock caching
//! - IndexedEntityStore with automatic index management
//! - Secondary indexes for username and role lookup
//! - TableProvider for DataFusion integration

pub mod users_indexes;
pub mod models;
pub mod users_provider;
pub mod users_table;

pub use users_indexes::{create_users_indexes, UserRoleIndex, UserUsernameIndex};
pub use models::{AuthType, Role, User, UserName};
pub use users_provider::{UsersStore, UsersTableProvider};
pub use users_table::UsersTableSchema;
