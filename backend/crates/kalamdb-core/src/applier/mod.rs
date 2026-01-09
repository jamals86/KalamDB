//! Applier implementations for Raft state machine persistence
//!
//! These implementations connect the Raft state machines to the actual
//! provider infrastructure, enabling replicated state across all nodes.

mod provider_meta_applier;
mod provider_shared_data_applier;
mod provider_user_data_applier;

pub use provider_meta_applier::ProviderMetaApplier;
pub use provider_shared_data_applier::ProviderSharedDataApplier;
pub use provider_user_data_applier::ProviderUserDataApplier;
