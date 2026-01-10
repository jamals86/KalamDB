//! Unified Applier trait and implementations
//!
//! The applier provides a single interface for executing commands,
//! whether in standalone or cluster mode.

use async_trait::async_trait;
use once_cell::sync::OnceCell;
use std::sync::Arc;

use super::commands::{
    CreateTableCommand, AlterTableCommand, DropTableCommand,
    CreateNamespaceCommand, DropNamespaceCommand,
    CreateStorageCommand, DropStorageCommand,
    CreateUserCommand, UpdateUserCommand, DeleteUserCommand,
};
use super::error::ApplierError;
use super::executor::CommandExecutorImpl;
use crate::app_context::AppContext;

/// Information about the current leader (for cluster mode)
#[derive(Debug, Clone)]
pub struct LeaderInfo {
    /// Node ID of the leader
    pub node_id: u64,
    /// Address for forwarding commands
    pub address: String,
}

/// Unified Applier trait - the single interface for all command execution
///
/// This trait is dyn-compatible and provides methods for each command type.
/// Implementations handle the mode-specific logic (standalone vs cluster).
#[async_trait]
pub trait UnifiedApplier: Send + Sync {
    // =========================================================================
    // DDL Operations
    // =========================================================================
    
    /// Create a table
    async fn create_table(&self, cmd: CreateTableCommand) -> Result<String, ApplierError>;
    
    /// Alter a table
    async fn alter_table(&self, cmd: AlterTableCommand) -> Result<String, ApplierError>;
    
    /// Drop a table
    async fn drop_table(&self, cmd: DropTableCommand) -> Result<String, ApplierError>;
    
    // =========================================================================
    // Namespace Operations
    // =========================================================================
    
    /// Create a namespace
    async fn create_namespace(&self, cmd: CreateNamespaceCommand) -> Result<String, ApplierError>;
    
    /// Drop a namespace
    async fn drop_namespace(&self, cmd: DropNamespaceCommand) -> Result<String, ApplierError>;
    
    // =========================================================================
    // Storage Operations
    // =========================================================================
    
    /// Create a storage backend
    async fn create_storage(&self, cmd: CreateStorageCommand) -> Result<String, ApplierError>;
    
    /// Drop a storage backend
    async fn drop_storage(&self, cmd: DropStorageCommand) -> Result<String, ApplierError>;
    
    // =========================================================================
    // User Operations
    // =========================================================================
    
    /// Create a user
    async fn create_user(&self, cmd: CreateUserCommand) -> Result<String, ApplierError>;
    
    /// Update a user
    async fn update_user(&self, cmd: UpdateUserCommand) -> Result<String, ApplierError>;
    
    /// Delete a user (soft delete)
    async fn delete_user(&self, cmd: DeleteUserCommand) -> Result<String, ApplierError>;
    
    // =========================================================================
    // Status Methods
    // =========================================================================
    
    /// Check if this node can accept writes
    fn can_accept_writes(&self) -> bool;
    
    /// Check if we're in cluster mode
    fn is_cluster_mode(&self) -> bool;
    
    /// Get leader info for forwarding (cluster mode only)
    fn get_leader_info(&self) -> Option<LeaderInfo>;
    
    /// Initialize the applier with AppContext (called after AppContext is created)
    fn set_app_context(&self, app_context: Arc<AppContext>);
}

/// Standalone applier - executes commands directly
/// 
/// Uses lazy initialization pattern to avoid circular dependency with AppContext.
/// The executor is created when `set_app_context()` is called.
pub struct StandaloneApplier {
    executor: OnceCell<CommandExecutorImpl>,
}

impl Default for StandaloneApplier {
    fn default() -> Self {
        Self::new()
    }
}

impl StandaloneApplier {
    /// Create a new standalone applier (executor not yet initialized)
    pub fn new() -> Self {
        Self {
            executor: OnceCell::new(),
        }
    }
    
    /// Get reference to the executor (panics if not initialized)
    pub fn executor(&self) -> &CommandExecutorImpl {
        self.executor.get().expect("StandaloneApplier not initialized - call set_app_context first")
    }
    
