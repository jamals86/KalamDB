//! State Machine implementations for Raft consensus
//!
//! Each Raft group has its own state machine that:
//! - Applies committed log entries to local state
//! - Creates snapshots for log compaction
//! - Restores state from snapshots
//!
//! ## State Machine Types
//!
//! - [`SystemStateMachine`]: Namespaces, tables, storages (MetaSystem group)
//! - [`UsersStateMachine`]: User CRUD operations (MetaUsers group)
//! - [`JobsStateMachine`]: Job lifecycle management (MetaJobs group)
//! - [`UserDataStateMachine`]: Per-user data operations (DataUserShard groups)
//! - [`SharedDataStateMachine`]: Shared table operations (DataSharedShard group)

mod trait_def;
mod serde_helpers;
mod system;
mod users;
mod jobs;
mod user_data;
mod shared_data;

pub use trait_def::{KalamStateMachine, StateMachineSnapshot, ApplyResult};
pub use serde_helpers::{encode, decode};
pub use system::SystemStateMachine;
pub use users::UsersStateMachine;
pub use jobs::JobsStateMachine;
pub use user_data::UserDataStateMachine;
pub use shared_data::SharedDataStateMachine;
