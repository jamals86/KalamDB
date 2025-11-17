//! Typed handler for FLUSH ALL TABLES statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::jobs::executors::flush::FlushParams;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::{TableId, TableName};
use kalamdb_commons::{JobId, JobType};
use kalamdb_sql::ddl::FlushAllTablesStatement;
use std::sync::Arc;

/// Handler for FLUSH ALL TABLES
pub struct FlushAllTablesHandler {
    app_context: Arc<AppContext>,
}

impl FlushAllTablesHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<FlushAllTablesStatement> for FlushAllTablesHandler {
    async fn execute(
        &self,
        statement: FlushAllTablesStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Scan namespace for all tables (system + user/shared/stream are all listed in system.tables)
        let tables_provider = self.app_context.system_tables().tables();
        let all_defs = tables_provider.list_tables()?;
        let ns = statement.namespace.clone();
        let target_tables: Vec<(TableName, kalamdb_commons::schemas::TableType)> = all_defs
            .into_iter()
            .filter(|d| d.namespace_id == ns)
            .map(|d| (d.table_name.clone(), d.table_type))
            .collect();

        if target_tables.is_empty() {
            return Err(KalamDbError::NotFound(format!(
                "No tables found in namespace {}",
                ns.as_str()
            )));
        }

        let job_manager = self.app_context.job_manager();
        let mut job_ids: Vec<String> = Vec::new();
        for (table_name, table_type) in &target_tables {
            let table_id = TableId::new(ns.clone(), table_name.clone());
            let params = FlushParams {
                table_id: table_id.clone(),
                table_type: *table_type,
                flush_threshold: None,
            };
            let idempotency_key = format!("flush-{}-{}", ns.as_str(), table_name.as_str());
            let job_id: JobId = job_manager
                .create_job_typed(
                    JobType::Flush,
                    ns.clone(),
                    params,
                    Some(idempotency_key),
                    None,
                )
                .await?;
            job_ids.push(job_id.as_str().to_string());
        }

        Ok(ExecutionResult::Success {
            message: format!(
                "Flush started for {} table(s) in namespace '{}'. Job IDs: [{}]",
                job_ids.len(),
                ns.as_str(),
                job_ids.join(", ")
            ),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &FlushAllTablesStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        if !context.is_admin() {
            return Err(KalamDbError::Unauthorized(
                "FLUSH ALL TABLES requires DBA or System role".to_string(),
            ));
        }
        Ok(())
    }
}
