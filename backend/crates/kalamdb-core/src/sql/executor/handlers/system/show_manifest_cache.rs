//! SHOW MANIFEST CACHE handler (Phase 4, US6, T089-T090)
//!
//! Returns all manifest cache entries with their metadata.
//! Uses ManifestTableProvider from kalamdb-system for consistent schema.

use crate::error::KalamDbError;
use crate::sql::executor::handlers::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use async_trait::async_trait;
use kalamdb_sql::ShowManifestStatement;
use kalamdb_system::providers::ManifestTableProvider;
use std::sync::Arc;

/// Handler for SHOW MANIFEST CACHE command
///
/// Delegates to ManifestTableProvider for consistent schema and implementation.
pub struct ShowManifestCacheHandler {
    app_context: Arc<crate::app_context::AppContext>,
}

impl ShowManifestCacheHandler {
    /// Create a new ShowManifestCacheHandler
    pub fn new(app_context: Arc<crate::app_context::AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait]
impl TypedStatementHandler<ShowManifestStatement> for ShowManifestCacheHandler {
    async fn execute(
        &self,
        _stmt: ShowManifestStatement,
        _params: Vec<ScalarValue>,
        context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let start_time = std::time::Instant::now();
        // Use ManifestTableProvider to scan the manifest cache
        let provider = ManifestTableProvider::new(self.app_context.storage_backend());

        let batch = provider
            .scan_to_record_batch()
            .map_err(|e| KalamDbError::Other(format!("Failed to read manifest cache: {}", e)))?;

        let row_count = batch.num_rows();

        log::info!("SHOW MANIFEST CACHE returned {} entries", row_count);

        // Log query operation
        let duration = start_time.elapsed().as_secs_f64() * 1000.0;
        use crate::sql::executor::helpers::audit;
        let audit_entry = audit::log_query_operation(
            context,
            "SHOW",
            "MANIFEST CACHE",
            duration,
            None,
        );
        audit::persist_audit_entry(&self.app_context, &audit_entry).await?;

        Ok(ExecutionResult::Rows {
            batches: vec![batch],
            row_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;

    #[tokio::test]
    async fn test_show_manifest_cache_empty() {
        use crate::sql::executor::models::ExecutionContext;
        use datafusion::prelude::SessionContext;
        use kalamdb_commons::models::{Role, UserId};
        use std::sync::Arc;

        let app_context = Arc::new(AppContext::new_test());
        let handler = ShowManifestCacheHandler::new(app_context.clone());
        let stmt = ShowManifestStatement;
        let exec_ctx = ExecutionContext::new(
            UserId::from("1"),
            Role::System,
            Arc::new(SessionContext::new()),
        );

        let result = handler.execute(stmt, vec![], &exec_ctx).await;
        assert!(result.is_ok());

        if let Ok(ExecutionResult::Rows { batches, row_count }) = result {
            assert_eq!(row_count, 0);
            assert_eq!(batches.len(), 1);
            assert_eq!(batches[0].num_rows(), 0);
        } else {
            panic!("Expected Rows result");
        }
    }

    #[test]
    fn test_schema_structure() {
        use kalamdb_system::providers::manifest::ManifestTableSchema;

        let schema = ManifestTableSchema::schema();
        assert_eq!(schema.fields().len(), 10);
        assert_eq!(schema.field(0).name(), "cache_key");
        assert_eq!(schema.field(1).name(), "namespace_id");
        assert_eq!(schema.field(2).name(), "table_name");
        assert_eq!(schema.field(3).name(), "scope");
        assert_eq!(schema.field(4).name(), "etag");
        assert_eq!(schema.field(5).name(), "last_refreshed");
        assert_eq!(schema.field(6).name(), "last_accessed");
        assert_eq!(schema.field(7).name(), "ttl_seconds");
        assert_eq!(schema.field(8).name(), "source_path");
        assert_eq!(schema.field(9).name(), "sync_state");
    }
}
