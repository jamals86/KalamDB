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
//! use kalamdb_store::{RocksDBBackend, storage_trait::StorageBackend};
//! use std::sync::Arc;
//!
//! # fn example() -> anyhow::Result<()> {
//! # let db = std::sync::Arc::new(rocksdb::DB::open_default("test_db")?);
//! # let backend: Arc<dyn StorageBackend> = Arc::new(RocksDBBackend::new(db));
//! // Execute SQL against system tables
//! let kalamdb = KalamSql::new(backend)?;
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

use kalamdb_commons::{NamespaceId, StorageId, TableName, UserId};
use kalamdb_store::StorageBackend;
// Re-export system models from kalamdb-commons (single source of truth)
pub use kalamdb_commons::system::{
    AuditLogEntry, InformationSchemaTable, Job, LiveQuery, Namespace, Storage,
    SystemTable as Table, TableSchema, User, UserTableCounter,
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

use anyhow::Result;
use std::sync::Arc;

// ============================================================================
// PHASE 6: KalamSql STRUCT COMMENTED OUT
// ============================================================================
// This struct is being removed in Phase 6. All usages should migrate to
// SystemTablesRegistry providers instead.
// Uncomment to see all places that need migration.
// ============================================================================


// ============================================================================
// PHASE 6: KalamSql STRUCT DISABLED
// ============================================================================
// All of KalamSql is commented out to find ALL usage sites.
// Compiler will show everywhere that needs migration to SystemTablesRegistry.
// ============================================================================

/*
/// Main SQL interface for system tables
/// 
/// NOTE (Phase 6): This struct is deprecated and will be removed.
/// New code should use SystemTablesRegistry providers instead.
/// Only kept for backward compatibility with background jobs.
pub struct KalamSql {
    adapter: StorageAdapter,
    parser: SqlParser,
}

impl KalamSql {
    /// Create a new KalamSQL instance using a generic storage backend
    pub fn new(backend: Arc<dyn StorageBackend>) -> Result<Self> {
        let adapter = StorageAdapter::new(backend);
        let parser = SqlParser::new();

        Ok(Self {
            adapter,
            parser,
        })
    }

    /// Get the underlying RocksDB adapter
    pub fn adapter(&self) -> &StorageAdapter {
        &self.adapter
    }

    // Typed helper methods
    // NOTE (Phase 6): These methods are deprecated. Use SystemTablesRegistry providers instead.

    /// Get a user by username
    pub fn get_user(&self, username: &str) -> Result<Option<User>> {
        self.adapter.get_user(username)
    }

    /// Get a user by user ID
    pub fn get_user_by_id(&self, user_id: &UserId) -> Result<Option<User>> {
        // Scan all users and find by ID
        // TODO: Add index for user_id for better performance
        let all_users = self.adapter.scan_all_users()?;
        Ok(all_users.into_iter().find(|u| &u.id == user_id))
    }

    /// Insert a new user
    pub fn insert_user(&self, user: &User) -> Result<()> {
        self.adapter.insert_user(user)
    }

    /// Update an existing user
    pub fn update_user(&self, user: &User) -> Result<()> {
        self.adapter.update_user(user)
    }

    /// Delete a user by username (soft delete)
    pub fn delete_user(&self, username: &str) -> Result<()> {
        self.adapter.delete_user(username)
    }

    /// Get a namespace by ID
    pub fn get_namespace(&self, namespace_id: &NamespaceId) -> Result<Option<Namespace>> {
        self.adapter.get_namespace(namespace_id.as_str())
    }

    /// Check if a namespace exists.
    pub fn namespace_exists(&self, namespace_id: &NamespaceId) -> Result<bool> {
        self.adapter.namespace_exists(namespace_id.as_str())
    }

    /// Insert a new namespace
    pub fn insert_namespace(&self, namespace_id: &NamespaceId, options: &str) -> Result<()> {
        let namespace = Namespace {
            namespace_id: namespace_id.clone(),
            name: namespace_id.as_str().to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            options: Some(options.to_string()),
            table_count: 0,
        };
        self.adapter.insert_namespace(&namespace)
    }

    /// Insert a namespace struct directly
    pub fn insert_namespace_struct(&self, namespace: &Namespace) -> Result<()> {
        self.adapter.insert_namespace(namespace)
    }

    /// Delete a namespace by namespace_id
    pub fn delete_namespace(&self, namespace_id: &NamespaceId) -> Result<()> {
        self.adapter.delete_namespace(namespace_id.as_str())
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

    /// Insert an audit log entry.
    pub fn insert_audit_log(&self, entry: &AuditLogEntry) -> Result<()> {
        self.adapter.insert_audit_log(entry)
    }

    /// Retrieve all audit log entries.
    pub fn scan_all_audit_logs(&self) -> Result<Vec<AuditLogEntry>> {
        self.adapter.scan_audit_logs()
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
    /// TODO: Use tablebyid index for better performance
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
    pub fn get_storage(&self, storage_id: &StorageId) -> Result<Option<Storage>> {
        self.adapter.get_storage(storage_id.as_str())
    }

    /// Insert a new storage
    pub fn insert_storage(&self, storage: &Storage) -> Result<()> {
        self.adapter.insert_storage(storage)
    }

    /// Delete a storage by ID
    pub fn delete_storage(&self, storage_id: &StorageId) -> Result<()> {
        self.adapter.delete_storage(storage_id.as_str())
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
    /// TODO:
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
        table_def: &kalamdb_commons::schemas::TableDefinition,
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
        namespace_id: &NamespaceId,
        table_name: &TableName,
    ) -> Result<Option<kalamdb_commons::schemas::TableDefinition>> {
        self.adapter
            .get_table_definition(namespace_id.as_str(), table_name.as_str())
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
        namespace_id: &NamespaceId,
    ) -> Result<Vec<kalamdb_commons::schemas::TableDefinition>> {
        self.adapter.scan_table_definitions(namespace_id.as_str())
    }

    /// Scan ALL table definitions across ALL namespaces
    ///
    /// # Returns
    /// Vector of all TableDefinition in the database
    pub fn scan_all_table_definitions(
        &self,
    ) -> Result<Vec<kalamdb_commons::schemas::TableDefinition>> {
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
*/ // END PHASE 6: KalamSql disabled
