//! Core types for SQL execution handlers
//!
//! This module defines shared types used across all SQL execution handlers:
//! - `ExecutionContext`: Unified execution context (replaces KalamSessionState)
//! - `ExecutionResult`: Result type for handler operations
//! - `ExecutionMetadata`: Metadata about query execution
//! - `ParamValue`: Parameter values for prepared statements

use kalamdb_commons::{NamespaceId, UserId, Role};
use std::time::SystemTime;

/// Unified execution context for SQL queries
///
/// Consolidates ExecutionContext and KalamSessionState into a single struct.
/// Contains all context needed for query execution: user identity, namespace,
/// audit information, and timestamps.
///
/// # Example
/// ```ignore
/// use kalamdb_core::sql::executor::handlers::types::ExecutionContext;
/// use kalamdb_commons::{UserId, NamespaceId, Role};
///
/// let ctx = ExecutionContext::new(UserId::from("alice"), Role::User);
/// assert!(!ctx.is_admin());
/// ```
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// User ID executing the query (public for backward compatibility)
    pub user_id: UserId,
    
    /// User's role (public for backward compatibility)
    pub user_role: Role,
    
    /// Active namespace for the query (optional for backward compatibility)
    namespace_id: Option<NamespaceId>,
    
    /// Optional request ID for tracking
    request_id: Option<String>,
    
    /// Optional IP address for audit logging
    ip_address: Option<String>,
    
    /// Execution timestamp
    timestamp: SystemTime,
}

impl ExecutionContext {
    /// Create a new execution context (backward compatible - no namespace required)
    pub fn new(user_id: UserId, user_role: Role) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
        }
    }
    
    /// Create execution context with namespace
    pub fn with_namespace(user_id: UserId, user_role: Role, namespace_id: NamespaceId) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id: Some(namespace_id),
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
        }
    }

    /// Create context with audit information
    pub fn with_audit_info(
        user_id: UserId,
        user_role: Role,
        namespace_id: Option<NamespaceId>,
        request_id: Option<String>,
        ip_address: Option<String>,
    ) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id,
            request_id,
            ip_address,
            timestamp: SystemTime::now(),
        }
    }

    /// Create anonymous context (for system operations)
    pub fn anonymous() -> Self {
        Self {
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
        }
    }

    /// Check if user has admin privileges (Dba or System role)
    pub fn is_admin(&self) -> bool {
        matches!(self.user_role, Role::Dba | Role::System)
    }

    /// Check if user is system role
    pub fn is_system(&self) -> bool {
        matches!(self.user_role, Role::System)
    }

    // Getters
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn user_role(&self) -> Role {
        self.user_role
    }

    pub fn namespace_id(&self) -> Option<&NamespaceId> {
        self.namespace_id.as_ref()
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn ip_address(&self) -> Option<&str> {
        self.ip_address.as_deref()
    }

    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

/// Result type for SQL execution
///
/// Represents the outcome of executing a SQL statement.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// DDL operation completed successfully with a message
    Success(String),

    /// Query result as Arrow RecordBatch
    RecordBatch(arrow::array::RecordBatch),

    /// Multiple record batches (for streaming results)
    RecordBatches(Vec<arrow::array::RecordBatch>),

    /// Subscription metadata (for SUBSCRIBE TO commands)
    Subscription(serde_json::Value),
}

/// Metadata about query execution
///
/// Captures performance metrics and execution details for observability.
#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    /// Number of rows affected by the query
    pub rows_affected: usize,
    
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    
    /// Type of SQL statement executed (from kalamdb_sql::SqlStatement)
    pub statement_type: String,
}

impl ExecutionMetadata {
    /// Create new execution metadata
    pub fn new(rows_affected: usize, execution_time_ms: u64, statement_type: String) -> Self {
        Self {
            rows_affected,
            execution_time_ms,
            statement_type,
        }
    }
}

/// Parameter values for prepared statements
///
/// Supports common SQL data types for parameter binding.
///
/// # Example
/// ```
/// use kalamdb_core::sql::executor::handlers::types::ParamValue;
///
/// let params = vec![
///     ParamValue::Int(42),
///     ParamValue::Text("alice".to_string()),
///     ParamValue::Boolean(true),
/// ];
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// 32-bit integer
    Int(i32),
    
    /// 64-bit integer
    BigInt(i64),
    
    /// 32-bit float
    Float(f32),
    
    /// 64-bit float
    Double(f64),
    
    /// Text string
    Text(String),
    
    /// Boolean value
    Boolean(bool),
    
    /// NULL value
    Null,
}

impl ParamValue {
    /// Get the type name for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            ParamValue::Int(_) => "INT",
            ParamValue::BigInt(_) => "BIGINT",
            ParamValue::Float(_) => "FLOAT",
            ParamValue::Double(_) => "DOUBLE",
            ParamValue::Text(_) => "TEXT",
            ParamValue::Boolean(_) => "BOOLEAN",
            ParamValue::Null => "NULL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_context_new() {
        let ctx = ExecutionContext::new(
            UserId::from("user_test"),
            Role::User,
        );

        assert_eq!(ctx.user_id.as_str(), "user_test");
        assert_eq!(ctx.user_role, Role::User);
        assert!(!ctx.is_admin());
        assert!(!ctx.is_system());
    }

    #[test]
    fn test_execution_context_is_admin() {
        let dba_ctx = ExecutionContext::new(
            UserId::from("dba_user"),
            Role::Dba,
        );
        assert!(dba_ctx.is_admin());

        let system_ctx = ExecutionContext::new(
            UserId::from("system"),
            Role::System,
        );
        assert!(system_ctx.is_admin());
        assert!(system_ctx.is_system());

        let user_ctx = ExecutionContext::new(
            UserId::from("regular_user"),
            Role::User,
        );
        assert!(!user_ctx.is_admin());
    }

    #[test]
    fn test_execution_context_with_audit_info() {
        let ctx = ExecutionContext::with_audit_info(
            UserId::from("user_test"),
            Role::User,
            Some(NamespaceId::from("test_ns")),
            Some("req-123".to_string()),
            Some("192.168.1.1".to_string()),
        );

        assert_eq!(ctx.request_id(), Some("req-123"));
        assert_eq!(ctx.ip_address(), Some("192.168.1.1"));
    }

    #[test]
    fn test_execution_context_anonymous() {
        let ctx = ExecutionContext::anonymous();
        
        assert_eq!(ctx.user_id.as_str(), "anonymous");
        assert_eq!(ctx.user_role, Role::User);
        assert!(!ctx.is_system());
        assert!(!ctx.is_admin());
    }

    #[test]
    fn test_param_value_type_name() {
        assert_eq!(ParamValue::Int(42).type_name(), "INT");
        assert_eq!(ParamValue::BigInt(42).type_name(), "BIGINT");
        assert_eq!(ParamValue::Float(3.14).type_name(), "FLOAT");
        assert_eq!(ParamValue::Double(3.14).type_name(), "DOUBLE");
        assert_eq!(ParamValue::Text("test".to_string()).type_name(), "TEXT");
        assert_eq!(ParamValue::Boolean(true).type_name(), "BOOLEAN");
        assert_eq!(ParamValue::Null.type_name(), "NULL");
    }

    #[test]
    fn test_execution_metadata() {
        let metadata = ExecutionMetadata::new(10, 150, "SELECT".to_string());
        
        assert_eq!(metadata.rows_affected, 10);
        assert_eq!(metadata.execution_time_ms, 150);
        assert_eq!(metadata.statement_type, "SELECT");
    }
}
