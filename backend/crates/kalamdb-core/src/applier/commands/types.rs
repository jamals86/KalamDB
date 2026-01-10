//! Command Type Definitions
//!
//! Simple struct-based commands without complex traits.

use serde::{Deserialize, Serialize};

use kalamdb_commons::models::schemas::{TableDefinition, TableType};
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, UserId};
use kalamdb_commons::system::Storage;
use kalamdb_commons::types::User;

// =============================================================================
// DDL Commands
// =============================================================================

/// Create Table Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTableCommand {
    pub table_id: TableId,
    pub table_type: TableType,
    pub table_def: TableDefinition,
}

/// Alter Table Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlterTableCommand {
    pub table_id: TableId,
    pub table_def: TableDefinition,
    pub old_version: u32,
}

/// Drop Table Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropTableCommand {
    pub table_id: TableId,
}

// =============================================================================
// Namespace Commands
// =============================================================================

/// Create Namespace Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNamespaceCommand {
    pub namespace_id: NamespaceId,
}

/// Drop Namespace Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropNamespaceCommand {
    pub namespace_id: NamespaceId,
}

// =============================================================================
// Storage Commands
// =============================================================================

/// Create Storage Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStorageCommand {
    pub storage: Storage,
}

/// Drop Storage Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropStorageCommand {
    pub storage_id: StorageId,
}

// =============================================================================
// User Commands
// =============================================================================

/// Create User Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserCommand {
    pub user: User,
}

/// Update User Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserCommand {
    pub user: User,
}

/// Delete User Command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteUserCommand {
    pub user_id: UserId,
}
