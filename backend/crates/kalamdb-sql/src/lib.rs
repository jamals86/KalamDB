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

    /// Get a storage location by name
    pub fn get_storage_location(&self, location_name: &str) -> Result<Option<StorageLocation>> {
        self.adapter.get_storage_location(location_name)
    }

    /// Insert a storage location
    pub fn insert_storage_location(&self, location: &StorageLocation) -> Result<()> {
        self.adapter.insert_storage_location(location)
    }

    /// Get a live query by ID
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQuery>> {
        self.adapter.get_live_query(live_id)
    }

    /// Insert a live query
    pub fn insert_live_query(&self, live_query: &LiveQuery) -> Result<()> {
        self.adapter.insert_live_query(live_query)
    }

    /// Get a job by ID
    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>> {
        self.adapter.get_job(job_id)
    }

    /// Insert a job
    pub fn insert_job(&self, job: &Job) -> Result<()> {
        self.adapter.insert_job(job)
    }

    // Scan operations for all system tables

    /// Scan all users
    ///
    /// Returns a vector of all users in the system.
    pub fn scan_all_users(&self) -> Result<Vec<User>> {
        self.adapter.scan_all_users()
    }

    /// Scan all namespaces
    ///
    /// Returns a vector of all namespaces in the system.
    pub fn scan_all_namespaces(&self) -> Result<Vec<Namespace>> {
        self.adapter.scan_all_namespaces()
    }

    /// Scan all storage locations
    ///
    /// Returns a vector of all storage locations in the system.
    pub fn scan_all_storage_locations(&self) -> Result<Vec<StorageLocation>> {
        self.adapter.scan_all_storage_locations()
    }

    /// Scan all live queries
    ///
    /// Returns a vector of all active live queries in the system.
    pub fn scan_all_live_queries(&self) -> Result<Vec<LiveQuery>> {
        self.adapter.scan_all_live_queries()
    }

    /// Scan all jobs
    ///
    /// Returns a vector of all jobs in the system.
    pub fn scan_all_jobs(&self) -> Result<Vec<Job>> {
        self.adapter.scan_all_jobs()
    }

    /// Scan all tables
    ///
    /// Returns a vector of all tables in the system.
    pub fn scan_all_tables(&self) -> Result<Vec<Table>> {
        self.adapter.scan_all_tables()
    }

    /// Insert a new table
    ///
    /// Inserts table metadata into system_tables.
    pub fn insert_table(&self, table: &Table) -> Result<()> {
        self.adapter.insert_table(table)
    }

    /// Scan all table schemas
    ///
    /// Returns a vector of all table schemas in the system.
    pub fn scan_all_table_schemas(&self) -> Result<Vec<TableSchema>> {
        self.adapter.scan_all_table_schemas()
    }

    // Additional CRUD operations for table deletion and updates

    /// Delete a table by table_id
    ///
    /// Removes the table metadata from system_tables.
    pub fn delete_table(&self, table_id: &str) -> Result<()> {
        self.adapter.delete_table(table_id)
    }

    /// Delete all table schemas for a given table_id
    ///
    /// Removes all schema versions for the specified table from system_table_schemas.
    pub fn delete_table_schemas_for_table(&self, table_id: &str) -> Result<()> {
        self.adapter.delete_table_schemas_for_table(table_id)
    }

    /// Update a storage location
    ///
    /// Updates an existing storage location (upsert).
    pub fn update_storage_location(&self, location: &StorageLocation) -> Result<()> {
        self.adapter.update_storage_location(location)
    }

    /// Update a job
    ///
    /// Updates an existing job (upsert), typically for status changes.
    pub fn update_job(&self, job: &Job) -> Result<()> {
        self.adapter.update_job(job)
    }

    /// Update a table
    ///
    /// Updates existing table metadata (upsert).
    pub fn update_table(&self, table: &Table) -> Result<()> {
        self.adapter.update_table(table)
    }

    /// Get all table schemas for a given table_id
    ///
    /// Returns all schema versions for the specified table, sorted by version descending.
    pub fn get_table_schemas_for_table(&self, table_id: &str) -> Result<Vec<TableSchema>> {
        self.adapter.get_table_schemas_for_table(table_id)
    }

    /// Get a table by table_id
    ///
    /// Fetches table metadata from system_tables.
    pub fn get_table(&self, table_id: &str) -> Result<Option<Table>> {
        self.adapter.get_table(table_id)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_kalamdb_sql_creation() {
        // This test would require a RocksDB instance
        // Will be implemented in integration tests
    }
}
