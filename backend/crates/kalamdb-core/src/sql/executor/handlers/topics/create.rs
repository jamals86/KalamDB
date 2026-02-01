//! CREATE TOPIC handler

use crate::app_context::AppContext;
use crate::error::KalamDbError;
use crate::sql::executor::handlers::typed::TypedStatementHandler;
use crate::sql::context::{ExecutionContext, ExecutionResult, ScalarValue};
use kalamdb_commons::models::TopicId;
use kalamdb_sql::ddl::CreateTopicStatement;
use kalamdb_system::providers::topics::models::Topic;
use std::sync::Arc;

/// Handler for CREATE TOPIC statements
pub struct CreateTopicHandler {
    app_context: Arc<AppContext>,
}

impl CreateTopicHandler {
    pub fn new(app_context: Arc<AppContext>) -> Self {
        Self { app_context }
    }
}

#[async_trait::async_trait]
impl TypedStatementHandler<CreateTopicStatement> for CreateTopicHandler {
    async fn execute(
        &self,
        statement: CreateTopicStatement,
        _params: Vec<ScalarValue>,
        _context: &ExecutionContext,
    ) -> Result<ExecutionResult, KalamDbError> {
        let topic_id = TopicId::new(&statement.topic_name);

        // Access topics provider from system_tables
        let topics_provider = self.app_context.system_tables().topics();

        // Check if topic already exists
        if topics_provider.get_topic_by_id_async(&topic_id).await?.is_some() {
            return Err(KalamDbError::AlreadyExists(format!(
                "Topic '{}' already exists",
                statement.topic_name
            )));
        }

        // Create topic with default settings
        let mut topic = Topic::new(topic_id.clone(), statement.topic_name.clone());
        topic.partitions = statement.partitions.unwrap_or(1); // Default to 1 partition
        topic.retention_seconds = Some(7 * 24 * 60 * 60); // Default 7 days
        topic.retention_max_bytes = Some(1024 * 1024 * 1024); // Default 1GB

        // Insert into system.topics
        topics_provider.create_topic_async(topic.clone()).await?;

        // Add to TopicPublisherService cache for immediate availability
        self.app_context.topic_publisher().add_topic(topic.clone());

        Ok(ExecutionResult::Success {
            message: format!(
                "Created topic '{}' with {} partition(s)",
                statement.topic_name, topic.partitions
            ),
        })
    }

    async fn check_authorization(
        &self,
        _statement: &CreateTopicStatement,
        context: &ExecutionContext,
    ) -> Result<(), KalamDbError> {
        use kalamdb_commons::Role;

        // Only DBA and System roles can create topics
        match context.user_role() {
            Role::Dba | Role::System => Ok(()),
            _ => Err(KalamDbError::PermissionDenied(
                "CREATE TOPIC requires DBA or System role".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{create_test_session_simple, test_app_context_simple};
    use kalamdb_commons::models::UserId;
    use kalamdb_commons::Role;

    fn test_context() -> ExecutionContext {
        ExecutionContext::new(UserId::from("test_user"), Role::Dba, create_test_session_simple())
    }

    #[tokio::test]
    async fn test_create_topic() {
        let app_ctx = test_app_context_simple();
        let handler = CreateTopicHandler::new(app_ctx);
        let ctx = test_context();

        let stmt = CreateTopicStatement {
            topic_name: "test_events".to_string(),
            partitions: Some(4),
        };

        let result = handler.execute(stmt, vec![], &ctx).await;
        assert!(result.is_ok(), "Failed to create topic: {:?}", result.err());

        match result.unwrap() {
            ExecutionResult::Success { message } => {
                assert!(message.contains("test_events"));
                assert!(message.contains("4 partition"));
            }
            _ => panic!("Expected Success result"),
        }
    }

    #[tokio::test]
    async fn test_create_topic_duplicate() {
        let app_ctx = test_app_context_simple();
        let handler = CreateTopicHandler::new(app_ctx);
        let ctx = test_context();

        let stmt = CreateTopicStatement {
            topic_name: "duplicate_topic".to_string(),
            partitions: None,
        };

        // First creation should succeed
        let result1 = handler.execute(stmt.clone(), vec![], &ctx).await;
        assert!(result1.is_ok());

        // Second creation should fail with AlreadyExists
        let result2 = handler.execute(stmt, vec![], &ctx).await;
        assert!(result2.is_err());
        assert!(matches!(result2.unwrap_err(), KalamDbError::AlreadyExists(_)));
    }
}
