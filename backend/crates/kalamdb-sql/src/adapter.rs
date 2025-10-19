//! RocksDB adapter for system table operations
//!
//! Provides low-level read/write operations for system tables in RocksDB.

use crate::models::*;
use anyhow::{anyhow, Result};
use rocksdb::DB;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation() {
        // This test would require a RocksDB instance
        // Will be implemented in integration tests
    }
}
