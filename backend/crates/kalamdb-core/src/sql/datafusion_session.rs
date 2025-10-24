//! DataFusion session factory
//!
//! This module provides session creation with namespace and user context tracking.
//! Custom SQL functions (SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER) are registered
//! with each session for use in SELECT, WHERE, and DEFAULT clauses.

use crate::catalog::{NamespaceId, TableCache, UserId};
use crate::sql::functions::{
    CurrentUserFunction, SnowflakeIdFunction, UlidFunction, UuidV7Function,
};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionContext;
use datafusion::execution::runtime_env::{RuntimeConfig, RuntimeEnv};
use datafusion::logical_expr::ScalarUDF;
use datafusion::prelude::SessionConfig;
use std::sync::Arc;

/// Session state with KalamDB context
#[derive(Debug, Clone)]
pub struct KalamSessionState {
    /// Current user ID
    pub user_id: UserId,

    /// Current namespace
    pub namespace_id: NamespaceId,

    /// Table metadata cache
    pub table_cache: TableCache,
}

impl KalamSessionState {
    /// Create a new session state
    pub fn new(user_id: UserId, namespace_id: NamespaceId, table_cache: TableCache) -> Self {
        Self {
            user_id,
            namespace_id,
            table_cache,
        }
    }
}

/// DataFusion session factory
pub struct DataFusionSessionFactory {
    runtime_env: Arc<RuntimeEnv>,
}

impl DataFusionSessionFactory {
    /// Create a new session factory
    pub fn new() -> DataFusionResult<Self> {
        let runtime_config = RuntimeConfig::new();
        let runtime_env = RuntimeEnv::new(runtime_config)?;

        Ok(Self {
            runtime_env: Arc::new(runtime_env),
        })
    }

    /// Create a session with default configuration
    pub fn create_session(&self) -> SessionContext {
        let config = SessionConfig::new()
            .with_information_schema(true)
            .with_default_catalog_and_schema("kalam", "default");

        let ctx = SessionContext::new_with_config_rt(config, self.runtime_env.clone());
        
        // Register custom functions that are not built-in to DataFusion
        // Note: NOW() and CURRENT_TIMESTAMP() are already built-in to DataFusion
        self.register_custom_functions(&ctx, None);
        
        ctx
    }

    /// Create a session for a specific user and namespace
    pub fn create_session_for_user(
        &self,
        user_id: UserId,
        namespace_id: NamespaceId,
        table_cache: TableCache,
    ) -> (SessionContext, KalamSessionState) {
        let config = SessionConfig::new()
            .with_information_schema(true)
            .with_default_catalog_and_schema("kalam", "default");

        let ctx = SessionContext::new_with_config_rt(config, self.runtime_env.clone());
        let state = KalamSessionState::new(user_id.clone(), namespace_id, table_cache);

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
        let current_user_fn = if let Some(uid) = user_id {
            // Use a session state-aware version when we have full state
            // For now, just use the user_id string directly
            let state = KalamSessionState::new(
                uid.clone(),
                NamespaceId::new("default"),
                TableCache::new(),
            );
            CurrentUserFunction::with_session_state(&state)
        } else {
            CurrentUserFunction::new()
        };
        ctx.register_udf(ScalarUDF::from(current_user_fn));
    }

    /// Create a session with custom configuration
    pub fn create_session_with_config(&self, config: SessionConfig) -> SessionContext {
        SessionContext::new_with_config_rt(config, self.runtime_env.clone())
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
    fn test_create_session_for_user() {
        let factory = DataFusionSessionFactory::new().unwrap();
        let user_id = UserId::new("user1");
        let namespace_id = NamespaceId::new("app");
        let table_cache = TableCache::new();

        let (session, state) =
            factory.create_session_for_user(user_id.clone(), namespace_id.clone(), table_cache);

        // Verify session is created
        assert!(session.catalog("kalam").is_some());

        // Verify state
        assert_eq!(state.user_id, user_id);
        assert_eq!(state.namespace_id, namespace_id);
    }

    #[test]
    fn test_session_state() {
        let user_id = UserId::new("user1");
        let namespace_id = NamespaceId::new("app");
        let table_cache = TableCache::new();

        let state = KalamSessionState::new(user_id.clone(), namespace_id.clone(), table_cache);

        assert_eq!(state.user_id, user_id);
        assert_eq!(state.namespace_id, namespace_id);
    }
}
