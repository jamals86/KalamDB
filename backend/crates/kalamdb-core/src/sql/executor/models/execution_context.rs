use crate::sql::CurrentUserFunction;
use datafusion::logical_expr::ScalarUDF;
use datafusion::prelude::SessionContext;
use datafusion::scalar::ScalarValue;
use kalamdb_commons::models::ReadContext;
use kalamdb_commons::{NamespaceId, Role, UserId};
use kalamdb_session::{SessionUserContext, UserContext};
use std::sync::Arc;
use std::time::SystemTime;

/// Unified execution context for SQL queries
#[derive(Clone)]
pub struct ExecutionContext {
    /// User identity executing the query
    user_context: UserContext,
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
    /// Base SessionContext from AppContext (tables already registered)
    /// We extract SessionState from this and inject user_id to create per-request SessionContext
    base_session_context: Arc<SessionContext>,
}

impl ExecutionContext {
    /// Create a new ExecutionContext with base SessionContext
    ///
    /// # Arguments
    /// * `user_id` - User ID executing the query
    /// * `user_role` - User's role for authorization
    /// * `base_session_context` - Base SessionContext from AppContext (tables already registered)
    ///
    /// # Note
    /// The base_session_context contains all registered table providers.
    /// When executing queries, call `create_session_with_user()` to get a
    /// SessionContext with user_id injected for per-user filtering.
    pub fn new(
        user_id: UserId,
        user_role: Role,
        base_session_context: Arc<SessionContext>,
    ) -> Self {
        Self {
            user_context: UserContext::client(user_id, user_role),
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            base_session_context,
        }
    }

    pub fn with_namespace(
        user_id: UserId,
        user_role: Role,
        namespace_id: NamespaceId,
        base_session_context: Arc<SessionContext>,
    ) -> Self {
        Self {
            user_context: UserContext::client(user_id, user_role),
            namespace_id: Some(namespace_id),
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            base_session_context,
        }
    }

    pub fn with_audit_info(
        user_id: UserId,
        user_role: Role,
        namespace_id: Option<NamespaceId>,
        request_id: Option<String>,
        ip_address: Option<String>,
        base_session_context: Arc<SessionContext>,
    ) -> Self {
        Self {
            user_context: UserContext::client(user_id, user_role),
            namespace_id,
            request_id,
            ip_address,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            base_session_context,
        }
    }

    pub fn anonymous(base_session_context: Arc<SessionContext>) -> Self {
        Self {
            user_context: UserContext::client(UserId::from("anonymous"), Role::User),
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            base_session_context,
        }
    }

    #[inline]
    pub fn is_admin(&self) -> bool {
        kalamdb_session::is_admin_role(self.user_context.role)
    }
    #[inline]
    pub fn is_system(&self) -> bool {
        kalamdb_session::is_system_role(self.user_context.role)
    }

    /// Check if this is an anonymous user (not authenticated)
    ///
    /// Anonymous users have limited permissions:
    /// - Can only SELECT from public tables
    /// - Cannot CREATE, ALTER, DROP, INSERT, UPDATE, or DELETE
    #[inline]
    pub fn is_anonymous(&self) -> bool {
        self.user_context.user_id.as_str() == kalamdb_commons::constants::ANONYMOUS_USER_ID
    }

