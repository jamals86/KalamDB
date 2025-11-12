//! System table store implementation using EntityStore pattern.
//!
//! Moved from stores/system_table.rs to tables/system/system_table_store.rs
//! to colocate system table logic under tables/. The original module path
//! `crate::stores::system_table::*` remains valid via re-exports.

use crate::error::KalamDbError;
use crate::tables::shared_tables::shared_table_store::SharedTableRow;
use crate::tables::stream_tables::stream_table_store::StreamTableRow;
use crate::tables::system::SystemTableProviderExt;
use crate::tables::user_tables::user_table_store::UserTableRow;
use kalamdb_commons::ids::{SharedTableRowId, UserTableRowId, StreamTableRowId};
use kalamdb_store::{
    entity_store::{CrossUserTableStore, EntityStore},
    StorageBackend, StorageKey,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Generic store for system tables with admin-only access control.
pub struct SystemTableStore<K, V> {
    backend: Arc<dyn StorageBackend>,
    partition: String,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> SystemTableStore<K, V> {
    pub fn new(backend: Arc<dyn StorageBackend>, partition: impl Into<String>) -> Self {
        Self { backend, partition: partition.into(), _phantom: std::marker::PhantomData }
    }
}

impl<K, V> EntityStore<K, V> for SystemTableStore<K, V>
where
    K: StorageKey,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn backend(&self) -> &Arc<dyn StorageBackend> { &self.backend }
    fn partition(&self) -> &str { &self.partition }
}

impl<K: Send + Sync, V: Send + Sync> SystemTableProviderExt for SystemTableStore<K, V> {
    fn table_name(&self) -> &str {
        self.partition.strip_prefix("system_").unwrap_or(&self.partition)
    }

    fn schema_ref(&self) -> arrow::datatypes::SchemaRef {
        Arc::new(arrow::datatypes::Schema::empty())
    }

    fn load_batch(&self) -> std::result::Result<arrow::record_batch::RecordBatch, KalamDbError> {
        Err(KalamDbError::InvalidOperation(
            "System tables do not support RecordBatch loading".to_string(),
        ))
    }
}

impl<K, V> CrossUserTableStore<K, V> for SystemTableStore<K, V>
where
    K: StorageKey,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn table_access(&self) -> Option<kalamdb_commons::models::TableAccess> { None }
}

/// Extension trait for user table stores with user-specific operations
pub trait UserTableStoreExt<K, V> {
    fn scan_user(&self, user_key_prefix: &K) -> std::result::Result<Vec<(K, V)>, KalamDbError>;
    fn get(&self, key: &K) -> std::result::Result<Option<V>, KalamDbError>;
    fn put(&self, key: &K, row: &V) -> std::result::Result<(), KalamDbError>;
    fn delete(&self, key: &K, hard: bool) -> std::result::Result<(), KalamDbError>;
    fn get_include_deleted(&self, key: &K) -> std::result::Result<Option<V>, KalamDbError>;
    fn scan_all(&self) -> std::result::Result<Vec<(K, V)>, KalamDbError>;
    fn create_column_family(&self) -> std::result::Result<(), KalamDbError>;
    fn drop_table(&self) -> std::result::Result<(), KalamDbError>;
    fn delete_all_for_user(&self, user_key_prefix: &K) -> std::result::Result<usize, KalamDbError>;
    fn scan_iter(&self) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError>;
    fn delete_batch_by_keys(&self, keys: &[K]) -> std::result::Result<(), KalamDbError>;
}

