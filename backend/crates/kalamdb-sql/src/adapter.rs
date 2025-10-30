//! RocksDB adapter for system table operations
//!
//! Provides low-level read/write operations for system tables in RocksDB.

// Import all system models from the crate root (which re-exports from commons)
use crate::{AuditLogEntry, Job, LiveQuery, Namespace, Storage, Table, TableSchema, User};
// use kalamdb_commons::models::TableDefinition; // Unused
use anyhow::{anyhow, Result};
use kalamdb_commons::models::AuditLogId;
use kalamdb_commons::{StoragePartition, SystemTable};
use kalamdb_store::{EntityStoreV2, StorageBackend};
use std::sync::Arc;

/// Storage adapter for system tables (backend-agnostic)
#[derive(Clone)]
pub struct StorageAdapter {
    backend: Arc<dyn StorageBackend>,
}

impl StorageAdapter {
    /// Create a new adapter backed by a generic storage backend
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self { backend }
    }

    /// Get the underlying storage backend
    ///
    /// This is useful for creating new EntityStore-based table providers
    /// that need direct access to the backend.
    pub fn backend(&self) -> Arc<dyn StorageBackend> {
        self.backend.clone()
    }

    // User operations

    /// Get a user by username
    pub fn get_user(&self, username: &str) -> Result<Option<User>> {
        let p = SystemTable::Users.partition();
        let key = format!("user:{}", username);
        match self.backend.get(&p.into(), key.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Insert an audit log entry
    pub fn insert_audit_log(&self, entry: &AuditLogEntry) -> Result<()> {
        // Create an entity store for audit logs using the entity_store module
        struct AuditLogEntityStore {
            backend: Arc<dyn StorageBackend>,
        }

        impl EntityStoreV2<AuditLogId, AuditLogEntry> for AuditLogEntityStore {
            fn backend(&self) -> &Arc<dyn StorageBackend> {
                &self.backend
            }

            fn partition(&self) -> &str {
                "system_audit_log"
            }
        }

        let store = AuditLogEntityStore {
            backend: self.backend.clone(),
        };
        EntityStoreV2::put(&store, &entry.audit_id, entry)?;
        Ok(())
    }

    /// Scan all audit log entries
    pub fn scan_audit_logs(&self) -> Result<Vec<AuditLogEntry>> {
        // Create an entity store for audit logs using the entity_store module
        struct AuditLogEntityStore {
            backend: Arc<dyn StorageBackend>,
        }

        impl EntityStoreV2<AuditLogId, AuditLogEntry> for AuditLogEntityStore {
            fn backend(&self) -> &Arc<dyn StorageBackend> {
                &self.backend
            }

            fn partition(&self) -> &str {
                "system_audit_log"
            }
        }

        let store = AuditLogEntityStore {
            backend: self.backend.clone(),
        };
        let entries: Vec<(Vec<u8>, AuditLogEntry)> = EntityStoreV2::scan_all(&store)?;
        Ok(entries.into_iter().map(|(_, entry)| entry).collect())
    }

    /// Insert a new user
    pub fn insert_user(&self, user: &User) -> Result<()> {
        let p = SystemTable::Users.partition();
        let key = format!("user:{}", user.username);
        let value = serde_json::to_vec(user)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
        Ok(())
    }

    /// Update a user
    pub fn update_user(&self, user: &User) -> Result<()> {
        self.insert_user(user) // Same as insert for now
    }

    /// Delete a user
    pub fn delete_user(&self, username: &str) -> Result<()> {
        let p = SystemTable::Users.partition();
        let key = format!("user:{}", username);
        self.backend.delete(&p.into(), key.as_bytes())?;
        Ok(())
    }

    // Namespace operations

    /// Get a namespace by ID
    pub fn get_namespace(&self, namespace_id: &str) -> Result<Option<Namespace>> {
        let p = SystemTable::Namespaces.partition();
        let key = format!("ns:{}", namespace_id);
        match self.backend.get(&p.into(), key.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Check whether a namespace exists without loading the full record.
    pub fn namespace_exists(&self, namespace_id: &str) -> Result<bool> {
        Ok(self.get_namespace(namespace_id)?.is_some())
    }

    /// Insert a new namespace
    pub fn insert_namespace(&self, namespace: &Namespace) -> Result<()> {
        let p = SystemTable::Namespaces.partition();
        let key = format!("ns:{}", namespace.namespace_id);
        let value = serde_json::to_vec(namespace)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a namespace by namespace_id
    pub fn delete_namespace(&self, namespace_id: &str) -> Result<()> {
        let p = SystemTable::Namespaces.partition();
        let key = format!("ns:{}", namespace_id);
        self.backend.delete(&p.into(), key.as_bytes())?;
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

        let p = StoragePartition::SystemColumns.partition();
        let value = serde_json::to_vec(&column_meta)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
        Ok(())
    }

    // Live query operations

    /// Get a live query by ID
    pub fn get_live_query(&self, live_id: &str) -> Result<Option<LiveQuery>> {
        let p = SystemTable::LiveQueries.partition();
        let key = format!("lq:{}", live_id);
        match self.backend.get(&p.into(), key.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Insert a new live query
    pub fn insert_live_query(&self, live_query: &LiveQuery) -> Result<()> {
        let p = SystemTable::LiveQueries.partition();
        let key = format!("lq:{}", live_query.live_id);
        let value = serde_json::to_vec(live_query)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a live query by ID
    pub fn delete_live_query(&self, live_id: &str) -> Result<()> {
        let p = SystemTable::LiveQueries.partition();
        let key = format!("lq:{}", live_id);
        self.backend.delete(&p.into(), key.as_bytes())?;
        Ok(())
    }

    // Job operations

    /// Get a job by ID
    pub fn get_job(&self, job_id: &str) -> Result<Option<Job>> {
        let p = SystemTable::Jobs.partition();
        let key = format!("job:{}", job_id);
        match self.backend.get(&p.into(), key.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Insert a new job
    pub fn insert_job(&self, job: &Job) -> Result<()> {
        let p = SystemTable::Jobs.partition();
        let key = format!("job:{}", job.job_id);
        let value = serde_json::to_vec(job)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a job by ID
    pub fn delete_job(&self, job_id: &str) -> Result<()> {
        let p = SystemTable::Jobs.partition();
        let key = format!("job:{}", job_id);
        self.backend.delete(&p.into(), key.as_bytes())?;
        Ok(())
    }

    // Table operations

    /// Get a table by ID
    pub fn get_table(&self, table_id: &str) -> Result<Option<Table>> {
        let p = SystemTable::Tables.partition();
        let key = format!("table:{}", table_id);
        match self.backend.get(&p.into(), key.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Insert a new table
    pub fn insert_table(&self, table: &Table) -> Result<()> {
        let p = SystemTable::Tables.partition();
        let key = format!("table:{}", table.table_id);
        let value = serde_json::to_vec(table)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
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
        let p = StoragePartition::InformationSchemaTables.partition();
        let key = format!("{}:{}", table_def.namespace_id, table_def.table_name);
        let value = serde_json::to_vec(table_def)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
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
        let p = StoragePartition::InformationSchemaTables.partition();
        let key = format!("{}:{}", namespace_id, table_name);
        match self.backend.get(&p.into(), key.as_bytes())? {
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
        let p = StoragePartition::InformationSchemaTables.partition();
        let prefix = format!("{}:", namespace_id);
        let mut tables = Vec::new();
        let iter = self
            .backend
            .scan(&p.into(), Some(prefix.as_bytes()), None)?;
        for (_k, v) in iter {
            tables.push(serde_json::from_slice(&v)?);
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
        let p = StoragePartition::InformationSchemaTables.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut tables = Vec::new();
        for (_k, v) in iter {
            tables.push(serde_json::from_slice(&v)?);
        }
        Ok(tables)
    }

    // Storage operations

    /// Get a storage by ID
    pub fn get_storage(&self, storage_id: &str) -> Result<Option<Storage>> {
        let p = SystemTable::Storages.partition();
        let key = format!("storage:{}", storage_id);
        match self.backend.get(&p.into(), key.as_bytes())? {
            Some(value) => Ok(Some(serde_json::from_slice(&value)?)),
            None => Ok(None),
        }
    }

    /// Insert a new storage
    pub fn insert_storage(&self, storage: &Storage) -> Result<()> {
        let p = SystemTable::Storages.partition();
        let key = format!("storage:{}", storage.storage_id);
        let value = serde_json::to_vec(storage)?;
        self.backend.put(&p.into(), key.as_bytes(), &value)?;
        Ok(())
    }

    /// Delete a storage by ID
    pub fn delete_storage(&self, storage_id: &str) -> Result<()> {
        let p = SystemTable::Storages.partition();
        let key = format!("storage:{}", storage_id);
        self.backend.delete(&p.into(), key.as_bytes())?;
        Ok(())
    }

    // Scan operations for all system tables

    /// Scan all users
    pub fn scan_all_users(&self) -> Result<Vec<User>> {
        let p = SystemTable::Users.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut users = Vec::new();
        for (_k, v) in iter {
            users.push(serde_json::from_slice(&v)?);
        }
        Ok(users)
    }

    /// Scan all namespaces
    pub fn scan_all_namespaces(&self) -> Result<Vec<Namespace>> {
        let p = SystemTable::Namespaces.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut namespaces = Vec::new();
        for (_k, v) in iter {
            namespaces.push(serde_json::from_slice(&v)?);
        }
        Ok(namespaces)
    }

    /// Scan all live queries
    pub fn scan_all_live_queries(&self) -> Result<Vec<LiveQuery>> {
        let p = SystemTable::LiveQueries.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut live_queries = Vec::new();
        for (_k, v) in iter {
            live_queries.push(serde_json::from_slice(&v)?);
        }
        Ok(live_queries)
    }

    /// Scan all jobs
    pub fn scan_all_jobs(&self) -> Result<Vec<Job>> {
        let p = SystemTable::Jobs.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut jobs = Vec::new();
        for (_k, v) in iter {
            jobs.push(serde_json::from_slice(&v)?);
        }
        Ok(jobs)
    }

    /// Scan all tables
    pub fn scan_all_tables(&self) -> Result<Vec<Table>> {
        let p = SystemTable::Tables.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut tables = Vec::new();
        for (_k, v) in iter {
            tables.push(serde_json::from_slice(&v)?);
        }
        Ok(tables)
    }

    /// Scan all storages
    pub fn scan_all_storages(&self) -> Result<Vec<Storage>> {
        let p = SystemTable::Storages.partition();
        let iter = self.backend.scan(&p.into(), None, None)?;
        let mut storages = Vec::new();
        for (_k, v) in iter {
            storages.push(serde_json::from_slice(&v)?);
        }
        Ok(storages)
    }

    // Additional CRUD operations for table deletion and updates

    /// Delete a table by table_id
    pub fn delete_table(&self, table_id: &str) -> Result<()> {
        let p = SystemTable::Tables.partition();
        let key = format!("table:{}", table_id);
        self.backend.delete(&p.into(), key.as_bytes())?;
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
