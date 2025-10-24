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
pub mod compatibility;
pub mod ddl;
pub mod executor;
pub mod models;
pub mod parser;
pub mod query_cache;
pub mod statement_classifier;

pub use adapter::RocksDbAdapter;
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
pub use executor::SqlExecutor;
pub use models::*;
pub use parser::SqlParser;
pub use query_cache::{QueryCache, QueryCacheKey, QueryCacheTtlConfig};

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

    /// Check if a namespace exists.
    pub fn namespace_exists(&self, namespace_id: &str) -> Result<bool> {
        self.adapter.namespace_exists(namespace_id)
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

    /// Insert a namespace struct directly
    pub fn insert_namespace_struct(&self, namespace: &Namespace) -> Result<()> {
        self.adapter.insert_namespace(namespace)
    }

    /// Delete a namespace by namespace_id
    pub fn delete_namespace(&self, namespace_id: &str) -> Result<()> {
        self.adapter.delete_namespace(namespace_id)
    }

    /// Get table schema by table_id and version
    pub fn get_table_schema(
        &self,
        table_id: &str,
        version: Option<i32>,
    ) -> Result<Option<TableSchema>> {
        self.adapter.get_table_schema(table_id, version)
    }

    /// Insert column metadata
    ///
    /// Stores metadata for a single column including DEFAULT expression,
    /// data type, nullability, and ordinal position.
    pub fn insert_column_metadata(
        &self,
        table_id: &str,
        column_name: &str,
        data_type: &str,
        is_nullable: bool,
        ordinal_position: i32,
        default_expression: Option<&str>,
    ) -> Result<()> {
        self.adapter.insert_column_metadata(
            table_id,
            column_name,
            data_type,
            is_nullable,
            ordinal_position,
            default_expression,
        )
    }

    /// Get a live query by ID
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQuery>> {
        self.adapter.get_live_query(live_id)
    }

    /// Insert a live query
    pub fn insert_live_query(&self, live_query: &LiveQuery) -> Result<()> {
        self.adapter.insert_live_query(live_query)
    }

    /// Delete a live query by ID
    pub fn delete_live_query(&self, live_id: &str) -> Result<()> {
        self.adapter.delete_live_query(live_id)
    }

    /// Get a job by ID
    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>> {
        self.adapter.get_job(job_id)
    }

    /// Insert a job
    pub fn insert_job(&self, job: &Job) -> Result<()> {
        self.adapter.insert_job(job)
    }

    /// Delete a job by ID
    pub fn delete_job(&self, job_id: &str) -> Result<()> {
        self.adapter.delete_job(job_id)
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

    /// Scan all storages
    ///
    /// Returns a vector of all storages in the system.
    pub fn scan_all_storages(&self) -> Result<Vec<Storage>> {
        self.adapter.scan_all_storages()
    }

    /// Get a storage by ID
    pub fn get_storage(&self, storage_id: &str) -> Result<Option<Storage>> {
        self.adapter.get_storage(storage_id)
    }

    /// Insert a new storage
    pub fn insert_storage(&self, storage: &Storage) -> Result<()> {
        self.adapter.insert_storage(storage)
    }

    /// Delete a storage by ID
    pub fn delete_storage(&self, storage_id: &str) -> Result<()> {
        self.adapter.delete_storage(storage_id)
    }

    // Additional CRUD operations for table deletion and updates

    /// Delete a table by table_id
    ///
    /// Removes the table metadata from system_tables.
    pub fn delete_table(&self, table_id: &str) -> Result<()> {
        self.adapter.delete_table(table_id)
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

    /// Get a table by table_id
    ///
    /// Fetches table metadata from system_tables.
    pub fn get_table(&self, table_id: &str) -> Result<Option<Table>> {
        self.adapter.get_table(table_id)
    }

    // ===================================
    // information_schema.tables Operations
    // ===================================

    /// Insert or update complete table definition in information_schema_tables.
    /// Single atomic write for all table metadata (replaces fragmented writes).
    ///
    /// # Arguments
    /// * `table_def` - Complete table definition with metadata, columns, and schema history
    ///
    /// # Returns
    /// Ok(()) on success, error on failure
    pub fn upsert_table_definition(
        &self,
        table_def: &kalamdb_commons::models::TableDefinition,
    ) -> Result<()> {
        self.adapter.upsert_table_definition(table_def)
    }

    /// Get complete table definition from information_schema_tables.
    /// Single atomic read for all table metadata.
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace identifier
    /// * `table_name` - Table name
    ///
    /// # Returns
    /// Some(TableDefinition) if found, None if not found
    pub fn get_table_definition(
        &self,
        namespace_id: &str,
        table_name: &str,
    ) -> Result<Option<kalamdb_commons::models::TableDefinition>> {
        self.adapter.get_table_definition(namespace_id, table_name)
    }

    /// Scan all table definitions in a namespace from information_schema_tables.
    /// Used for SHOW TABLES and metadata queries.
    ///
    /// # Arguments
    /// * `namespace_id` - Namespace identifier
    ///
    /// # Returns
    /// Vector of all TableDefinition in the namespace
    pub fn scan_table_definitions(
        &self,
        namespace_id: &str,
    ) -> Result<Vec<kalamdb_commons::models::TableDefinition>> {
        self.adapter.scan_table_definitions(namespace_id)
    }

    /// Scan ALL table definitions across ALL namespaces
    ///
    /// # Returns
    /// Vector of all TableDefinition in the database
    pub fn scan_all_table_definitions(
        &self,
    ) -> Result<Vec<kalamdb_commons::models::TableDefinition>> {
        self.adapter.scan_all_table_definitions()
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
