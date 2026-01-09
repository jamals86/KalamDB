//! Command and response types for Raft operations
//!
//! Each Raft group has its own command and response types that define
//! the operations it can perform.
//!
//! ## Group Structure
//!
//! - **Meta group**: Unified metadata (namespaces, tables, storages, users, jobs)
//! - **Data groups**: User table shards + shared table shards

mod meta;
mod data;

// Unified Meta commands
pub use meta::{MetaCommand, MetaResponse};

// Data commands
pub use data::{UserDataCommand, SharedDataCommand, DataResponse};
