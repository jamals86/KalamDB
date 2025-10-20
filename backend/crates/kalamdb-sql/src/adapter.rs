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
    pub fn get_table_schema(
        &self,
        table_id: &str,
        version: Option<i32>,
    ) -> Result<Option<TableSchema>> {
        let cf = self
            .db
            .cf_handle("system_table_schemas")
            .ok_or_else(|| anyhow!("system_table_schemas CF not found"))?;

        let key = match version {
            Some(v) => format!("schema:{}:{}", table_id, v),
            None => {
                // Get latest version
                // TODO: Implement proper versioning lookup
                format!("schema:{}:1", table_id)
            }
        };

        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let schema: TableSchema = serde_json::from_slice(&value)?;
                Ok(Some(schema))
            }
            None => Ok(None),
        }
    }

    /// Insert a new table schema
    pub fn insert_table_schema(&self, schema: &TableSchema) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_table_schemas")
            .ok_or_else(|| anyhow!("system_table_schemas CF not found"))?;

        let key = format!("schema:{}:{}", schema.table_id, schema.version);
        let value = serde_json::to_vec(schema)?;
        self.db.put_cf(&cf, key.as_bytes(), &value)?;
        Ok(())
    }

    // Storage location operations

    /// Get a storage location by name
    pub fn get_storage_location(&self, location_name: &str) -> Result<Option<StorageLocation>> {
        let cf = self
            .db
            .cf_handle("system_storage_locations")
            .ok_or_else(|| anyhow!("system_storage_locations CF not found"))?;

        let key = format!("loc:{}", location_name);
        match self.db.get_cf(&cf, key.as_bytes())? {
            Some(value) => {
                let location: StorageLocation = serde_json::from_slice(&value)?;
                Ok(Some(location))
            }
            None => Ok(None),
        }
    }

    /// Insert a new storage location
    pub fn insert_storage_location(&self, location: &StorageLocation) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_storage_locations")
            .ok_or_else(|| anyhow!("system_storage_locations CF not found"))?;

        let key = format!("loc:{}", location.location_name);
        let value = serde_json::to_vec(location)?;
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

    /// Scan all storage locations
    pub fn scan_all_storage_locations(&self) -> Result<Vec<StorageLocation>> {
        let cf = self
            .db
            .cf_handle("system_storage_locations")
            .ok_or_else(|| anyhow!("system_storage_locations CF not found"))?;

        let mut locations = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let location: StorageLocation = serde_json::from_slice(&value)?;
            locations.push(location);
        }

        Ok(locations)
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

    /// Scan all table schemas
    pub fn scan_all_table_schemas(&self) -> Result<Vec<TableSchema>> {
        let cf = self
            .db
            .cf_handle("system_table_schemas")
            .ok_or_else(|| anyhow!("system_table_schemas CF not found"))?;

        let mut schemas = Vec::new();
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        for item in iter {
            let (_, value) = item?;
            let schema: TableSchema = serde_json::from_slice(&value)?;
            schemas.push(schema);
        }

        Ok(schemas)
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

    /// Delete all table schemas for a given table_id
    pub fn delete_table_schemas_for_table(&self, table_id: &str) -> Result<()> {
        let cf = self
            .db
            .cf_handle("system_table_schemas")
            .ok_or_else(|| anyhow!("system_table_schemas CF not found"))?;

        // Iterate and delete all schemas with prefix "schema:{table_id}:"
        let prefix = format!("schema:{}:", table_id);
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        let mut keys_to_delete = Vec::new();
        for item in iter {
            let (key_bytes, _) = item?;
            let key = String::from_utf8(key_bytes.to_vec())?;
            if key.starts_with(&prefix) {
                keys_to_delete.push(key);
            }
        }

        // Delete all matching keys
        for key in keys_to_delete {
            self.db.delete_cf(&cf, key.as_bytes())?;
        }

        Ok(())
    }

    /// Update a storage location
    pub fn update_storage_location(&self, location: &StorageLocation) -> Result<()> {
        self.insert_storage_location(location) // Same as insert (upsert)
    }

    /// Update a job
    pub fn update_job(&self, job: &Job) -> Result<()> {
        self.insert_job(job) // Same as insert (upsert)
    }

    /// Update a table
    pub fn update_table(&self, table: &Table) -> Result<()> {
        self.insert_table(table) // Same as insert (upsert)
    }

    /// Get all table schemas for a given table_id
    pub fn get_table_schemas_for_table(&self, table_id: &str) -> Result<Vec<TableSchema>> {
        let cf = self
            .db
            .cf_handle("system_table_schemas")
            .ok_or_else(|| anyhow!("system_table_schemas CF not found"))?;

        let prefix = format!("schema:{}:", table_id);
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);

        let mut schemas = Vec::new();
        for item in iter {
            let (key_bytes, value) = item?;
            let key = String::from_utf8(key_bytes.to_vec())?;
            if key.starts_with(&prefix) {
                let schema: TableSchema = serde_json::from_slice(&value)?;
                schemas.push(schema);
            }
        }

        // Sort by version descending (newest first)
        schemas.sort_by(|a, b| b.version.cmp(&a.version));

        Ok(schemas)
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
