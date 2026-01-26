//! Storage Executor - CREATE/DROP STORAGE operations
//!
//! This is the SINGLE place where storage mutations happen.

use std::sync::Arc;

use kalamdb_commons::models::StorageId;
use kalamdb_system::Storage;

use crate::app_context::AppContext;
use crate::applier::ApplierError;

/// Executor for storage operations
pub struct StorageExecutor {
    app_context: Arc<AppContext>,
}

impl StorageExecutor {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }

    /// Execute CREATE STORAGE
    pub async fn create_storage(&self, storage: &Storage) -> Result<String, ApplierError> {
        log::info!("CommandExecutorImpl: Creating storage {}", storage.storage_id);

        self.app_context
            .system_tables()
            .storages()
            .create_storage(storage.clone())
            .map_err(|e| ApplierError::Execution(format!("Failed to create storage: {}", e)))?;

        Ok(format!("Storage {} created successfully", storage.storage_id))
    }

    /// Execute DROP STORAGE
    pub async fn drop_storage(&self, storage_id: &StorageId) -> Result<String, ApplierError> {
        log::info!("CommandExecutorImpl: Dropping storage {}", storage_id);

        self.app_context
            .system_tables()
            .storages()
            .delete_storage(storage_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to drop storage: {}", e)))?;

        Ok(format!("Storage {} dropped successfully", storage_id))
    }
}
