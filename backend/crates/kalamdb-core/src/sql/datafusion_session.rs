//! DataFusion session factory
//!
//! This module provides session creation with namespace and user context tracking.
//! Custom SQL functions (SNOWFLAKE_ID, UUID_V7, ULID, CURRENT_USER) are registered
//! with each session for use in SELECT, WHERE, and DEFAULT clauses.
//!
//! ## Parallelism Configuration
//!
//! DataFusion settings are loaded from `server.toml` [datafusion] section:
//! - `query_parallelism`: Number of parallel threads (0 = auto-detect CPU cores)
//! - `max_partitions`: Maximum partitions per query (enables parallel execution)
//! - `batch_size`: Arrow batch size for record processing (default: 8192)

use crate::sql::functions::{
    CurrentUserFunction, SnowflakeIdFunction, UlidFunction, UuidV7Function,
};
use datafusion::error::Result as DataFusionResult;
use datafusion::execution::context::SessionContext;
use datafusion::logical_expr::ScalarUDF;
use datafusion::prelude::SessionConfig;
use kalamdb_commons::config::DataFusionSettings;

// KalamSessionState removed (ExecutionContext used at higher layer)

/// DataFusion session factory with parallelism configuration
///
/// Creates DataFusion sessions with optimized settings for parallel query execution.
/// Configuration is loaded from server.toml [datafusion] section.
pub struct DataFusionSessionFactory {
    /// Target partitions for parallel execution (from config or auto-detected)
    target_partitions: usize,
    /// Batch size for Arrow record processing
    batch_size: usize,
}

impl DataFusionSessionFactory {
    /// Create a new session factory with default configuration
    pub fn new() -> DataFusionResult<Self> {
        Self::with_config(&DataFusionSettings::default())
    }

    /// Create a new session factory with custom DataFusion settings
    ///
    /// # Arguments
    /// * `settings` - DataFusion configuration from server.toml
    ///
    /// # Example
    /// ```ignore
    /// let settings = DataFusionSettings {
    ///     query_parallelism: 0,  // Auto-detect CPU cores
    ///     max_partitions: 16,
    ///     batch_size: 8192,
    ///     memory_limit: 1073741824,
    /// };
    /// let factory = DataFusionSessionFactory::with_config(&settings)?;
    /// ```
    pub fn with_config(settings: &DataFusionSettings) -> DataFusionResult<Self> {
        // Determine target partitions: use config value or auto-detect CPU cores
        let target_partitions = if settings.query_parallelism == 0 {
            num_cpus::get().max(1)
        } else {
            settings.query_parallelism
        };

        // Cap at max_partitions to prevent over-parallelization
        let target_partitions = target_partitions.min(settings.max_partitions);

        log::info!(
            "DataFusion configured: target_partitions={}, batch_size={}",
            target_partitions,
            settings.batch_size
        );

        Ok(Self {
            target_partitions,
            batch_size: settings.batch_size,
        })
    }

    /// Create a session with optimized parallel configuration
    ///
    /// Sessions are configured with:
    /// - `target_partitions`: Enables parallel execution for queries
    /// - `batch_size`: Controls memory usage and CPU cache efficiency
    /// - Parquet bloom filter and page index pruning for fast filtering
    pub fn create_session(&self) -> SessionContext {
        let config = SessionConfig::new()
            .with_information_schema(true)
            .with_parquet_bloom_filter_pruning(true)
            .with_parquet_page_index_pruning(true)
            .with_target_partitions(self.target_partitions)
            .with_batch_size(self.batch_size)
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

    #[test]
    fn test_create_session_with_config() {
        let settings = DataFusionSettings {
            query_parallelism: 4,
            max_partitions: 8,
            batch_size: 4096,
            memory_limit: 1073741824,
        };
        let factory = DataFusionSessionFactory::with_config(&settings).unwrap();

        // Verify factory uses config values
        assert_eq!(factory.target_partitions, 4);
        assert_eq!(factory.batch_size, 4096);

        let session = factory.create_session();
        assert!(session.catalog("kalam").is_some());
    }

    #[test]
    fn test_auto_detect_parallelism() {
        let settings = DataFusionSettings {
            query_parallelism: 0, // Auto-detect
            max_partitions: 16,
            batch_size: 8192,
            memory_limit: 1073741824,
        };
        let factory = DataFusionSessionFactory::with_config(&settings).unwrap();

        // Should auto-detect CPU cores (at least 1)
        assert!(factory.target_partitions >= 1);
        assert!(factory.target_partitions <= 16); // Capped at max_partitions
    }
}
