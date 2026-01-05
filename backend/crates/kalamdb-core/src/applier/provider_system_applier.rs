//! Implementation of SystemApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::SystemApplier`
//! that persists system metadata to the actual providers (RocksDB-backed stores).

use async_trait::async_trait;
use std::sync::Arc;

use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::{NamespaceId, TableId, TableName};
use kalamdb_commons::system::Namespace;
use kalamdb_raft::{RaftError, SystemApplier};
use kalamdb_system::SystemTablesRegistry;

/// SystemApplier implementation using SystemTablesRegistry
///
/// This is called by the Raft state machine when applying committed commands.
/// All nodes (leader and followers) execute this, ensuring consistent state.
pub struct ProviderSystemApplier {
    system_tables: Arc<SystemTablesRegistry>,
}

impl ProviderSystemApplier {
    /// Create a new ProviderSystemApplier
    pub fn new(system_tables: Arc<SystemTablesRegistry>) -> Self {
        Self { system_tables }
    }
}

#[async_trait]
impl SystemApplier for ProviderSystemApplier {
    async fn create_namespace(&self, namespace_id: &str, _created_by: Option<&str>) -> Result<(), RaftError> {
        let namespace = Namespace::new(namespace_id);
        log::info!("ProviderSystemApplier: Creating namespace {}", namespace_id);
        
        self.system_tables
            .namespaces()
            .create_namespace_async(namespace)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to create namespace: {}", e)))?;
        
        Ok(())
    }
    
    async fn delete_namespace(&self, namespace_id: &str) -> Result<(), RaftError> {
        let ns_id = NamespaceId::new(namespace_id);
        log::info!("ProviderSystemApplier: Deleting namespace {}", namespace_id);
        
        self.system_tables
            .namespaces()
            .delete_namespace_async(&ns_id)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to delete namespace: {}", e)))?;
        
        Ok(())
    }
    
    async fn create_table(
        &self,
        namespace_id: &str,
        table_name: &str,
        _table_type: &str,
        schema_json: &str,
    ) -> Result<(), RaftError> {
        let table_id = TableId::new(
            NamespaceId::new(namespace_id),
            TableName::new(table_name),
        );
        
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
        
        log::info!("ProviderSystemApplier: Creating table {}.{}", namespace_id, table_name);
        
        self.system_tables
            .tables()
            .create_table_async(&table_id, &table_def)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to create table: {}", e)))?;
        
        Ok(())
    }
    
    async fn alter_table(
        &self,
        namespace_id: &str,
        table_name: &str,
        schema_json: &str,
    ) -> Result<(), RaftError> {
        let table_id = TableId::new(
            NamespaceId::new(namespace_id),
            TableName::new(table_name),
        );
        
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
        
        log::info!("ProviderSystemApplier: Altering table {}.{}", namespace_id, table_name);
        
        self.system_tables
            .tables()
            .update_table_async(&table_id, &table_def)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to alter table: {}", e)))?;
        
        Ok(())
    }
    
    async fn drop_table(&self, namespace_id: &str, table_name: &str) -> Result<(), RaftError> {
        let table_id = TableId::new(
            NamespaceId::new(namespace_id),
            TableName::new(table_name),
        );
        
        log::info!("ProviderSystemApplier: Dropping table {}.{}", namespace_id, table_name);
        
        self.system_tables
            .tables()
            .delete_table_async(&table_id)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to drop table: {}", e)))?;
        
        Ok(())
    }
    
    async fn register_storage(&self, _storage_id: &str, config_json: &str) -> Result<(), RaftError> {
        let storage: kalamdb_commons::system::Storage = serde_json::from_str(config_json)
            .map_err(|e| RaftError::provider(format!("Invalid storage config: {}", e)))?;
        
        log::info!("ProviderSystemApplier: Registering storage {}", storage.storage_id.as_str());
        
        self.system_tables
            .storages()
            .create_storage_async(storage)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to register storage: {}", e)))?;
        
        Ok(())
    }
    
    async fn unregister_storage(&self, storage_id: &str) -> Result<(), RaftError> {
        let sid = kalamdb_commons::models::StorageId::new(storage_id);
        log::info!("ProviderSystemApplier: Unregistering storage {}", storage_id);
        
        self.system_tables
            .storages()
            .delete_storage_async(&sid)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to unregister storage: {}", e)))?;
        
        Ok(())
    }
}
