//! DataFusion session factory
//!
//! This module provides session creation with namespace and user context tracking.
//! Custom SQL functions (SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER) are registered
//! with each session for use in SELECT, WHERE, and DEFAULT clauses.

use crate::schema_registry::{NamespaceId, UserId};
use crate::sql::functions::{
    CurrentUserFunction, SnowflakeIdFunction, UlidFunction, UuidV7Function,
};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::ScalarUDF;
use datafusion::prelude::SessionConfig;

/// Session state with KalamDB context
///
/// **DEPRECATED**: This struct is being phased out in favor of `ExecutionContext`.
/// 
/// `KalamSessionState` was originally created to hold user/namespace context for DataFusion sessions.
/// However, `ExecutionContext` (in `sql/executor/handlers/types.rs`) now provides a more comprehensive
/// execution context that includes user ID, role, namespace, audit information, and timestamps.
///
/// This struct remains for backward compatibility with DataFusion session creation,
/// but new code should use `ExecutionContext` instead.
///
/// See: Phase 2 Task T014 (009-core-architecture)
#[deprecated(
    since = "0.9.0",
    note = "Use ExecutionContext from sql/executor/handlers/types.rs instead"
)]
#[derive(Debug, Clone)]
pub struct KalamSessionState {
    /// Current user ID
    pub user_id: UserId,

    /// Current namespace
    pub namespace_id: NamespaceId,
}

#[allow(deprecated)]
impl KalamSessionState {
    /// Create a new session state
    #[deprecated(
        since = "0.9.0",
        note = "Use ExecutionContext::with_namespace() instead"
    )]
    pub fn new(user_id: UserId, namespace_id: NamespaceId) -> Self {
        Self {
            user_id,
            namespace_id,
        }
    }
}

/// DataFusion session factory
pub struct DataFusionSessionFactory {}

impl DataFusionSessionFactory {
    /// Create a new session factory
    pub fn new() -> DataFusionResult<Self> {
        Ok(Self {})
    }

    /// Create a session with default configuration
    pub fn create_session(&self) -> SessionContext {
        let config = SessionConfig::new()
            .with_information_schema(true)
            .with_default_catalog_and_schema("kalam", "default");

        let ctx = SessionContext::new_with_config(config);

        // Register custom functions that are not built-in to DataFusion
        // Note: NOW() and CURRENT_TIMESTAMP() are already built-in to DataFusion
        self.register_custom_functions(&ctx, None);

        ctx
    }

    /// Create a session for a specific user and namespace
    #[allow(deprecated)]
    pub fn create_session_for_user(
        &self,
        user_id: UserId,
        namespace_id: NamespaceId,
    ) -> (SessionContext, KalamSessionState) {
        let config = SessionConfig::new()
            .with_information_schema(true)
            .with_default_catalog_and_schema("kalam", "default");

        let ctx = SessionContext::new_with_config(config);
        let state = KalamSessionState::new(user_id.clone(), namespace_id);

        // Register custom functions with user context for CURRENT_USER()
        self.register_custom_functions(&ctx, Some(&user_id));

        (ctx, state)
    }

    /// Register custom SQL functions with the session
    ///
    /// Registers KalamDB-specific functions:
    /// - SNOWFLAKE_ID() - 64-bit time-ordered distributed IDs
    /// - UUID_V7() - RFC 9562 compliant UUIDs with timestamp
    /// - ULID() - 26-character Crockford Base32 time-sortable IDs
    /// - CURRENT_USER() - Returns the authenticated user ID
    ///
    /// DataFusion built-in functions already available:
    /// - NOW() - Current timestamp
    /// - CURRENT_TIMESTAMP() - Alias for NOW()
    fn register_custom_functions(&self, ctx: &SessionContext, user_id: Option<&UserId>) {
        // Register SNOWFLAKE_ID() function
        let snowflake_fn = SnowflakeIdFunction::new();
        ctx.register_udf(ScalarUDF::from(snowflake_fn));

        // Register UUID_V7() function
        let uuid_fn = UuidV7Function::new();
        ctx.register_udf(ScalarUDF::from(uuid_fn));

        // Register ULID() function
        let ulid_fn = UlidFunction::new();
        ctx.register_udf(ScalarUDF::from(ulid_fn));

        // Register CURRENT_USER() function with user context if available
        #[allow(deprecated)]
        let current_user_fn = if let Some(uid) = user_id {
            // Use a session state-aware version when we have full state
            let state =
                KalamSessionState::new(uid.clone(), NamespaceId::new("default"));
            CurrentUserFunction::with_session_state(&state)
        } else {
            CurrentUserFunction::new()
        };
        ctx.register_udf(ScalarUDF::from(current_user_fn));
    }

    /// Create a session with custom configuration
    pub fn create_session_with_config(&self, config: SessionConfig) -> SessionContext {
        SessionContext::new_with_config(config)
    }
}

impl Default for DataFusionSessionFactory {
    fn default() -> Self {
        Self::new().expect("Failed to create DataFusion session factory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_factory() {
        let factory = DataFusionSessionFactory::new();
        assert!(factory.is_ok());
    }

    #[test]
    fn test_create_session() {
        let factory = DataFusionSessionFactory::new().unwrap();
        let session = factory.create_session();

        // Verify session is created
        assert!(session.catalog("kalam").is_some());
    }

    #[test]
    #[allow(deprecated)]
    fn test_create_session_for_user() {
        let factory = DataFusionSessionFactory::new().unwrap();
        let user_id = UserId::new("user1");
        let namespace_id = NamespaceId::new("app");

        let (session, state) =
            factory.create_session_for_user(user_id.clone(), namespace_id.clone());

        // Verify session is created
        assert!(session.catalog("kalam").is_some());

        // Verify state
        assert_eq!(state.user_id, user_id);
        assert_eq!(state.namespace_id, namespace_id);
    }

    #[test]
    #[allow(deprecated)]
    fn test_session_state() {
        let user_id = UserId::new("user1");
        let namespace_id = NamespaceId::new("app");

        let state = KalamSessionState::new(user_id.clone(), namespace_id.clone());

        assert_eq!(state.user_id, user_id);
        assert_eq!(state.namespace_id, namespace_id);
    }
}
