use kalamdb_commons::{NamespaceId, Role, UserId};
use std::sync::Arc;
use std::time::SystemTime;
use datafusion::prelude::SessionContext;
use datafusion::scalar::ScalarValue;

/// Unified execution context for SQL queries
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
    /// Query parameters ($1, $2, ...) - max 50, 512KB each
    pub params: Vec<ScalarValue>,
    /// DataFusion session for query execution
    pub session: Arc<SessionContext>,
}

impl ExecutionContext {
    pub fn new(user_id: UserId, user_role: Role) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session: Arc::new(SessionContext::new()),
        }
    }

    pub fn with_namespace(user_id: UserId, user_role: Role, namespace_id: NamespaceId) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id: Some(namespace_id),
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session: Arc::new(SessionContext::new()),
        }
    }

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
            params: Vec::new(),
            session: Arc::new(SessionContext::new()),
        }
    }

    pub fn anonymous() -> Self {
        Self {
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session: Arc::new(SessionContext::new()),
        }
    }

    pub fn is_admin(&self) -> bool { matches!(self.user_role, Role::Dba | Role::System) }
    pub fn is_system(&self) -> bool { matches!(self.user_role, Role::System) }

    pub fn user_id(&self) -> &UserId { &self.user_id }
    pub fn user_role(&self) -> Role { self.user_role }
    pub fn namespace_id(&self) -> Option<&NamespaceId> { self.namespace_id.as_ref() }
    pub fn request_id(&self) -> Option<&str> { self.request_id.as_deref() }
    pub fn ip_address(&self) -> Option<&str> { self.ip_address.as_deref() }
    pub fn timestamp(&self) -> SystemTime { self.timestamp }
}
