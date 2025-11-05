//! KalamSQL - Unified SQL interface for KalamDB system tables
//!
//! This crate provides a SQL-based interface for managing KalamDB's system tables:
//! - system_users: User management
//! - system_live_queries: Live query subscriptions  
//! - system_jobs: Background job tracking
//! - system_namespaces: Namespace metadata
//! - system_tables: Table metadata
//! - system_storages: Storage backend configurations
//! - information_schema_tables: Unified table metadata (replaces system_table_schemas + system_columns)
//!
//! All system metadata is stored in RocksDB column families, eliminating JSON config files.
//!
//! # Example
//!
//! ```no_run
//! use kalamdb_store::{RocksDBBackend, storage_trait::StorageBackend};
//! use std::sync::Arc;
//!
//! # fn example() -> anyhow::Result<()> {
//! # let db = std::sync::Arc::new(rocksdb::DB::open_default("test_db")?);
//! # let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db));
//! // Execute SQL against system tables
//! let results = kalamdb.execute("SELECT * FROM system.users WHERE username = 'alice'")?;
//!
//! // Typed helpers for common operations
//! let user = kalamdb.get_user("alice")?;
//! kalamdb.insert_namespace("my_app", "{}")?;
//! # Ok(())
//! # }
//! ```

// ============================================================================
// PHASE 6: adapter module disabled (StorageAdapter removal)
// ============================================================================
// pub mod adapter; // PHASE 6: Disabled - file renamed to adapter.rs.disabled

pub mod batch_execution;
pub mod compatibility;
pub mod ddl;
pub mod parser;
pub mod query_cache;
pub mod statement_classifier;

// Re-export system models from kalamdb-commons (single source of truth)
pub use kalamdb_commons::system::{
    AuditLogEntry, Job, LiveQuery, Namespace, Storage,
    User, UserTableCounter,
};

// pub use adapter::StorageAdapter; // PHASE 6: Disabled
// Backwards-compatibility alias, TODO: Remove the alias and use only StorageAdapter
// pub type RocksDbAdapter = StorageAdapter; // PHASE 6: Disabled
pub use batch_execution::split_statements;
pub use compatibility::{
    format_mysql_column_not_found, format_mysql_error, format_mysql_syntax_error,
    format_mysql_table_not_found, format_postgres_column_not_found, format_postgres_error,
    format_postgres_syntax_error, format_postgres_table_not_found, map_sql_type_to_arrow,
    ErrorStyle,
};
pub use ddl::{
    parse_job_command, AlterStorageStatement, CreateStorageStatement, DropStorageStatement,
    FlushAllTablesStatement, FlushTableStatement, JobCommand, ShowStoragesStatement,
    SubscribeOptions, SubscribeStatement,
};
pub use parser::SqlParser;
pub use query_cache::{QueryCache, QueryCacheKey, QueryCacheTtlConfig};

// ============================================================================
// PHASE 6: KalamSql STRUCT COMMENTED OUT
// ============================================================================
// This struct is being removed in Phase 6. All usages should migrate to
// SystemTablesRegistry providers instead.
// Uncomment to see all places that need migration.
// ============================================================================

