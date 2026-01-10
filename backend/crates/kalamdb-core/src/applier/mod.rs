//! Unified Command Applier
//!
//! This module provides a unified execution path for all database commands,
//! regardless of standalone or cluster mode. The key principle:
//!
//! **"All commands flow through the Applier, regardless of mode."**
//!
//! ## Architecture
//!
//! - [`UnifiedApplier`]: Trait for applying commands (dyn-compatible)
//! - [`StandaloneApplier`]: Direct execution for single-node mode
//! - [`ClusterApplier`]: Routes through Raft consensus for cluster mode
//! - [`CommandExecutorImpl`]: Single place where all mutations happen
//! - [`commands`]: Command types for each operation
//! - [`raft`]: Raft state machine applier implementations
//!
//! ## Key Invariants
//!
//! 1. **Single Mutation Point**: All mutations in `CommandExecutorImpl`
//! 2. **No Code Duplication**: Logic exists in exactly one place
//! 3. **No Mode Branching**: Zero `is_cluster_mode()` in handlers
//! 4. **OpenRaft Quorum**: We trust OpenRaft for consensus

mod applier;
mod cluster;
mod command;
pub mod commands;
mod error;
pub mod executor;
mod forwarder;

// Events module
pub mod events;

// Raft applier implementations (traits defined in kalamdb-raft)
pub mod raft;

// Re-exports
pub use applier::{LeaderInfo, StandaloneApplier, UnifiedApplier};
pub use cluster::ClusterApplier;
pub use command::{CommandResult, CommandType, Validate};
pub use error::ApplierError;
pub use executor::CommandExecutorImpl;
pub use forwarder::CommandForwarder;

// Re-export Raft appliers
pub use raft::{ProviderMetaApplier, ProviderSharedDataApplier, ProviderUserDataApplier};

use std::sync::Arc;

/// Create the appropriate applier based on configuration
///
/// The applier is created without AppContext - call `set_app_context()`
/// after AppContext is fully initialized.
///
/// - Standalone mode (no cluster config): Returns `StandaloneApplier`
/// - Cluster mode: Returns `ClusterApplier`
pub fn create_applier(is_cluster_mode: bool) -> Arc<dyn UnifiedApplier> {
    if is_cluster_mode {
        log::info!("Creating ClusterApplier for cluster mode");
        Arc::new(ClusterApplier::new())
    } else {
        log::debug!("Creating StandaloneApplier for standalone mode");
        Arc::new(StandaloneApplier::new())
    }
}
