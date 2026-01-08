//! Applier traits for state machine persistence
//!
//! These traits define callbacks that state machines invoke when applying
//! committed commands. The actual implementation is provided by kalamdb-core
//! using its provider infrastructure, avoiding circular dependencies.

mod data_applier;
mod system_applier;
mod users_applier;

pub use data_applier::{
    NoOpSharedDataApplier, NoOpUserDataApplier, SharedDataApplier, UserDataApplier,
};
pub use system_applier::{NoOpSystemApplier, SystemApplier};
pub use users_applier::{NoOpUsersApplier, UsersApplier};