    #[inline]
    pub fn user_id(&self) -> &UserId {
        &self.user_context.user_id
    }
    #[inline]
    pub fn user_role(&self) -> Role {
        self.user_context.role
    }
    #[inline]
    pub fn namespace_id(&self) -> Option<&NamespaceId> {
        self.namespace_id.as_ref()
    }
    #[inline]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }
    #[inline]
    pub fn ip_address(&self) -> Option<&str> {
        self.ip_address.as_deref()
    }
    #[inline]
    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }

    // Builder methods for Phase 3
    pub fn with_params(mut self, params: Vec<ScalarValue>) -> Self {
        self.params = params;
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

    /// Set the namespace for this execution context
    pub fn with_namespace_id(mut self, namespace_id: NamespaceId) -> Self {
        self.namespace_id = Some(namespace_id);
        self
    }

    /// Set the read context (client vs internal)
    ///
    /// Use `ReadContext::Internal` for WebSocket subscriptions on followers
    /// to bypass leader-only read checks while still applying RLS.
    pub fn with_read_context(mut self, read_context: ReadContext) -> Self {
        self.user_context.read_context = read_context;
        self
    }

    /// Create a per-request SessionContext with current user_id and role injected
    ///
    /// Clones the base SessionState and injects the current user_id and role into config.extensions.
    /// The clone is relatively cheap (~1-2μs) because most fields are Arc-wrapped.
    ///
    /// # What Gets Cloned
    /// - session_id: String (~50 bytes)
    /// - config: Arc<SessionConfig> (pointer copy)
    /// - runtime_env: Arc<RuntimeEnv> (pointer copy)
    /// - catalog_list: Arc<dyn CatalogList> (pointer copy)
    /// - scalar_functions: HashMap<String, Arc<ScalarUDF>> (HashMap clone, Arc values)
    /// - Total: ~1-2μs per request
    ///
    /// # Performance Impact
    /// - At 10,000 QPS: 10-20ms/sec = 1-2% CPU overhead
    /// - At 100,000 QPS: 100-200ms/sec = 10-20% CPU overhead
    /// - Acceptable trade-off for clean user isolation
    ///
    /// # User Isolation
    /// UserTableProvider and StreamTableProvider will read SessionUserContext from
    /// state.config().options().extensions during scan() to filter data by user.
    ///
    /// # Namespace Handling
    /// If `namespace_id` is set on this ExecutionContext, it will override the
    /// default_schema in the session config. This allows clients to specify the
    /// active namespace per-request.
    ///
    /// # Returns
    /// SessionContext with user_id and role injected, ready for query execution
    pub fn create_session_with_user(&self) -> SessionContext {
        // Clone SessionState (mostly Arc pointer copies, ~1-2μs)
        let mut session_state = self.base_session_context.state();

        // Inject current user_id, role, and read_context into session config extensions
        // TableProviders will read this during scan() for per-user filtering and leader check
        // Use the read_context from this ExecutionContext (defaults to Client)
        session_state
            .config_mut()
            .options_mut()
            .extensions
            .insert(SessionUserContext::new(
                self.user_context.user_id.clone(),
                self.user_context.role,
                self.user_context.read_context,
            ));

        // Override default_schema if namespace_id is set on this context
        if let Some(ref ns) = self.namespace_id {
            session_state.config_mut().options_mut().catalog.default_schema =
                ns.as_str().to_string();
        }

        // Create SessionContext from the per-user state
        let ctx = SessionContext::new_with_state(session_state);

        // Register CURRENT_USER() function with user context
        // This overrides the default CURRENT_USER() registered in base session
        let current_user_fn = CurrentUserFunction::with_user_id(self.user_id());
        ctx.register_udf(ScalarUDF::from(current_user_fn));

        ctx
    }

    /// Get the current default namespace (schema) from DataFusion session config
    ///
    /// This reads `datafusion.catalog.default_schema` from the session configuration.
    /// The default schema is set to "default" initially and can be changed using:
    /// - `USE namespace`
    /// - `USE NAMESPACE namespace`  
    /// - `SET NAMESPACE namespace`
    ///
    /// # Returns
    /// The current default namespace as a NamespaceId (defaults to "default")
    pub fn default_namespace(&self) -> NamespaceId {
        let state = self.base_session_context.state();
        let default_schema = state.config().options().catalog.default_schema.clone();
        NamespaceId::new(default_schema)
    }

    #[inline]
    pub fn user_context(&self) -> &UserContext {
        &self.user_context
    }
}
