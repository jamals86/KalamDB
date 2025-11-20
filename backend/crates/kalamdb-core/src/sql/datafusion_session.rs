//! DataFusion session factory
//!
//! This module provides session creation with namespace and user context tracking.
//! Custom SQL functions (SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER) are registered
//! with each session for use in SELECT, WHERE, and DEFAULT clauses.

use crate::sql::functions::{
    CurrentUserFunction, SnowflakeIdFunction, UlidFunction, UuidV7Function,
};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::ScalarUDF;
use datafusion::prelude::SessionConfig;

// KalamSessionState removed (ExecutionContext used at higher layer)

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
            .with_parquet_bloom_filter_pruning(true)
            .with_parquet_page_index_pruning(true)
            .with_default_catalog_and_schema("kalam", "default");

        let ctx = SessionContext::new_with_config(config);

        // Register custom functions that are not built-in to DataFusion
        // Note: NOW() and CURRENT_TIMESTAMP() are already built-in to DataFusion
        self.register_custom_functions(&ctx);

        ctx
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
    fn register_custom_functions(&self, ctx: &SessionContext) {
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
        ctx.register_udf(ScalarUDF::from(CurrentUserFunction::new()));
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
}
