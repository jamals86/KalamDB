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

        // Delete from system.topics first
        topics_provider.delete_topic_async(&topic_id).await?;

        // Remove from TopicPublisherService cache
        self.app_context.topic_publisher().remove_topic(&topic_id);

        // Clean up all topic data from storage
        // Note: We don't drop the entire partitions as they're shared across all topics.
        // Instead, we delete specific topic messages and offsets using scan+delete operations.
        
        // Clean up consumer group offsets from the offset store
        // This removes all offset tracking for all consumer groups on this topic
        let offset_store = self.app_context.topic_publisher().offset_store();
        match offset_store.delete_topic_offsets(&topic_id) {
            Ok(count) => {
                log::info!("Deleted {} consumer group offsets for topic '{}'", count, statement.topic_name);
            }
            Err(e) => {
                log::warn!("Failed to delete consumer offsets for topic '{}': {}", statement.topic_name, e);
            }
        }

        // TODO: Clean up topic messages from the message store
        // This requires adding a delete_messages_for_topic method to TopicMessageStore
        // For now, messages will be cleaned up by retention policy

        Ok(ExecutionResult::Success {
            message: format!("Dropped topic '{}' and cleaned up consumer offsets", statement.topic_name),
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
