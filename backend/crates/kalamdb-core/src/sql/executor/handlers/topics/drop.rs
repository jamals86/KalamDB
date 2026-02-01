//! DROP TOPIC handler

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::context::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::TopicId;
use kalamdb_sql::ddl::DropTopicStatement;
use std::sync::Arc;

/// Handler for DROP TOPIC statements
pub struct DropTopicHandler {
    app_context: Arc<AppContext>,
}

impl DropTopicHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<DropTopicStatement> for DropTopicHandler {
    async fn execute(
        &self,
        statement: DropTopicStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let topic_id = TopicId::new(&statement.topic_name);

        // Access topics provider from system_tables
        let topics_provider = self.app_context.system_tables().topics();

        // Check if topic exists
        let topic = topics_provider.get_topic_by_id_async(&topic_id).await?;

        if topic.is_none() {
            return Err(KalamDbError::NotFound(format!(
                "Topic '{}' does not exist",
                statement.topic_name
            )));
        }

        // Delete from system.topics
        topics_provider.delete_topic_async(&topic_id).await?;

        // Remove from TopicPublisherService cache
        self.app_context.topic_publisher().remove_topic(&topic_id);

        // TODO: Clean up topic messages from topic_messages table
        // This will be handled by a background job or retention policy

        Ok(ExecutionResult::Success {
            message: format!("Dropped topic '{}'", statement.topic_name),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &DropTopicStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::Role;

        // Only DBA and System roles can drop topics
        match context.user_role() {
            Role::Dba | Role::System => Ok(()),
            _ => Err(KalamDbError::PermissionDenied(
                "DROP TOPIC requires DBA or System role".to_string(),
            )),
        }
    }
}