impl UserTableStoreExt<UserTableRowId, UserTableRow> for SystemTableStore<UserTableRowId, UserTableRow> {
    fn scan_user(&self, user_key_prefix: &UserTableRowId) -> std::result::Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        let results = EntityStore::scan_prefix(self, user_key_prefix)?;
        let mut typed_results = Vec::new();
        for (key_bytes, row) in results {
            let key = UserTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            typed_results.push((key, row));
        }
        Ok(typed_results)
    }

    fn get(&self, key: &UserTableRowId) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        let row = EntityStore::get(self, key)?;
        Ok(row.filter(|r| !r._deleted))
    }

    fn put(&self, key: &UserTableRowId, row: &UserTableRow) -> std::result::Result<(), KalamDbError> {
        EntityStore::put(self, key, row)?;
        Ok(())
    }

    fn delete(&self, key: &UserTableRowId, hard: bool) -> std::result::Result<(), KalamDbError> {
        if hard { 
            EntityStore::delete(self, key)?; 
        } else {
            match EntityStore::get(self, key)? {
                Some(mut row) => { 
                    row._deleted = true; 
                    EntityStore::put(self, key, &row)?; 
                }
                None => {}
            }
        }
        Ok(())
    }

    fn get_include_deleted(&self, key: &UserTableRowId) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        Ok(EntityStore::get(self, key)?)
    }

    fn scan_all(&self) -> std::result::Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        let mut typed_results = Vec::new();
        for (key_bytes, row) in results {
            let key = UserTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            typed_results.push((key, row));
        }
        Ok(typed_results)
    }

    fn create_column_family(&self) -> std::result::Result<(), KalamDbError> {
        let partition = kalamdb_store::Partition::new(self.partition.clone());
        self.backend.create_partition(&partition).map_err(|e| KalamDbError::Other(format!("Failed to create partition: {}", e)))?;
        Ok(())
    }

    fn drop_table(&self) -> std::result::Result<(), KalamDbError> {
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results { 
            let key = UserTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            EntityStore::delete(self, &key)?;
        }
        Ok(())
    }

    fn delete_all_for_user(&self, user_key_prefix: &UserTableRowId) -> std::result::Result<usize, KalamDbError> {
        let user_rows = self.scan_user(user_key_prefix)?;
        let mut deleted_count = 0; 
        for (key, _) in user_rows { 
            EntityStore::delete(self, &key)?;
            deleted_count += 1; 
        }
        Ok(deleted_count)
    }

    fn scan_iter(&self) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| { 
            let value_bytes = serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?; 
            Ok((k, value_bytes)) 
        });
        Ok(Box::new(iter))
    }

    fn delete_batch_by_keys(&self, keys: &[UserTableRowId]) -> std::result::Result<(), KalamDbError> {
        for key in keys { 
            EntityStore::delete(self, key)?; 
        }
        Ok(())
    }
}

/// Extension trait for shared table stores with shared table operations
pub trait SharedTableStoreExt<K, V> {
    fn scan(&self) -> std::result::Result<Vec<(K, V)>, KalamDbError>;
    fn get(&self, key: &K) -> std::result::Result<Option<V>, KalamDbError>;
    fn put(&self, key: &K, row: &V) -> std::result::Result<(), KalamDbError>;
    fn delete(&self, key: &K, hard: bool) -> std::result::Result<(), KalamDbError>;
    fn get_include_deleted(&self, key: &K) -> std::result::Result<Option<V>, KalamDbError>;
    fn create_column_family(&self) -> std::result::Result<(), KalamDbError>;
    fn drop_table(&self) -> std::result::Result<(), KalamDbError>;
    fn scan_iter(&self) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError>;
    fn delete_batch_by_keys(&self, keys: &[K]) -> std::result::Result<(), KalamDbError>;
}

impl SharedTableStoreExt<UserTableRowId, UserTableRow> for SystemTableStore<UserTableRowId, UserTableRow> {
    fn scan(&self) -> std::result::Result<Vec<(UserTableRowId, UserTableRow)>, KalamDbError> {
        UserTableStoreExt::scan_all(self)
    }
    fn get(&self, key: &UserTableRowId) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        UserTableStoreExt::get_include_deleted(self, key)
    }
    fn put(&self, key: &UserTableRowId, row: &UserTableRow) -> std::result::Result<(), KalamDbError> {
        EntityStore::put(self, key, row)?; 
        Ok(())
    }
    fn delete(&self, key: &UserTableRowId, hard: bool) -> std::result::Result<(), KalamDbError> {
        if hard { 
            EntityStore::delete(self, key)?; 
        } else if let Some(_existing) = EntityStore::get(self, key)? { 
            EntityStore::delete(self, key)?; 
        }
        Ok(())
    }
    fn get_include_deleted(&self, key: &UserTableRowId) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        UserTableStoreExt::get_include_deleted(self, key)
    }
    fn create_column_family(&self) -> std::result::Result<(), KalamDbError> {
        UserTableStoreExt::create_column_family(self)
    }
    fn drop_table(&self) -> std::result::Result<(), KalamDbError> {
        UserTableStoreExt::drop_table(self)
    }
    fn scan_iter(&self) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        UserTableStoreExt::scan_iter(self)
    }
    fn delete_batch_by_keys(&self, keys: &[UserTableRowId]) -> std::result::Result<(), KalamDbError> {
        UserTableStoreExt::delete_batch_by_keys(self, keys)
    }
}

