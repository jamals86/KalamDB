//! RocksDB adapter for system table operations
//!
//! Provides low-level read/write operations for system tables in RocksDB.

use crate::models::*;
use anyhow::{anyhow, Result};
use rocksdb::{IteratorMode, DB};
use std::sync::Arc;

/// RocksDB adapter for system tables
#[derive(Clone)]
pub struct RocksDbAdapter {
    db: Arc<DB>,
}

impl RocksDbAdapter {
    /// Create a new RocksDB adapter
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    // User operations

    /// Get a user by username
    pub fn get_user(&self, username: &str) -> Result<Option<User>> {
        let cf = self
            .db
            .cf_handle("system_users")
            .ok_or_else(|| anyhow!("system_users CF not found"))?;

        let key = format!("user:{}", username);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let user: User = serde_json::from_slice(&value)?;
                Ok(Some(user))
            }
            None => Ok(None),
        }
    }

    /// Insert a new user
    pub fn insert_user(&self, user: &User) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_users")
            .ok_or_else(|| anyhow!("system_users CF not found"))?;

        let key = format!("user:{}", user.username);
        let value = serde_json::to_vec(user)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Update a user
    pub fn update_user(&self, user: &User) -> Result<()> {
        self.insert_user(user) // Same as insert for now
    }

    /// Delete a user
    pub fn delete_user(&self, username: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_users")
            .ok_or_else(|| anyhow!("system_users CF not found"))?;

        let key = format!("user:{}", username);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    // Namespace operations

    /// Get a namespace by ID
    pub fn get_namespace(&self, namespace_id: &str) -> Result<Option<Namespace>> {
        let cf = self
            .db
            .cf_handle("system_namespaces")
            .ok_or_else(|| anyhow!("system_namespaces CF not found"))?;

        let key = format!("ns:{}", namespace_id);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let namespace: Namespace = serde_json::from_slice(&value)?;
                Ok(Some(namespace))
            }
            None => Ok(None),
        }
    }

    /// Insert a new namespace
    pub fn insert_namespace(&self, namespace: &Namespace) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_namespaces")
            .ok_or_else(|| anyhow!("system_namespaces CF not found"))?;

        let key = format!("ns:{}", namespace.namespace_id);
        let value = serde_json::to_vec(namespace)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a namespace by namespace_id
    pub fn delete_namespace(&self, namespace_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_namespaces")
            .ok_or_else(|| anyhow!("system_namespaces CF not found"))?;

        let key = format!("ns:{}", namespace_id);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    // Table schema operations

    /// Get table schema by table_id and version
    ///
    /// **DEPRECATED**: This method is maintained for backward compatibility.
    /// New code should use `get_table_definition()` which provides complete table metadata.
    ///
    /// If version is None, returns the latest version from information_schema.tables.
    /// If version is Some(v), returns the specific version from schema_history.
    pub fn get_table_schema(
        &self,
        table_id: &str,
        version: Option<i32>,
    ) -> Result<Option<TableSchema>> {
        // Parse table_id (format: "namespace:table_name")
        let parts: Vec<&str> = table_id.split(':').collect();
        if parts.len() != 2 {
            return Err(anyhow!(
                "Invalid table_id format. Expected 'namespace:table_name', got '{}'",
                table_id
            ));
        }
        let namespace_id = parts[0];
        let table_name = parts[1];

        // Get table definition from information_schema.tables
        let table_def = self.get_table_definition(namespace_id, table_name)?;

        match table_def {
            None => Ok(None),
            Some(def) => {
                match version {
                    None => {
                        // Return latest schema from TableDefinition
                        let latest_schema_ver = def
                            .schema_history
                            .last()
                            .ok_or_else(|| anyhow!("No schema versions found for {}", table_id))?;

                        Ok(Some(TableSchema {
                            schema_id: format!("{}:{}", table_id, def.schema_version),
                            table_id: table_id.to_string(),
                            version: def.schema_version as i32,
                            arrow_schema: latest_schema_ver.arrow_schema_json.clone(),
                            created_at: def.created_at,
                            changes: serde_json::to_string(&latest_schema_ver.changes)
                                .unwrap_or_else(|_| "[]".to_string()),
                        }))
                    }
                    Some(v) => {
                        // Find specific version in schema_history
                        let schema_ver = def
                            .schema_history
                            .iter()
                            .find(|sv| sv.version == v as u32)
                            .ok_or_else(|| {
                                anyhow!("Schema version {} not found for {}", v, table_id)
                            })?;

                        Ok(Some(TableSchema {
                            schema_id: format!("{}:{}", table_id, v),
                            table_id: table_id.to_string(),
                            version: v,
                            arrow_schema: schema_ver.arrow_schema_json.clone(),
                            created_at: schema_ver.created_at,
                            changes: serde_json::to_string(&schema_ver.changes)
                                .unwrap_or_else(|_| "[]".to_string()),
                        }))
                    }
                }
            }
        }
    }

    /// Insert column metadata into system.columns
    ///
    /// Stores metadata about a single column including DEFAULT expression,
    /// data type, nullability, and ordinal position.
    ///
    /// # Arguments
    /// * `table_id` - Table identifier (format: "namespace:table_name")
    /// * `column_name` - Name of the column
    /// * `data_type` - Arrow DataType as string (e.g., "Int64", "Utf8")
    /// * `is_nullable` - Whether column accepts NULL values
    /// * `ordinal_position` - 1-indexed column position in table
    /// * `default_expression` - Optional DEFAULT expression (e.g., "NOW()", "SNOWFLAKE_ID()")
    pub fn insert_column_metadata(
        &self,
        table_id: &str,
        column_name: &str,
        data_type: &str,
        is_nullable: bool,
        ordinal_position: i32,
        default_expression: Option<&str>,
    ) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_columns")
            .ok_or_else(|| anyhow!("system_columns CF not found"))?;

        // Key format: {table_id}:{column_name}
        let key = format!("{}:{}", table_id, column_name);

        let column_meta = serde_json::json!({
            "table_id": table_id,
            "column_name": column_name,
            "data_type": data_type,
            "is_nullable": is_nullable,
            "ordinal_position": ordinal_position,
            "default_expression": default_expression,
        });

        let value = serde_json::to_vec(&column_meta)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    // Live query operations

    /// Get a live query by ID
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQuery>> {
        let cf = self
            .db
            .cf_handle("system_live_queries")
            .ok_or_else(|| anyhow!("system_live_queries CF not found"))?;

        let key = format!("lq:{}", live_id);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let live_query: LiveQuery = serde_json::from_slice(&value)?;
                Ok(Some(live_query))
            }
            None => Ok(None),
        }
    }

    /// Insert a new live query
    pub fn insert_live_query(&self, live_query: &LiveQuery) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_live_queries")
            .ok_or_else(|| anyhow!("system_live_queries CF not found"))?;

        let key = format!("lq:{}", live_query.live_id);
        let value = serde_json::to_vec(live_query)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a live query by ID
    pub fn delete_live_query(&self, live_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_live_queries")
            .ok_or_else(|| anyhow!("system_live_queries CF not found"))?;

        let key = format!("lq:{}", live_id);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    // Job operations

    /// Get a job by ID
    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>> {
        let cf = self
            .db
            .cf_handle("system_jobs")
            .ok_or_else(|| anyhow!("system_jobs CF not found"))?;

        let key = format!("job:{}", job_id);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let job: Job = serde_json::from_slice(&value)?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Insert a new job
    pub fn insert_job(&self, job: &Job) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_jobs")
            .ok_or_else(|| anyhow!("system_jobs CF not found"))?;

        let key = format!("job:{}", job.job_id);
        let value = serde_json::to_vec(job)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a job by ID
    pub fn delete_job(&self, job_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_jobs")
            .ok_or_else(|| anyhow!("system_jobs CF not found"))?;

        let key = format!("job:{}", job_id);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    // Table operations

    /// Get a table by ID
    pub fn get_table(&self, table_id: &str) -> Result<Option<Table>> {
        let cf = self
            .db
            .cf_handle("system_tables")
            .ok_or_else(|| anyhow!("system_tables CF not found"))?;

        let key = format!("table:{}", table_id);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let table: Table = serde_json::from_slice(&value)?;
                Ok(Some(table))
            }
            None => Ok(None),
        }
    }

    /// Insert a new table
    pub fn insert_table(&self, table: &Table) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_tables")
            .ok_or_else(|| anyhow!("system_tables CF not found"))?;

        let key = format!("table:{}", table.table_id);
        let value = serde_json::to_vec(table)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
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
        let cf = self
            .db
            .cf_handle("information_schema_tables")
            .ok_or_else(|| anyhow!("information_schema_tables CF not found"))?;

        let key = format!("{}:{}", table_def.namespace_id, table_def.table_name);
        let value = serde_json::to_vec(table_def)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
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
        let cf = self
            .db
            .cf_handle("information_schema_tables")
            .ok_or_else(|| anyhow!("information_schema_tables CF not found"))?;

        let key = format!("{}:{}", namespace_id, table_name);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let table_def: kalamdb_commons::models::TableDefinition =
                    serde_json::from_slice(&value)?;
                Ok(Some(table_def))
            }
            None => Ok(None),
        }
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
        let cf = self
            .db
            .cf_handle("information_schema_tables")
            .ok_or_else(|| anyhow!("information_schema_tables CF not found"))?;

        let prefix = format!("{}:", namespace_id);
        let mut tables = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            if key_str.starts_with(&prefix) {
                let table_def: kalamdb_commons::models::TableDefinition =
                    serde_json::from_slice(&value)?;
                tables.push(table_def);
            }
        }

        Ok(tables)
    }

    /// Scan ALL table definitions across ALL namespaces
    ///
    /// # Returns
    /// Vector of all TableDefinition in the database
    pub fn scan_all_table_definitions(
        &self,
    ) -> Result<Vec<kalamdb_commons::models::TableDefinition>> {
        let cf = self
            .db
            .cf_handle("information_schema_tables")
            .ok_or_else(|| anyhow!("information_schema_tables CF not found"))?;

        let mut tables = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (_key, value) = item?;
            let table_def: kalamdb_commons::models::TableDefinition =
                serde_json::from_slice(&value)?;
            tables.push(table_def);
        }

        Ok(tables)
    }

    // Storage operations

    /// Get a storage by ID
    pub fn get_storage(&self, storage_id: &str) -> Result<Option<Storage>> {
        let cf = self
            .db
            .cf_handle("system_storages")
            .ok_or_else(|| anyhow!("system_storages CF not found"))?;

        let key = format!("storage:{}", storage_id);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let storage: Storage = serde_json::from_slice(&value)?;
                Ok(Some(storage))
            }
            None => Ok(None),
        }
    }

    /// Insert a new storage
    pub fn insert_storage(&self, storage: &Storage) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_storages")
            .ok_or_else(|| anyhow!("system_storages CF not found"))?;

        let key = format!("storage:{}", storage.storage_id);
        let value = serde_json::to_vec(storage)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a storage by ID
    pub fn delete_storage(&self, storage_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_storages")
            .ok_or_else(|| anyhow!("system_storages CF not found"))?;

        let key = format!("storage:{}", storage_id);
        self.db.delete_cf(&cf, key.as_bytes())?;
        Ok(())
    }

    // Scan operations for all system tables

    /// Scan all users
    pub fn scan_all_users(&self) -> Result<Vec<User>> {
        let cf = self
            .db
            .cf_handle("system_users")
            .ok_or_else(|| anyhow!("system_users CF not found"))?;

        let mut users = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let user: User = serde_json::from_slice(&value)?;
            users.push(user);
        }

        Ok(users)
    }

    /// Scan all namespaces
    pub fn scan_all_namespaces(&self) -> Result<Vec<Namespace>> {
        let cf = self
            .db
            .cf_handle("system_namespaces")
            .ok_or_else(|| anyhow!("system_namespaces CF not found"))?;

        let mut namespaces = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let namespace: Namespace = serde_json::from_slice(&value)?;
            namespaces.push(namespace);
        }

        Ok(namespaces)
    }

    /// Scan all live queries
    pub fn scan_all_live_queries(&self) -> Result<Vec<LiveQuery>> {
        let cf = self
            .db
            .cf_handle("system_live_queries")
            .ok_or_else(|| anyhow!("system_live_queries CF not found"))?;

        let mut live_queries = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let live_query: LiveQuery = serde_json::from_slice(&value)?;
            live_queries.push(live_query);
        }

        Ok(live_queries)
    }

    /// Scan all jobs
    pub fn scan_all_jobs(&self) -> Result<Vec<Job>> {
        let cf = self
            .db
            .cf_handle("system_jobs")
            .ok_or_else(|| anyhow!("system_jobs CF not found"))?;

        let mut jobs = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let job: Job = serde_json::from_slice(&value)?;
            jobs.push(job);
        }

        Ok(jobs)
    }

    /// Scan all tables
    pub fn scan_all_tables(&self) -> Result<Vec<Table>> {
        let cf = self
            .db
            .cf_handle("system_tables")
            .ok_or_else(|| anyhow!("system_tables CF not found"))?;

        let mut tables = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let table: Table = serde_json::from_slice(&value)?;
            tables.push(table);
        }

        Ok(tables)
    }

    /// Scan all storages
    pub fn scan_all_storages(&self) -> Result<Vec<Storage>> {
        let cf = self
            .db
            .cf_handle("system_storages")
            .ok_or_else(|| anyhow!("system_storages CF not found"))?;

        let mut storages = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let storage: Storage = serde_json::from_slice(&value)?;
            storages.push(storage);
        }

        Ok(storages)
    }

    // Additional CRUD operations for table deletion and updates

    /// Delete a table by table_id
    pub fn delete_table(&self, table_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_tables")
            .ok_or_else(|| anyhow!("system_tables CF not found"))?;

        let key = format!("table:{}", table_id);
        self.db.delete_cf(&cf, key.as_bytes())?;

        // Flush to ensure delete is immediately visible
        self.db.flush_cf(&cf)?;
        Ok(())
    }

    /// Update a job
    pub fn update_job(&self, job: &Job) -> Result<()> {
        self.insert_job(job) // Same as insert (upsert)
    }

    /// Update a table
    pub fn update_table(&self, table: &Table) -> Result<()> {
        self.insert_table(table) // Same as insert (upsert)
    }

    /// Update a storage
    pub fn update_storage(&self, storage: &Storage) -> Result<()> {
        self.insert_storage(storage) // Same as insert (upsert)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_adapter_creation() {
        // This test would require a RocksDB instance
        // Will be implemented in integration tests
    }
}
