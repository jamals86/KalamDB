//! KalamSQL - Unified SQL interface for KalamDB system tables
//!
//! This crate provides a SQL-based interface for managing KalamDB's 7 system tables:
//! - system_users: User management
//! - system_live_queries: Live query subscriptions  
//! - system_storage_locations: Storage location definitions
//! - system_jobs: Background job tracking
//! - system_namespaces: Namespace metadata
//! - system_tables: Table metadata
//! - system_table_schemas: Table schema versions
//!
//! All system metadata is stored in RocksDB column families, eliminating JSON config files.
//!
//! # Example
//!
//! ```no_run
//! use kalamdb_sql::KalamSql;
//! use rocksdb::DB;
//! use std::sync::Arc;
//!
//! # fn example() -> anyhow::Result<()> {
//! # let db = Arc::new(DB::open_default("test_db")?);
//! // Execute SQL against system tables
//! let kalamdb = KalamSql::new(db)?;
//! let results = kalamdb.execute("SELECT * FROM system.users WHERE username = 'alice'")?;
//!
//! // Typed helpers for common operations
//! let user = kalamdb.get_user("alice")?;
//! kalamdb.insert_namespace("my_app", "{}")?;
//! # Ok(())
//! # }
//! ```

pub mod adapter;
pub mod executor;
pub mod models;
pub mod parser;

pub use adapter::RocksDbAdapter;
pub use executor::SqlExecutor;
pub use models::*;
pub use parser::SqlParser;

use anyhow::Result;
use rocksdb::DB;
use std::sync::Arc;

/// Main SQL interface for system tables
pub struct KalamSql {
    adapter: RocksDbAdapter,
    parser: SqlParser,
    executor: SqlExecutor,
}

impl KalamSql {
    /// Create a new KalamSQL instance
    pub fn new(db: Arc<DB>) -> Result<Self> {
        let adapter = RocksDbAdapter::new(db);
        let parser = SqlParser::new();
        let executor = SqlExecutor::new(adapter.clone());

        Ok(Self {
            adapter,
            parser,
            executor,
        })
    }

    /// Execute a SQL statement against system tables
    ///
    /// Supports SELECT, INSERT, UPDATE, DELETE for all 7 system tables.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use kalamdb_sql::KalamSql;
    /// # fn example(kalamdb: &KalamSql) -> anyhow::Result<()> {
    /// let results = kalamdb.execute("SELECT * FROM system.users")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn execute(&self, sql: &str) -> Result<Vec<serde_json::Value>> {
        let statement = self.parser.parse(sql)?;
        self.executor.execute(statement)
    }

    // Typed helper methods

    /// Get a user by username
    pub fn get_user(&self, username: &str) -> Result<Option<User>> {
        self.adapter.get_user(username)
    }

    /// Insert a new user
    pub fn insert_user(&self, user: &User) -> Result<()> {
        self.adapter.insert_user(user)
    }

    /// Get a namespace by ID
    pub fn get_namespace(&self, namespace_id: &str) -> Result<Option<Namespace>> {
        self.adapter.get_namespace(namespace_id)
    }

    /// Insert a new namespace
    pub fn insert_namespace(&self, namespace_id: &str, options: &str) -> Result<()> {
        let namespace = Namespace {
            namespace_id: namespace_id.to_string(),
            name: namespace_id.to_string(),
            created_at: chrono::Utc::now().timestamp(),
            options: options.to_string(),
            table_count: 0,
        };
        self.adapter.insert_namespace(&namespace)
    }

    /// Get table schema by table_id and version
    pub fn get_table_schema(
        &self,
        table_id: &str,
        version: Option<i32>,
    ) -> Result<Option<TableSchema>> {
        self.adapter.get_table_schema(table_id, version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kalamdb_sql_creation() {
        // This test would require a RocksDB instance
        // Will be implemented in integration tests
    }
}
