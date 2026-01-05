//! SystemApplier trait for persisting system metadata
//!
//! This trait is implemented in kalamdb-core to persist namespace, table,
//! and storage changes to the actual providers (RocksDB-backed stores).

use async_trait::async_trait;

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
    async fn create_namespace(&self, namespace_id: &str, created_by: Option<&str>) -> Result<(), RaftError>;
    
    /// Delete a namespace from persistent storage
    async fn delete_namespace(&self, namespace_id: &str) -> Result<(), RaftError>;
    
    /// Create a table in persistent storage
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace containing the table
    /// * `table_name` - Name of the table
    /// * `table_type` - Type of table (USER, SHARED, STREAM)
    /// * `schema_json` - JSON-serialized TableDefinition
    async fn create_table(
        &self,
        namespace_id: &str,
        table_name: &str,
        table_type: &str,
        schema_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Alter a table in persistent storage
    async fn alter_table(
        &self,
        namespace_id: &str,
        table_name: &str,
        schema_json: &str,
    ) -> Result<(), RaftError>;
    
    /// Drop a table from persistent storage
    async fn drop_table(&self, namespace_id: &str, table_name: &str) -> Result<(), RaftError>;
    
    /// Register storage configuration
    async fn register_storage(&self, storage_id: &str, config_json: &str) -> Result<(), RaftError>;
    
    /// Unregister storage configuration
    async fn unregister_storage(&self, storage_id: &str) -> Result<(), RaftError>;
}

/// No-op applier for testing or standalone scenarios
///
/// Does nothing - used when persistence is handled elsewhere.
pub struct NoOpSystemApplier;

#[async_trait]
impl SystemApplier for NoOpSystemApplier {
    async fn create_namespace(&self, _namespace_id: &str, _created_by: Option<&str>) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn delete_namespace(&self, _namespace_id: &str) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn create_table(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        _table_type: &str,
        _schema_json: &str,
    ) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn alter_table(
        &self,
        _namespace_id: &str,
        _table_name: &str,
        _schema_json: &str,
    ) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn drop_table(&self, _namespace_id: &str, _table_name: &str) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn register_storage(&self, _storage_id: &str, _config_json: &str) -> Result<(), RaftError> {
        Ok(())
    }
    
    async fn unregister_storage(&self, _storage_id: &str) -> Result<(), RaftError> {
        Ok(())
    }
}
