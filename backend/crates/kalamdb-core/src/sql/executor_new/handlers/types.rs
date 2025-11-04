//! SQL Execution Types
//!
//! Common types used across all handler modules.

use crate::catalog::UserId;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::Role;

/// SQL execution result
#[derive(Debug)]
pub enum ExecutionResult {
    /// DDL operation completed successfully with a message
    Success(String),

    /// Query result as Arrow RecordBatch
    RecordBatch(RecordBatch),

    /// Multiple record batches (for streaming results)
    RecordBatches(Vec<RecordBatch>),

    /// Subscription metadata (for SUBSCRIBE TO commands)
    Subscription(serde_json::Value),
}

/// Execution context for SQL queries
///
/// Contains authentication and authorization information for the current query execution.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// User ID of the user executing the query
    pub user_id: UserId,
    /// Role of the user (User, Service, Dba, System)
    pub user_role: Role,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(user_id: UserId, user_role: Role) -> Self {
        Self { user_id, user_role }
    }

    /// Create an anonymous execution context with User role
    pub fn anonymous() -> Self {
        Self {
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
        }
    }

    /// Check if the user is an administrator (Dba or System role)
    pub fn is_admin(&self) -> bool {
        matches!(self.user_role, Role::Dba | Role::System)
    }

    /// Check if the user is a system user
    pub fn is_system(&self) -> bool {
        matches!(self.user_role, Role::System)
    }
}

/// Additional metadata for a statement execution (per request).
#[derive(Debug, Clone, Default)]
pub struct ExecutionMetadata {
    /// Optional client IP address (for audit logging).
    pub ip_address: Option<String>,
}
