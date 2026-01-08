//! Implementation of SystemApplier for provider persistence
//!
//! This module provides the concrete implementation of `kalamdb_raft::SystemApplier`
//! that persists system metadata to the actual providers (RocksDB-backed stores).

use async_trait::async_trait;
use datafusion::catalog::MemorySchemaProvider;
use std::sync::Arc;

use crate::app_context::AppContext;
use crate::sql::executor::helpers::table_registration::{
    register_shared_table_provider, register_stream_table_provider, register_user_table_provider,
};
use kalamdb_commons::models::schemas::{TableDefinition, TableOptions, TableType};
use kalamdb_commons::models::{NamespaceId, StorageId, TableId, UserId};
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

    fn register_namespace_schema(&self, namespace_id: &NamespaceId) -> Result<(), RaftError> {
        let app_ctx = AppContext::get();
        let base_session = app_ctx.base_session_context();
        let catalog = base_session.catalog("kalam").ok_or_else(|| {
            RaftError::provider("Catalog 'kalam' not found in session".to_string())
        })?;

        if catalog.schema(namespace_id.as_str()).is_some() {
            return Ok(());
        }

        let schema_provider = Arc::new(MemorySchemaProvider::new());
        catalog
            .register_schema(namespace_id.as_str(), schema_provider)
            .map_err(|e| {
                RaftError::provider(format!(
                    "Failed to register namespace schema '{}': {}",
                    namespace_id.as_str(),
                    e
                ))
            })?;

        log::info!(
            "ProviderSystemApplier: Registered DataFusion schema for namespace {}",
            namespace_id
        );

        Ok(())
    }

    fn register_table_provider(
        &self,
        table_id: &TableId,
        table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<(), RaftError> {
        let app_ctx = AppContext::get();
        let schema_registry = app_ctx.schema_registry();

        if let Some(existing_provider) = schema_registry.get_provider(table_id) {
            schema_registry
                .insert_provider(table_id.clone(), existing_provider)
                .map_err(|e| {
                    RaftError::provider(format!(
                        "Failed to re-register existing provider: {}",
                        e
                    ))
                })?;
            return Ok(());
        }

        let arrow_schema = table_def
            .to_arrow_schema()
            .map_err(|e| RaftError::provider(format!("Failed to build Arrow schema: {}", e)))?;

        match table_type {
            TableType::User => {
                register_user_table_provider(&app_ctx, table_id, arrow_schema)
                    .map_err(|e| RaftError::provider(format!("Failed to register USER table: {}", e)))
            }
            TableType::Shared => {
                register_shared_table_provider(&app_ctx, table_id, arrow_schema)
                    .map_err(|e| RaftError::provider(format!("Failed to register SHARED table: {}", e)))
            }
            TableType::Stream => {
                let ttl_seconds = match &table_def.table_options {
                    TableOptions::Stream(opts) => Some(opts.ttl_seconds),
                    _ => None,
                };
                register_stream_table_provider(&app_ctx, table_id, arrow_schema, ttl_seconds)
                    .map_err(|e| RaftError::provider(format!("Failed to register STREAM table: {}", e)))
            }
            TableType::System => Ok(()),
        }?;

        Ok(())
    }
}

#[async_trait]
impl SystemApplier for ProviderSystemApplier {
    async fn create_namespace(&self, namespace_id: &NamespaceId, _created_by: Option<&UserId>) -> Result<(), RaftError> {
        let namespace = Namespace::new(namespace_id.as_str());
        log::info!("ProviderSystemApplier: Creating namespace {}", namespace_id);
        
        self.system_tables
            .namespaces()
            .create_namespace_async(namespace)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to create namespace: {}", e)))?;

        self.register_namespace_schema(namespace_id)?;
        
        Ok(())
    }
    
    async fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<(), RaftError> {
        log::info!("ProviderSystemApplier: Deleting namespace {}", namespace_id);
        
        self.system_tables
            .namespaces()
            .delete_namespace_async(namespace_id)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to delete namespace: {}", e)))?;
        
        Ok(())
    }
    