    /// Check if executor is initialized
    pub fn is_initialized(&self) -> bool {
        self.executor.get().is_some()
    }
}

#[async_trait]
impl UnifiedApplier for StandaloneApplier {
    // DDL Operations
    async fn create_table(&self, cmd: CreateTableCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().ddl().create_table(
            &cmd.table_id,
            cmd.table_type,
            &cmd.table_def,
        ).await
    }
    
    async fn alter_table(&self, cmd: AlterTableCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().ddl().alter_table(
            &cmd.table_id,
            &cmd.table_def,
            cmd.old_version,
        ).await
    }
    
    async fn drop_table(&self, cmd: DropTableCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().ddl().drop_table(&cmd.table_id).await
    }
    
    // Namespace Operations
    async fn create_namespace(&self, cmd: CreateNamespaceCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().namespace().create_namespace(&cmd.namespace_id).await
    }
    
    async fn drop_namespace(&self, cmd: DropNamespaceCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().namespace().drop_namespace(&cmd.namespace_id).await
    }
    
    // Storage Operations
    async fn create_storage(&self, cmd: CreateStorageCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().storage().create_storage(&cmd.storage).await
    }
    
    async fn drop_storage(&self, cmd: DropStorageCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().storage().drop_storage(&cmd.storage_id).await
    }
    
    // User Operations
    async fn create_user(&self, cmd: CreateUserCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().user().create_user(&cmd.user).await
    }
    
    async fn update_user(&self, cmd: UpdateUserCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().user().update_user(&cmd.user).await
    }
    
    async fn delete_user(&self, cmd: DeleteUserCommand) -> Result<String, ApplierError> {
        cmd.validate()?;
        self.executor().user().delete_user(&cmd.user_id).await
    }
    
    // Status Methods
    fn can_accept_writes(&self) -> bool {
        true // Standalone always accepts writes
    }
    
    fn is_cluster_mode(&self) -> bool {
        false
    }
    
    fn get_leader_info(&self) -> Option<LeaderInfo> {
        None // No leader in standalone mode
    }
    
    fn set_app_context(&self, app_context: Arc<AppContext>) {
        if self.executor.set(CommandExecutorImpl::new(app_context)).is_err() {
            log::warn!("StandaloneApplier already initialized; ignoring duplicate set_app_context");
        }
    }
}

// Validation trait implementation for commands
trait CommandValidate {
    fn validate(&self) -> Result<(), ApplierError>;
}

impl CommandValidate for CreateTableCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        if self.table_id.table_name().as_str().is_empty() {
            return Err(ApplierError::Validation("Table name cannot be empty".into()));
        }
        if self.table_id.namespace_id().as_str().is_empty() {
            return Err(ApplierError::Validation("Namespace cannot be empty".into()));
        }
        Ok(())
    }
}

impl CommandValidate for AlterTableCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        if self.table_def.schema_version <= self.old_version {
            return Err(ApplierError::Validation(
                "New version must be greater than old version".into()
            ));
        }
        Ok(())
    }
}

impl CommandValidate for DropTableCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        Ok(())
    }
}

impl CommandValidate for CreateNamespaceCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        if self.namespace_id.as_str().is_empty() {
            return Err(ApplierError::Validation("Namespace name cannot be empty".into()));
        }
        Ok(())
    }
}

impl CommandValidate for DropNamespaceCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        Ok(())
    }
}

impl CommandValidate for CreateStorageCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        if self.storage.storage_id.as_str().is_empty() {
            return Err(ApplierError::Validation("Storage ID cannot be empty".into()));
        }
        Ok(())
    }
}

impl CommandValidate for DropStorageCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        Ok(())
    }
}

impl CommandValidate for CreateUserCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        if self.user.id.as_str().is_empty() {
            return Err(ApplierError::Validation("User ID cannot be empty".into()));
        }
        Ok(())
    }
}

impl CommandValidate for UpdateUserCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        Ok(())
    }
}

impl CommandValidate for DeleteUserCommand {
    fn validate(&self) -> Result<(), ApplierError> {
        Ok(())
    }
}
