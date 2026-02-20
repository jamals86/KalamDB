//! System.users table module
//!
//! This module contains all components for the system.users table:
//! - Table schema definition with OnceLock caching
//! - IndexedEntityStore with automatic index management
//! - Secondary indexes for username and role lookup
//! - TableProvider for DataFusion integration

pub mod models;
pub mod users_indexes;
pub mod users_provider;

pub use models::{AuthData, AuthType, OAuthProvider, Role, User, UserName};
pub use users_indexes::{create_users_indexes, UserRoleIndex, UserUsernameIndex};
pub use users_provider::{UsersStore, UsersTableProvider};
