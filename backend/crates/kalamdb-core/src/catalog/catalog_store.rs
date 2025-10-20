//! Catalog store using RocksDB
//!
//! **DEPRECATED**: This module is deprecated as of v0.2.0. Use kalamdb-sql for system tables
//! and kalamdb-store for user tables instead of direct RocksDB access.
//!
//! **Migration Guide**:
//! - System tables → Use kalamdb-sql::KalamSql with SQL queries
//! - User tables → Use kalamdb-store::UserTableStore for DML operations
//!
//! This module provides storage for system table data using dedicated column families.
//!
//! **NOTE**: This is a transitional layer. The updated column family naming (`system_{name}`)
//! matches the Phase 1.5 architecture changes. Future refactoring (T028a) will replace direct
//! RocksDB operations with kalamdb-sql SQL interface for consistency.

use crate::catalog::UserId;
use crate::error::KalamDbError;
use rocksdb::{ColumnFamily, DB};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// System table catalog store backed by RocksDB
///
/// **DEPRECATED**: Use kalamdb-sql for system tables and kalamdb-store for user tables.
/// This struct will be removed in a future version.
///
/// **Architecture Update**: Uses new column family naming `system_{name}` (not `system_table:{name}`)
#[deprecated(
    since = "0.2.0",
    note = "Use kalamdb-sql for system tables and kalamdb-store for user tables instead of CatalogStore"
)]
pub struct CatalogStore {
    db: Arc<DB>,
}

#[allow(deprecated)]
impl CatalogStore {
    /// Create a new catalog store
    pub fn new(db: Arc<DB>) -> Self {
        Self { db }
    }

    /// Get column family handle for a system table
    ///
    /// **Updated**: Uses new CF naming convention `system_{name}` per Phase 1.5 changes
    fn get_cf_handle(&self, table_name: &str) -> Result<&ColumnFamily, KalamDbError> {
        let cf_name = format!("system_{}", table_name);
        self.db.cf_handle(&cf_name).ok_or_else(|| {
            KalamDbError::CatalogError(format!("Column family not found: {}", cf_name))
        })
    }

    /// Insert or update data in a system table
    pub fn put(&self, table_name: &str, key: &[u8], value: &[u8]) -> Result<(), KalamDbError> {
        let cf = self.get_cf_handle(table_name)?;
        self.db.put_cf(cf, key, value).map_err(|e| {
            KalamDbError::Storage(crate::error::StorageError::Other(format!(
                "Failed to put data in {}: {}",
                table_name, e
            )))
        })
    }

    /// Get data from a system table
    pub fn get(&self, table_name: &str, key: &[u8]) -> Result<Option<Vec<u8>>, KalamDbError> {
        let cf = self.get_cf_handle(table_name)?;
        self.db.get_cf(cf, key).map_err(|e| {
            KalamDbError::Storage(crate::error::StorageError::Other(format!(
                "Failed to get data from {}: {}",
                table_name, e
            )))
        })
    }

    /// Delete data from a system table
    pub fn delete(&self, table_name: &str, key: &[u8]) -> Result<(), KalamDbError> {
        let cf = self.get_cf_handle(table_name)?;
        self.db.delete_cf(cf, key).map_err(|e| {
            KalamDbError::Storage(crate::error::StorageError::Other(format!(
                "Failed to delete data from {}: {}",
                table_name, e
            )))
        })
    }

