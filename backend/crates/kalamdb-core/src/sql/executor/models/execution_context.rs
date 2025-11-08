use kalamdb_commons::{NamespaceId, Role, UserId};
use std::sync::Arc;
use std::time::SystemTime;
use datafusion::prelude::SessionContext;
use datafusion::scalar::ScalarValue;

/// Unified execution context for SQL queries
#[derive(Clone)]
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
    /// Create a new ExecutionContext with required session
    ///
    /// # Arguments
    /// * `user_id` - User ID executing the query
    /// * `user_role` - User's role for authorization
    /// * `session` - Shared DataFusion session (from AppContext.base_session_context())
    ///
    /// # Note
    /// Session should be the shared AppContext session, not a new one per request.
    /// This keeps memory usage low (~8 bytes per request vs 500KB-1MB).
    pub fn new(user_id: UserId, user_role: Role, session: Arc<SessionContext>) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session,
        }
    }

    pub fn with_namespace(
        user_id: UserId,
        user_role: Role,
        namespace_id: NamespaceId,
        session: Arc<SessionContext>,
    ) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id: Some(namespace_id),
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session,
        }
    }

    pub fn with_audit_info(
        user_id: UserId,
        user_role: Role,
        namespace_id: Option<NamespaceId>,
        request_id: Option<String>,
        ip_address: Option<String>,
        session: Arc<SessionContext>,
    ) -> Self {
        Self {
            user_id,
            user_role,
            namespace_id,
            request_id,
            ip_address,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session,
        }
    }

    pub fn anonymous(session: Arc<SessionContext>) -> Self {
        Self {
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            session,
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

    // Builder methods for Phase 3
    pub fn with_params(mut self, params: Vec<ScalarValue>) -> Self {
        self.params = params;
        self
    }

    pub fn with_session(mut self, session: Arc<SessionContext>) -> Self {
        self.session = session;
        self
    }

    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn with_ip(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }
}
