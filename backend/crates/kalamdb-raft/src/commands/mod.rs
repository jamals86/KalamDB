//! Command and response types for Raft operations
//!
//! Each Raft group has its own command and response types that define
//! the operations it can perform.

// Re-export all command types
mod system;
mod users;
mod jobs;
mod data;

pub use system::{SystemCommand, SystemResponse};
pub use users::{UsersCommand, UsersResponse};
pub use jobs::{JobsCommand, JobsResponse};
pub use data::{UserDataCommand, SharedDataCommand, DataResponse};
