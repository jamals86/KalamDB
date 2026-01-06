//! Applier traits for state machine persistence
//!
//! These traits define callbacks that state machines invoke when applying
//! committed commands. The actual implementation is provided by kalamdb-core
//! using its provider infrastructure, avoiding circular dependencies.

mod system_applier;
mod users_applier;

pub use system_applier::{SystemApplier, NoOpSystemApplier};
pub use users_applier::{NoOpUsersApplier, UsersApplier};
