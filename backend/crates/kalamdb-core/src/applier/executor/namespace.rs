//! Namespace Executor - CREATE/DROP NAMESPACE operations
//!
//! This is the SINGLE place where namespace mutations happen.

use std::sync::Arc;

use kalamdb_commons::models::NamespaceId;
use kalamdb_commons::system::Namespace;

use crate::app_context::AppContext;
use crate::applier::ApplierError;

/// Executor for namespace operations
pub struct NamespaceExecutor {
    app_context: Arc<AppContext>,
}

impl NamespaceExecutor {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
    
    /// Execute CREATE NAMESPACE
    pub async fn create_namespace(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Creating namespace {}", namespace_id);
        
        let namespace = Namespace::new(namespace_id.as_str());
        
        self.app_context
            .system_tables()
            .namespaces()
            .create_namespace(namespace)
            .map_err(|e| ApplierError::Execution(format!("Failed to create namespace: {}", e)))?;
        
        Ok(format!("Namespace {} created successfully", namespace_id))
    }
    
    /// Execute DROP NAMESPACE
    pub async fn drop_namespace(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<String, ApplierError> {
        log::debug!("CommandExecutorImpl: Dropping namespace {}", namespace_id);
        
        self.app_context
            .system_tables()
            .namespaces()
            .delete_namespace(namespace_id)
            .map_err(|e| ApplierError::Execution(format!("Failed to drop namespace: {}", e)))?;
        
        Ok(format!("Namespace {} dropped successfully", namespace_id))
    }
}
