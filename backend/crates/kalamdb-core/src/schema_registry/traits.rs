//! Trait abstractions for external dependencies
//!
//! These traits allow kalamdb-registry to be independent of kalamdb-core
//! while still accessing necessary functionality.

use datafusion::datasource::TableProvider;
use std::sync::Arc;

/// Trait for accessing table metadata from system.tables
///
/// This is implemented by TablesTableProvider in kalamdb-system
#[async_trait::async_trait]
pub trait TableMetadataProvider: Send + Sync {
    /// Get table metadata by table ID
    async fn get_table(&self, table_id: &str) -> Result<Option<serde_json::Value>, String>;

    /// List all tables
    async fn list_tables(&self) -> Result<Vec<serde_json::Value>, String>;
}

/// For now, we use a placeholder type. This will be replaced with the trait when integrating
pub type TablesTableProvider = Arc<dyn TableProvider>;
