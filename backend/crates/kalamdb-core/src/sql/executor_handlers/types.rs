//! SQL Execution Types

use crate::catalog::UserId;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::Role;

/// SQL execution result
#[derive(Debug)]
pub enum ExecutionResult {
    Success(String),
    RecordBatch(RecordBatch),
    RecordBatches(Vec<RecordBatch>),
    Subscription(serde_json::Value),
}

/// Execution context for SQL queries
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub user_id: UserId,
    pub user_role: Role,
}

impl ExecutionContext {
    pub fn new(user_id: UserId, user_role: Role) -> Self {
        Self { user_id, user_role }
    }

    pub fn anonymous() -> Self {
        Self {
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
        }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self.user_role, Role::Dba | Role::System)
    }

    pub fn is_system(&self) -> bool {
        matches!(self.user_role, Role::System)
    }
}

/// Additional metadata for statement execution
#[derive(Debug, Clone, Default)]
pub struct ExecutionMetadata {
    pub ip_address: Option<String>,
}
