//! State Machine implementations for Raft consensus
//!
//! Each Raft group has its own state machine that:
//! - Applies committed log entries to local state
//! - Creates snapshots for log compaction
//! - Restores state from snapshots
//!
//! ## State Machine Types
//!
//! - [`MetaStateMachine`]: Unified metadata (namespaces, tables, storages, users, jobs)
//! - [`UserDataStateMachine`]: Per-user data operations (DataUserShard groups)
//! - [`SharedDataStateMachine`]: Shared table operations (DataSharedShard group)
//!
//! ## Watermark Coordination
//!
//! - [`PendingBuffer`]: Buffers data commands until Meta catches up
//! - [`MetadataCoordinator`]: Broadcasts meta-advanced events to data shards

mod trait_def;
pub mod serde_helpers;
mod meta;
mod user_data;
mod shared_data;
mod pending_buffer;
mod meta_coordinator;

// Re-export serialization helpers for convenience
pub use serde_helpers::{encode, decode};

pub use trait_def::{KalamStateMachine, StateMachineSnapshot, ApplyResult};

// Unified Meta state machine
pub use meta::MetaStateMachine;

// Data state machines
pub use user_data::UserDataStateMachine;
pub use shared_data::SharedDataStateMachine;

// Watermark coordination
pub use pending_buffer::{PendingBuffer, PendingCommand};
pub use meta_coordinator::{MetadataCoordinator, get_coordinator, init_coordinator};
