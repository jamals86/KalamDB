//! Typed handler for SUBSCRIBE statement

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::executor::models::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_sql::ddl::SubscribeStatement;
use std::sync::Arc;
use uuid::Uuid;

/// Handler for SUBSCRIBE TO (Live Query)
pub struct SubscribeHandler {
    _app_context: Arc<AppContext>, // Reserved for future use
}

impl SubscribeHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self {
            _app_context: app_context,
        }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<SubscribeStatement> for SubscribeHandler {
    async fn execute(
        &self,
        statement: SubscribeStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        // Generate a subscription id (actual registration happens over WebSocket handshake)
        let subscription_id = format!(
            "sub-{}-{}-{}",
            statement.namespace.as_str(),
            statement.table_name.as_str(),
            Uuid::new_v4().simple()
        );
        // Channel placeholder (could read from config.toml later)
        let channel = "ws://localhost:8080/ws".to_string();

        // Return subscription metadata with the SELECT query
        Ok(ExecutionResult::Subscription {
            subscription_id,
            channel,
            select_query: statement.select_query,
        })
    }

    async fn check_authorization(
        &self,
        _statement: &SubscribeStatement,
        _context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        // SUBSCRIBE allowed for all authenticated users (table-specific auth done in execution)
        Ok(())
    }
}