impl SharedTableStoreExt<SharedTableRowId, SharedTableRow> for SystemTableStore<SharedTableRowId, SharedTableRow> {
    fn scan(&self) -> std::result::Result<Vec<(SharedTableRowId, SharedTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        let mut typed_results = Vec::new();
        for (key_bytes, row) in results {
            let key = SharedTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            typed_results.push((key, row));
        }
        Ok(typed_results)
    }
    fn get(&self, key: &SharedTableRowId) -> std::result::Result<Option<SharedTableRow>, KalamDbError> {
        Ok(EntityStore::get(self, key)?)
    }
    fn put(&self, key: &SharedTableRowId, row: &SharedTableRow) -> std::result::Result<(), KalamDbError> {
        EntityStore::put(self, key, row)?; 
        Ok(())
    }
    fn delete(&self, key: &SharedTableRowId, _hard: bool) -> std::result::Result<(), KalamDbError> {
        EntityStore::delete(self, key)?; 
        Ok(())
    }
    fn get_include_deleted(&self, key: &SharedTableRowId) -> std::result::Result<Option<SharedTableRow>, KalamDbError> {
        Ok(EntityStore::get(self, key)?)
    }
    fn create_column_family(&self) -> std::result::Result<(), KalamDbError> {
        let partition = kalamdb_store::Partition::new(self.partition.clone());
        self.backend.create_partition(&partition).map_err(|e| KalamDbError::Other(format!("Failed to create partition: {}", e)))?; 
        Ok(())
    }
    fn drop_table(&self) -> std::result::Result<(), KalamDbError> {
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results { 
            let key = SharedTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            EntityStore::delete(self, &key)?; 
        }
        Ok(())
    }
    fn scan_iter(&self) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| { 
            let value_bytes = serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?; 
            Ok((k, value_bytes)) 
        });
        Ok(Box::new(iter))
    }
    fn delete_batch_by_keys(&self, keys: &[SharedTableRowId]) -> std::result::Result<(), KalamDbError> {
        for key in keys { 
            EntityStore::delete(self, key)?; 
        } 
        Ok(())
    }
}

impl SharedTableStoreExt<StreamTableRowId, StreamTableRow> for SystemTableStore<StreamTableRowId, StreamTableRow> {
    fn scan(&self) -> std::result::Result<Vec<(StreamTableRowId, StreamTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        let mut typed_results = Vec::new();
        for (key_bytes, row) in results {
            let key = StreamTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?;
            typed_results.push((key, row));
        }
        Ok(typed_results)
    }
    fn get(&self, key: &StreamTableRowId) -> std::result::Result<Option<StreamTableRow>, KalamDbError> {
        Ok(EntityStore::get(self, key)?)
    }
    fn put(&self, key: &StreamTableRowId, row: &StreamTableRow) -> std::result::Result<(), KalamDbError> {
        EntityStore::put(self, key, row)?; 
        Ok(())
    }
    fn delete(&self, key: &StreamTableRowId, _hard: bool) -> std::result::Result<(), KalamDbError> {
        EntityStore::delete(self, key)?; 
        Ok(())
    }
    fn get_include_deleted(&self, key: &StreamTableRowId) -> std::result::Result<Option<StreamTableRow>, KalamDbError> {
        Ok(EntityStore::get(self, key)?)
    }
    fn create_column_family(&self) -> std::result::Result<(), KalamDbError> {
        let partition = kalamdb_store::Partition::new(self.partition.clone());
        self.backend.create_partition(&partition).map_err(|e| KalamDbError::Other(format!("Failed to create partition: {}", e)))?; 
        Ok(())
    }
    fn drop_table(&self) -> std::result::Result<(), KalamDbError> {
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results { 
            let key = StreamTableRowId::from_bytes(&key_bytes)
                .map_err(|e| KalamDbError::InvalidOperation(format!("Invalid key bytes: {}", e)))?; 
            EntityStore::delete(self, &key)?; 
        }
        Ok(())
    }
    fn scan_iter(&self) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| { 
            let value_bytes = serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?; 
            Ok((k, value_bytes)) 
        });
        Ok(Box::new(iter))
    }
    fn delete_batch_by_keys(&self, keys: &[StreamTableRowId]) -> std::result::Result<(), KalamDbError> {
        for key in keys { 
            EntityStore::delete(self, key)?; 
        } 
        Ok(())
    }
}

impl SystemTableStore<StreamTableRowId, StreamTableRow> {
    pub fn cleanup_expired_rows(&self) -> std::result::Result<usize, KalamDbError> {
        // StreamTableRow no longer contains inserted_at/ttl_seconds fields.
        // TTL-based eviction is handled at the provider/executor level.
        // No-op here to maintain compatibility.
        Ok(0)
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use kalamdb_store::test_utils::InMemoryBackend;
//     use serde::{Deserialize, Serialize};

//     #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
//     struct TestEntity { id: String, name: String, value: i32 }

//     #[test]
//     fn test_system_table_store_put_get() {
//         let backend = Arc::new(InMemoryBackend::new());
//         let store = SystemTableStore::<String, TestEntity>::new(backend, "test_system");
//         let entity = TestEntity { id: "e1".into(), name: "Test Entity".into(), value: 42 };
//         let key_e1 = "e1".to_string();
//         store.put(&key_e1, &entity).unwrap();
//         let retrieved = store.get(&key_e1).unwrap().unwrap();
//         assert_eq!(retrieved, entity);
//         let key_e999 = "e999".to_string();
//         let missing = store.get(&key_e999).unwrap();
//         assert!(missing.is_none());
//     }
// }
