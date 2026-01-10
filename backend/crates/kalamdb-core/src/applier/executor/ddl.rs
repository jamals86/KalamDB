//! DDL Executor - CREATE/ALTER/DROP TABLE operations
//!
//! This is the SINGLE place where table mutations happen.

use std::sync::Arc;

use kalamdb_commons::models::schemas::TableDefinition;
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::TableType;

use crate::app_context::AppContext;
use crate::schema_registry::{CachedTableData, PathResolver};
use crate::sql::executor::helpers::table_registration;
use crate::applier::ApplierError;

/// Executor for DDL (Data Definition Language) operations
pub struct DdlExecutor {
    app_context: Arc<AppContext>,
}

impl DdlExecutor {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
    
    /// Execute CREATE TABLE
    ///
    /// This performs ALL the steps for table creation:
    /// 1. Persist to system.tables
    /// 2. Prime schema cache
    /// 3. Register DataFusion provider
    /// 4. Emit TableCreated event
    pub async fn create_table(
        &self,
        table_id: &TableId,
        table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<String, ApplierError> {
        log::info!(
            "CommandExecutorImpl: Creating {} table {}",
            table_type,
            table_id.full_name()
        );
        
        // 1. Persist to system.tables
        self.app_context
            .system_tables()
            .tables()
            .create_table(table_id, table_def)
            .map_err(|e| ApplierError::Execution(format!("Failed to create table: {}", e)))?;
        
        // 2. Prime the schema cache
        self.prime_schema_cache(table_id, table_type, table_def)?;
        
        // 3. Register the DataFusion provider
        self.register_table_provider(table_id, table_type, table_def)?;
        
        Ok(format!(
            "{} table {}.{} created successfully",
            table_type,
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        ))
    }
    
    /// Execute ALTER TABLE
    pub async fn alter_table(
        &self,
        table_id: &TableId,
        table_def: &TableDefinition,
        old_version: u32,
    ) -> Result<String, ApplierError> {
        log::info!(
            "CommandExecutorImpl: Altering table {} (version {} -> {})",
            table_id.full_name(),
            old_version,
            table_def.schema_version
        );
        
        // 1. Update in system.tables
        self.app_context
            .system_tables()
            .tables()
            .update_table(table_id, table_def)
            .map_err(|e| ApplierError::Execution(format!("Failed to alter table: {}", e)))?;
        
        // 2. Update schema cache
        let table_type = table_def.table_type;
        self.prime_schema_cache(table_id, table_type, table_def)?;
        
        // 3. Re-register provider with new schema
        self.register_table_provider(table_id, table_type, table_def)?;
        
        Ok(format!(
            "Table {}.{} altered successfully (version {} -> {})",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str(),
            old_version,
            table_def.schema_version
        ))
    }
    
    /// Execute DROP TABLE
    pub async fn drop_table(&self, table_id: &TableId) -> Result<String, ApplierError> {
        log::info!("CommandExecutorImpl: Dropping table {}", table_id.full_name());
        
        // 1. Remove from system.tables
        self.app_context
            .system_tables()
            .tables()
            .delete_table(table_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to drop table: {}", e)))?;
        
        // 2. Remove from schema cache
        self.app_context.schema_registry().invalidate_all_versions(table_id);
        
        // 3. Deregister DataFusion provider
        self.app_context
            .schema_registry()
            .remove_provider(table_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to deregister provider: {}", e)))?;
        
        Ok(format!(
            "Table {}.{} dropped successfully",
            table_id.namespace_id().as_str(),
            table_id.table_name().as_str()
        ))
    }
    
    /// Prime the schema cache with table definition
    fn prime_schema_cache(
        &self,
        table_id: &TableId,
        table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<(), ApplierError> {
        use kalamdb_commons::models::schemas::TableOptions;
        
        let table_def_arc = Arc::new(table_def.clone());
        let schema_registry = self.app_context.schema_registry();
        
        let (storage_id_opt, template) = match &table_def.table_options {
            TableOptions::User(opts) => {
                let storage_id = opts.storage_id.clone();
                let template = PathResolver::resolve_storage_path_template(
                    table_id,
                    table_type,
                    &storage_id,
                ).map_err(|e| ApplierError::Execution(format!("Failed to resolve path: {}", e)))?;
                (Some(storage_id), template)
            }
            TableOptions::Shared(opts) => {
                let storage_id = opts.storage_id.clone();
                let template = PathResolver::resolve_storage_path_template(
                    table_id,
                    table_type,
                    &storage_id,
                ).map_err(|e| ApplierError::Execution(format!("Failed to resolve path: {}", e)))?;
                (Some(storage_id), template)
            }
            TableOptions::Stream(_) => (None, String::new()),
            TableOptions::System(_) => return Ok(()),
        };
        
        let data = CachedTableData::with_storage_config(
            table_def_arc,
            storage_id_opt,
            template,
        );
        schema_registry.insert(table_id.clone(), Arc::new(data));
        
        Ok(())
    }
    
    /// Register table provider with DataFusion
    fn register_table_provider(
        &self,
        table_id: &TableId,
        table_type: TableType,
        table_def: &TableDefinition,
    ) -> Result<(), ApplierError> {
        let arrow_schema = self.app_context
            .schema_registry()
            .get_arrow_schema(table_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to get Arrow schema: {}", e)))?;
        
        match table_type {
            TableType::User => {
                table_registration::register_user_table_provider(
                    &self.app_context,
                    table_id,
                    arrow_schema,
                ).map_err(|e| ApplierError::Execution(format!("Failed to register user table: {}", e)))?;
            }
            TableType::Shared => {
                table_registration::register_shared_table_provider(
                    &self.app_context,
                    table_id,
                    arrow_schema,
                ).map_err(|e| ApplierError::Execution(format!("Failed to register shared table: {}", e)))?;
            }
            TableType::Stream => {
                use kalamdb_commons::models::schemas::TableOptions;
                let ttl = match &table_def.table_options {
                    TableOptions::Stream(opts) => Some(opts.ttl_seconds),
                    _ => Some(10),
                };
                table_registration::register_stream_table_provider(
                    &self.app_context,
                    table_id,
                    arrow_schema,
                    ttl,
                ).map_err(|e| ApplierError::Execution(format!("Failed to register stream table: {}", e)))?;
            }
            TableType::System => {
                // System tables are registered differently
            }
        }
        
        Ok(())
    }
}
