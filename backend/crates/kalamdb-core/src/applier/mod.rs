//! Applier implementations for Raft state machine persistence
//!
//! These implementations connect the Raft state machines to the actual
//! provider infrastructure, enabling replicated state across all nodes.

mod provider_system_applier;

pub use provider_system_applier::ProviderSystemApplier;
