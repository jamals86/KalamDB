//! Command types for unified execution
//!
//! All database mutations are represented as commands in a unified enum.
//! This design allows for dyn-compatibility and easy serialization.

use serde::{Deserialize, Serialize};

use super::error::ApplierError;

/// Result type for command execution
pub type CommandResult<T> = Result<T, ApplierError>;

/// Types of commands for routing to correct Raft group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommandType {
    // DDL Commands (Meta group)
    CreateNamespace,
    DropNamespace,
    CreateTable,
    AlterTable,
    DropTable,
    CreateStorage,
    DropStorage,
    
    // User Management (Meta group)
    CreateUser,
    UpdateUser,
    DeleteUser,
    
    // DML Commands (Data groups - routed by shard)
    Insert,
    Update,
    Delete,
}

impl CommandType {
    /// Check if this command type goes to the meta Raft group
    pub fn is_meta(&self) -> bool {
        matches!(
            self,
            CommandType::CreateNamespace
                | CommandType::DropNamespace
                | CommandType::CreateTable
                | CommandType::AlterTable
                | CommandType::DropTable
                | CommandType::CreateStorage
                | CommandType::DropStorage
                | CommandType::CreateUser
                | CommandType::UpdateUser
                | CommandType::DeleteUser
        )
    }
    
    /// Check if this is a DML command
    pub fn is_dml(&self) -> bool {
        matches!(
            self,
            CommandType::Insert | CommandType::Update | CommandType::Delete
        )
    }
}

/// Trait for command validation
pub trait Validate {
    /// Validate the command before execution
    fn validate(&self) -> CommandResult<()>;
}
