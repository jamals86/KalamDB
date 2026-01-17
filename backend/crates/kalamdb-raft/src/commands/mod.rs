//! Command and response types for Raft operations
//!
//! Each Raft group has its own command and response types that define
//! the operations it can perform.
//!
//! ## Group Structure
//!
//! - **Meta group**: Unified metadata (namespaces, tables, storages, users, jobs)
//! - **Data groups**: User table shards + shared table shards

use serde::{Deserialize, Serialize};

mod data;
mod meta;

// Unified Meta commands
pub use meta::{MetaCommand, MetaResponse};

// Data commands
pub use data::{DataResponse, SharedDataCommand, UserDataCommand};

/// Unified command type for all Raft operations
///
/// This wraps specific command types (Meta, UserData, SharedData) into a single
/// enum for type-safe routing and centralized serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftCommand {
    /// Metadata operation (namespaces, tables, users, jobs)
    Meta(MetaCommand),
    /// User table data operation
    UserData(UserDataCommand),
    /// Shared table data operation
    SharedData(SharedDataCommand),
}

/// Unified response type for all Raft operations
///
/// This wraps specific response types into a single enum for type-safe
/// deserialization and centralized handling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftResponse {
    /// Metadata operation response
    Meta(MetaResponse),
    /// Data operation response (user or shared)
    Data(DataResponse),
}
