//! Applier traits for state machine persistence
//!
//! These traits define callbacks that state machines invoke when applying
//! committed commands. The actual implementation is provided by kalamdb-core
//! using its provider infrastructure, avoiding circular dependencies.
//!
//! ## Structure
//!
//! - **MetaApplier**: Unified applier for all metadata (namespaces, tables, storages, users, jobs)
//! - **UserDataApplier / SharedDataApplier**: Data shard appliers

mod meta_applier;
mod data_applier;

// Unified Meta applier
pub use meta_applier::{MetaApplier, NoOpMetaApplier};

// Data appliers
pub use data_applier::{
    NoOpSharedDataApplier, NoOpUserDataApplier, SharedDataApplier, UserDataApplier,
};