    async fn create_table(
        &self,
        table_id: &TableId,
        _table_type: TableType,
        schema_json: &str,
    ) -> Result<(), RaftError> {
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
        
        log::info!("ProviderSystemApplier: Creating table {}", table_id.full_name());
        
        self.system_tables
            .tables()
            .create_table_async(table_id, &table_def)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to create table: {}", e)))?;

        self.register_table_provider(table_id, table_def.table_type, &table_def)?;
        
        Ok(())
    }
    
    async fn alter_table(
        &self,
        table_id: &TableId,
        schema_json: &str,
    ) -> Result<(), RaftError> {
        let table_def: TableDefinition = serde_json::from_str(schema_json)
            .map_err(|e| RaftError::provider(format!("Invalid schema JSON: {}", e)))?;
        
        log::info!("ProviderSystemApplier: Altering table {}", table_id.full_name());
        
        // Update persistent store
        self.system_tables
            .tables()
            .update_table_async(table_id, &table_def)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to alter table: {}", e)))?;
        
        // Also update schema registry and DataFusion catalog on this node
        // This ensures followers have the updated schema in memory
        let app_ctx = AppContext::get();
        let registry = app_ctx.schema_registry();
        
        // Update registry cache with new table definition
        registry.put_table_definition(table_id, &table_def)
            .map_err(|e| RaftError::provider(format!("Failed to update registry: {}", e)))?;
        
        // Update cached data (preserving storage config)
        {
            use crate::schema_registry::CachedTableData;
            
            let old_entry = registry.get(table_id);
            let new_data = CachedTableData::from_altered_table(
                table_id,
                std::sync::Arc::new(table_def.clone()),
                old_entry.as_deref(),
            ).map_err(|e| RaftError::provider(format!("Failed to create cached data: {}", e)))?;
            
            registry.insert(table_id.clone(), std::sync::Arc::new(new_data));
        }
        
        // Get arrow schema for provider registration
        let arrow_schema = registry.get_arrow_schema(table_id)
            .map_err(|e| RaftError::provider(format!("Failed to get arrow schema: {}", e)))?;
        
        // Unregister old provider and re-register with new schema
        use crate::sql::executor::helpers::table_registration::{
            unregister_table_provider, register_user_table_provider,
            register_shared_table_provider, register_stream_table_provider,
        };
        
        let _ = unregister_table_provider(&app_ctx, table_id);
        
        match table_def.table_type {
            kalamdb_commons::TableType::User => {
                register_user_table_provider(&app_ctx, table_id, arrow_schema)
                    .map_err(|e| RaftError::provider(format!("Failed to re-register user table: {}", e)))?;
            }
            kalamdb_commons::TableType::Shared => {
                register_shared_table_provider(&app_ctx, table_id, arrow_schema)
                    .map_err(|e| RaftError::provider(format!("Failed to re-register shared table: {}", e)))?;
            }
            kalamdb_commons::TableType::Stream => {
                let ttl_seconds = if let kalamdb_commons::schemas::TableOptions::Stream(opts) = &table_def.table_options {
                    Some(opts.ttl_seconds)
                } else {
                    None
                };
                register_stream_table_provider(&app_ctx, table_id, arrow_schema, ttl_seconds)
                    .map_err(|e| RaftError::provider(format!("Failed to re-register stream table: {}", e)))?;
            }
            kalamdb_commons::TableType::System => {
                // System tables are not altered this way
            }
        }
        
        log::info!("ProviderSystemApplier: Altered table {} and updated schema registry", table_id.full_name());
        
        Ok(())
    }
    
    async fn drop_table(&self, table_id: &TableId) -> Result<(), RaftError> {
        log::info!("ProviderSystemApplier: Dropping table {}", table_id.full_name());
        
        self.system_tables
            .tables()
            .delete_table_async(table_id)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to drop table: {}", e)))?;

        let app_ctx = AppContext::get();
        let _ = app_ctx.schema_registry().remove_provider(table_id);
        
        Ok(())
    }
    
    async fn register_storage(&self, _storage_id: &StorageId, config_json: &str) -> Result<(), RaftError> {
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
    
    async fn unregister_storage(&self, storage_id: &StorageId) -> Result<(), RaftError> {
        log::info!("ProviderSystemApplier: Unregistering storage {}", storage_id);
        
        self.system_tables
            .storages()
            .delete_storage_async(storage_id)
            .await
            .map_err(|e| RaftError::provider(format!("Failed to unregister storage: {}", e)))?;
        
        Ok(())
    }
}