    /// Iterate over all entries in a system table
    pub fn iter<'a>(
        &'a self,
        table_name: &str,
    ) -> Result<Box<dyn Iterator<Item = (Box<[u8]>, Box<[u8]>)> + 'a>, KalamDbError> {
        let cf = self.get_cf_handle(table_name)?;
        let iter = self
            .db
            .iterator_cf(cf, rocksdb::IteratorMode::Start)
            .filter_map(|result| result.ok());
        Ok(Box::new(iter))
    }

    /// Insert or update a user record
    pub fn put_user<T: Serialize>(
        &self,
        user_id: &UserId,
        user_data: &T,
    ) -> Result<(), KalamDbError> {
        let key = user_id.as_ref().as_bytes();
        let value = serde_json::to_vec(user_data).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize user data: {}", e))
        })?;
        self.put("users", key, &value)
    }

    /// Get a user record
    pub fn get_user<T: for<'de> Deserialize<'de>>(
        &self,
        user_id: &UserId,
    ) -> Result<Option<T>, KalamDbError> {
        let key = user_id.as_ref().as_bytes();
        let value = self.get("users", key)?;
        match value {
            Some(bytes) => {
                let user = serde_json::from_slice(&bytes).map_err(|e| {
                    KalamDbError::SerializationError(format!(
                        "Failed to deserialize user data: {}",
                        e
                    ))
                })?;
                Ok(Some(user))
            }
            None => Ok(None),
        }
    }

    /// Delete a user record
    pub fn delete_user(&self, user_id: &UserId) -> Result<(), KalamDbError> {
        let key = user_id.as_ref().as_bytes();
        self.delete("users", key)
    }

    /// Insert or update a live query record
    pub fn put_live_query<T: Serialize>(
        &self,
        live_id: &str,
        live_query_data: &T,
    ) -> Result<(), KalamDbError> {
        let key = live_id.as_bytes();
        let value = serde_json::to_vec(live_query_data).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize live query data: {}", e))
        })?;
        self.put("live_queries", key, &value)
    }

    /// Get a live query record
    pub fn get_live_query<T: for<'de> Deserialize<'de>>(
        &self,
        live_id: &str,
    ) -> Result<Option<T>, KalamDbError> {
        let key = live_id.as_bytes();
        let value = self.get("live_queries", key)?;
        match value {
            Some(bytes) => {
                let live_query = serde_json::from_slice(&bytes).map_err(|e| {
                    KalamDbError::SerializationError(format!(
                        "Failed to deserialize live query data: {}",
                        e
                    ))
                })?;
                Ok(Some(live_query))
            }
            None => Ok(None),
        }
    }

    /// Delete a live query record
    pub fn delete_live_query(&self, live_id: &str) -> Result<(), KalamDbError> {
        let key = live_id.as_bytes();
        self.delete("live_queries", key)
    }

    /// Insert or update a storage location record
    pub fn put_storage_location<T: Serialize>(
        &self,
        location_name: &str,
        location_data: &T,
    ) -> Result<(), KalamDbError> {
        let key = format!("location:{}", location_name);
        let value = serde_json::to_vec(location_data).map_err(|e| {
            KalamDbError::SerializationError(format!(
                "Failed to serialize storage location data: {}",
                e
            ))
        })?;
        self.put("storage_locations", key.as_bytes(), &value)
    }

    /// Get a storage location record
    pub fn get_storage_location<T: for<'de> Deserialize<'de>>(
        &self,
        location_name: &str,
    ) -> Result<Option<T>, KalamDbError> {
        let key = format!("location:{}", location_name);
        let value = self.get("storage_locations", key.as_bytes())?;
        match value {
            Some(bytes) => {
                let location = serde_json::from_slice(&bytes).map_err(|e| {
                    KalamDbError::SerializationError(format!(
                        "Failed to deserialize storage location data: {}",
                        e
                    ))
                })?;
                Ok(Some(location))
            }
            None => Ok(None),
        }
    }

    /// Delete a storage location record
    pub fn delete_storage_location(&self, location_name: &str) -> Result<(), KalamDbError> {
        let key = format!("location:{}", location_name);
        self.delete("storage_locations", key.as_bytes())
    }

    /// Insert or update a job record
    pub fn put_job<T: Serialize>(&self, job_id: &str, job_data: &T) -> Result<(), KalamDbError> {
        let key = format!("job:{}", job_id);
        let value = serde_json::to_vec(job_data).map_err(|e| {
            KalamDbError::SerializationError(format!("Failed to serialize job data: {}", e))
        })?;
        self.put("jobs", key.as_bytes(), &value)
    }

    /// Get a job record
    pub fn get_job<T: for<'de> Deserialize<'de>>(
        &self,
        job_id: &str,
    ) -> Result<Option<T>, KalamDbError> {
        let key = format!("job:{}", job_id);
        let value = self.get("jobs", key.as_bytes())?;
        match value {
            Some(bytes) => {
                let job = serde_json::from_slice(&bytes).map_err(|e| {
                    KalamDbError::SerializationError(format!(
                        "Failed to deserialize job data: {}",
                        e
                    ))
                })?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Delete a job record
    pub fn delete_job(&self, job_id: &str) -> Result<(), KalamDbError> {
        let key = format!("job:{}", job_id);
        self.delete("jobs", key.as_bytes())
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::storage::RocksDbInit;
    use serde::{Deserialize, Serialize};
    use std::env;
    use std::fs;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestUser {
        username: String,
        email: String,
    }

    #[test]
    fn test_catalog_store_user_operations() {
        let temp_dir = env::temp_dir().join("kalamdb_catalog_store_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());
        let db = init.open().unwrap();
        let store = CatalogStore::new(Arc::clone(&db));

        let user_id = UserId::new("user1");
        let user = TestUser {
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
        };

        // Test put
        store.put_user(&user_id, &user).unwrap();

        // Test get
        let retrieved: Option<TestUser> = store.get_user(&user_id).unwrap();
        assert_eq!(retrieved, Some(user.clone()));

        // Test delete
        store.delete_user(&user_id).unwrap();
        let retrieved: Option<TestUser> = store.get_user(&user_id).unwrap();
        assert_eq!(retrieved, None);

        RocksDbInit::close(db);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_catalog_store_live_query_operations() {
        let temp_dir = env::temp_dir().join("kalamdb_catalog_live_query_test");
        let _ = fs::remove_dir_all(&temp_dir);

        let init = RocksDbInit::new(temp_dir.to_str().unwrap());
        let db = init.open().unwrap();
        let store = CatalogStore::new(Arc::clone(&db));

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        struct TestLiveQuery {
            query: String,
            table_name: String,
        }

        let live_id = "user1-conn1-table1-q1";
        let live_query = TestLiveQuery {
            query: "SELECT * FROM table1".to_string(),
            table_name: "table1".to_string(),
        };

        // Test put
        store.put_live_query(live_id, &live_query).unwrap();

        // Test get
        let retrieved: Option<TestLiveQuery> = store.get_live_query(live_id).unwrap();
        assert_eq!(retrieved, Some(live_query.clone()));

        // Test delete
        store.delete_live_query(live_id).unwrap();
        let retrieved: Option<TestLiveQuery> = store.get_live_query(live_id).unwrap();
        assert_eq!(retrieved, None);

        RocksDbInit::close(db);
        let _ = fs::remove_dir_all(temp_dir);
    }
}
