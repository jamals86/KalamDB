//! SystemApplier trait for persisting system metadata
//!
//! This trait is implemented in kalamdb-core to persist namespace, table,
//! and storage changes to the actual providers (RocksDB-backed stores).

use async_trait::async_trait;
use kalamdb_commons::models::schemas::TableType;
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, UserId};

use crate::RaftError;

/// Applier callback for system metadata operations
///
/// This trait is called by SystemStateMachine after Raft consensus commits
/// a command. All nodes (leader and followers) call this, ensuring that
/// all replicas persist the same state to their local storage.
///
/// # Implementation
///
/// The implementation lives in kalamdb-core and uses SystemTablesRegistry
/// to persist changes to the namespace, table, and storage providers.
#[async_trait]
pub trait SystemApplier: Send + Sync {
    /// Create a namespace in persistent storage
    async fn create_namespace(&self, namespace_id: &NamespaceId, created_by: Option<&UserId>) -> Result<(), RaftError>;
    
    /// Delete a namespace from persistent storage
    async fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<(), RaftError>;
    
    /// Create a table in persistent storage
    ///
    /// # Arguments
    /// * `table_id` - TableId containing namespace and table name
    /// * `table_type` - Type of table (User, Shared, Stream)
    /// * `schema_json` - JSON-serialized TableDefinition
    async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        schema_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Alter a table in persistent storage
    async fn alter_table(
        &self,
        table_id: &TableId,
        schema_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Drop a table from persistent storage
    async fn drop_table(&self, table_id: &TableId) -> Result<(), RaftError>;
    
    /// Register storage configuration
    async fn register_storage(&self, storage_id: &StorageId, config_json: &str) -> Result<(), RaftError>;
    
    /// Unregister storage configuration
    async fn unregister_storage(&self, storage_id: &StorageId) -> Result<(), RaftError>;
}

/// No-op applier for testing or standalone scenarios
///
/// Does nothing - used when persistence is handled elsewhere.
pub struct NoOpSystemApplier;

#[async_trait]
impl SystemApplier for NoOpSystemApplier {
    async fn create_namespace(&self, _namespace_id: &NamespaceId, _created_by: Option<&UserId>) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn delete_namespace(&self, _namespace_id: &NamespaceId) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn create_table(
        &self,
        _table_id: &TableId,
        _table_type: TableType,
        _schema_json: &str,
    ) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn alter_table(
        &self,
        _table_id: &TableId,
        _schema_json: &str,
    ) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn drop_table(&self, _table_id: &TableId) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn register_storage(&self, _storage_id: &StorageId, _config_json: &str) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn unregister_storage(&self, _storage_id: &StorageId) -> Result<(), RaftError> {
        Ok(())
    }
}
