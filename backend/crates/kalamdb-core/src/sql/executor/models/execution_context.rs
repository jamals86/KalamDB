use datafusion::prelude::SessionContext;
use datafusion::scalar::ScalarValue;
use datafusion_common::config::{ConfigExtension, ExtensionOptions};
use kalamdb_commons::{NamespaceId, Role, UserId};
use std::sync::Arc;
use std::time::SystemTime;

/// Session-level user context passed via DataFusion's extension system
///
/// **Purpose**: Pass (user_id, role) from HTTP handler → ExecutionContext → TableProvider.scan()
/// via SessionState.config.options.extensions (ConfigExtension trait)
///
/// **Architecture**: Stateless TableProviders read this from SessionState during scan(),
/// eliminating the need for per-request provider instances or SessionState clones.
///
/// **Performance**: Storing metadata in extensions allows zero-copy table registration
/// (tables registered once in base_session_context, no clone overhead per request).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionUserContext {
    pub user_id: UserId,
    pub role: Role,
}

impl Default for SessionUserContext {
    fn default() -> Self {
        SessionUserContext {
            user_id: UserId::from("anonymous"),
            role: Role::User,
        }
    }
}

impl ExtensionOptions for SessionUserContext {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn cloned(&self) -> Box<dyn ExtensionOptions> {
        Box::new(self.clone())
    }

    fn set(&mut self, _key: &str, _value: &str) -> datafusion_common::Result<()> {
        // SessionUserContext is immutable - ignore set operations
        Ok(())
    }

    fn entries(&self) -> Vec<datafusion_common::config::ConfigEntry> {
        // No configuration entries
        vec![]
    }
}

impl ConfigExtension for SessionUserContext {
    const PREFIX: &'static str = "kalamdb";
}

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
            user_id,
            user_role,
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
            user_id,
            user_role,
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
            user_id,
            user_role,
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
            user_id: UserId::from("anonymous"),
            user_role: Role::User,
            namespace_id: None,
            request_id: None,
            ip_address: None,
            timestamp: SystemTime::now(),
            params: Vec::new(),
            base_session_context,
        }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self.user_role, Role::Dba | Role::System)
    }
    pub fn is_system(&self) -> bool {
        matches!(self.user_role, Role::System)
    }

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
    /// # Returns
    /// SessionContext with user_id and role injected, ready for query execution
    pub fn create_session_with_user(&self) -> SessionContext {
        // Clone SessionState (mostly Arc pointer copies, ~1-2μs)
        let mut session_state = self.base_session_context.state();

        // Inject current user_id and role into session config extensions
        // TableProviders will read this during scan() for per-user filtering
        session_state
            .config_mut()
            .options_mut()
            .extensions
            .insert(SessionUserContext {
                user_id: self.user_id.clone(),
                role: self.user_role.clone(),
            });

        // Create SessionContext from the per-user state
        SessionContext::new_with_state(session_state)
    }
}
