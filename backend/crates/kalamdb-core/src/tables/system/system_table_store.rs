//! System table store implementation using EntityStore pattern.
//!
//! Moved from stores/system_table.rs to tables/system/system_table_store.rs
//! to colocate system table logic under tables/. The original module path
//! `crate::stores::system_table::*` remains valid via re-exports.

use crate::error::KalamDbError;
use crate::tables::shared_tables::shared_table_store::{SharedTableRow, SharedTableRowId};
use crate::tables::stream_tables::stream_table_store::{StreamTableRow, StreamTableRowId};
use crate::tables::system::SystemTableProviderExt;
use crate::tables::user_tables::user_table_store::{UserTableRow, UserTableRowId};
use kalamdb_store::{
    entity_store::{CrossUserTableStore, EntityStore},
    StorageBackend,
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
    K: AsRef<[u8]> + Clone + Send + Sync,
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
    K: AsRef<[u8]> + Clone + Send + Sync,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    fn table_access(&self) -> Option<kalamdb_commons::models::TableAccess> { None }
}

/// Extension trait for user table stores with user-specific operations
pub trait UserTableStoreExt<K, V> {
    fn scan_user(&self, namespace_id: &str, table_name: &str, user_id: &str) -> std::result::Result<Vec<(String, V)>, KalamDbError>;
    fn get(&self, namespace_id: &str, table_name: &str, user_id: &str, row_id: &str) -> std::result::Result<Option<V>, KalamDbError>;
    fn put(&self, namespace_id: &str, table_name: &str, user_id: &str, row_id: &str, row: &V) -> std::result::Result<(), KalamDbError>;
    fn delete(&self, namespace_id: &str, table_name: &str, user_id: &str, row_id: &str, hard: bool) -> std::result::Result<(), KalamDbError>;
    fn get_include_deleted(&self, namespace_id: &str, table_name: &str, user_id: &str, row_id: &str) -> std::result::Result<Option<V>, KalamDbError>;
    fn scan_all(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Vec<(String, V)>, KalamDbError>;
    fn create_column_family(&self, namespace_id: &str, table_name: &str) -> std::result::Result<(), KalamDbError>;
    fn drop_table(&self, namespace_id: &str, table_name: &str) -> std::result::Result<(), KalamDbError>;
    fn delete_all_for_user(&self, namespace_id: &str, table_name: &str, user_id: &str) -> std::result::Result<usize, KalamDbError>;
    fn scan_iter(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError>;
    fn delete_batch_by_keys(&self, namespace_id: &str, table_name: &str, keys: &[String]) -> std::result::Result<(), KalamDbError>;
}

impl UserTableStoreExt<UserTableRowId, UserTableRow> for SystemTableStore<UserTableRowId, UserTableRow> {
    fn scan_user(&self, _namespace_id: &str, _table_name: &str, user_id: &str) -> std::result::Result<Vec<(String, UserTableRow)>, KalamDbError> {
        let prefix_key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), "");
        let results = EntityStore::scan_prefix(self, &prefix_key)?;
        Ok(results.into_iter().map(|(key, row)| (String::from_utf8_lossy(key.as_ref()).to_string(), row)).collect())
    }

    fn get(&self, _namespace_id: &str, _table_name: &str, user_id: &str, row_id: &str) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        let row = EntityStore::get(self, &key)?;
        Ok(row.filter(|r| !r._deleted))
    }

    fn put(&self, _namespace_id: &str, _table_name: &str, user_id: &str, row_id: &str, row: &UserTableRow) -> std::result::Result<(), KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        EntityStore::put(self, &key, row)?;
        Ok(())
    }

    fn delete(&self, _namespace_id: &str, _table_name: &str, user_id: &str, row_id: &str, hard: bool) -> std::result::Result<(), KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        if hard { EntityStore::delete(self, &key)?; } else {
            match EntityStore::get(self, &key)? {
                Some(mut row) => { row._deleted = true; EntityStore::put(self, &key, &row)?; }
                None => {}
            }
        }
        Ok(())
    }

    fn get_include_deleted(&self, _namespace_id: &str, _table_name: &str, user_id: &str, row_id: &str) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(user_id), row_id);
        Ok(EntityStore::get(self, &key)?)
    }

    fn scan_all(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Vec<(String, UserTableRow)>, KalamDbError> {
        let partition_name = format!("user_{}:{}", namespace_id, table_name);
        let partition = kalamdb_store::Partition::new(partition_name);
        let iter = self.backend.scan(&partition, None, None).map_err(|e| KalamDbError::Other(format!("Failed to scan user table partition: {}", e)))?;
        let mut results = Vec::new();
        for (key_bytes, value_bytes) in iter {
            let row: UserTableRow = serde_json::from_slice(&value_bytes).map_err(|e| KalamDbError::Other(format!("Failed to deserialize user table row: {}", e)))?;
            let key_str = String::from_utf8_lossy(&key_bytes).to_string();
            results.push((key_str, row));
        }
        Ok(results)
    }

    fn create_column_family(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<(), KalamDbError> {
        let partition = kalamdb_store::Partition::new(self.partition.clone());
        self.backend.create_partition(&partition).map_err(|e| KalamDbError::Other(format!("Failed to create partition: {}", e)))?;
        Ok(())
    }

    fn drop_table(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<(), KalamDbError> {
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results { let key = UserTableRowId::from_bytes(&key_bytes); EntityStore::delete(self, &key)?; }
        Ok(())
    }

    fn delete_all_for_user(&self, namespace_id: &str, table_name: &str, user_id: &str) -> std::result::Result<usize, KalamDbError> {
        let user_rows = self.scan_user(namespace_id, table_name, user_id)?;
        let mut deleted_count = 0; for (key_str, _) in user_rows { let key = UserTableRowId::from_key_string(key_str); EntityStore::delete(self, &key)?; deleted_count += 1; }
        Ok(deleted_count)
    }

    fn scan_iter(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| { let value_bytes = serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?; Ok((k, value_bytes)) });
        Ok(Box::new(iter))
    }

    fn delete_batch_by_keys(&self, _namespace_id: &str, _table_name: &str, keys: &[String]) -> std::result::Result<(), KalamDbError> {
        for key_str in keys { let key = UserTableRowId::from_key_string(key_str.clone()); EntityStore::delete(self, &key)?; }
        Ok(())
    }
}

/// Extension trait for shared table stores with shared table operations
pub trait SharedTableStoreExt<K, V> {
    fn scan(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Vec<(String, V)>, KalamDbError>;
    fn get(&self, namespace_id: &str, table_name: &str, row_id: &str) -> std::result::Result<Option<V>, KalamDbError>;
    fn put(&self, namespace_id: &str, table_name: &str, row_id: &str, row: &V) -> std::result::Result<(), KalamDbError>;
    fn delete(&self, namespace_id: &str, table_name: &str, row_id: &str, hard: bool) -> std::result::Result<(), KalamDbError>;
    fn get_include_deleted(&self, namespace_id: &str, table_name: &str, row_id: &str) -> std::result::Result<Option<V>, KalamDbError>;
    fn create_column_family(&self, namespace_id: &str, table_name: &str) -> std::result::Result<(), KalamDbError>;
    fn drop_table(&self, namespace_id: &str, table_name: &str) -> std::result::Result<(), KalamDbError>;
    fn scan_iter(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError>;
    fn delete_batch_by_keys(&self, namespace_id: &str, table_name: &str, keys: &[String]) -> std::result::Result<(), KalamDbError>;
}

impl SharedTableStoreExt<UserTableRowId, UserTableRow> for SystemTableStore<UserTableRowId, UserTableRow> {
    fn scan(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Vec<(String, UserTableRow)>, KalamDbError> {
        UserTableStoreExt::scan_all(self, namespace_id, table_name)
    }
    fn get(&self, namespace_id: &str, table_name: &str, row_id: &str) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        UserTableStoreExt::get_include_deleted(self, namespace_id, table_name, "", row_id)
    }
    fn put(&self, namespace_id: &str, _table_name: &str, row_id: &str, row: &UserTableRow) -> std::result::Result<(), KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(namespace_id), row_id);
        EntityStore::put(self, &key, row)?; Ok(())
    }
    fn delete(&self, namespace_id: &str, _table_name: &str, row_id: &str, hard: bool) -> std::result::Result<(), KalamDbError> {
        let key = UserTableRowId::new(kalamdb_commons::UserId::new(namespace_id), row_id);
        if hard { EntityStore::delete(self, &key)?; } else if let Some(_existing) = EntityStore::get(self, &key)? { EntityStore::delete(self, &key)?; }
        Ok(())
    }
    fn get_include_deleted(&self, namespace_id: &str, table_name: &str, row_id: &str) -> std::result::Result<Option<UserTableRow>, KalamDbError> {
        UserTableStoreExt::get_include_deleted(self, namespace_id, table_name, "", row_id)
    }
    fn create_column_family(&self, namespace_id: &str, table_name: &str) -> std::result::Result<(), KalamDbError> {
        UserTableStoreExt::create_column_family(self, namespace_id, table_name)
    }
    fn drop_table(&self, namespace_id: &str, table_name: &str) -> std::result::Result<(), KalamDbError> {
        UserTableStoreExt::drop_table(self, namespace_id, table_name)
    }
    fn scan_iter(&self, namespace_id: &str, table_name: &str) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        UserTableStoreExt::scan_iter(self, namespace_id, table_name)
    }
    fn delete_batch_by_keys(&self, namespace_id: &str, table_name: &str, keys: &[String]) -> std::result::Result<(), KalamDbError> {
        UserTableStoreExt::delete_batch_by_keys(self, namespace_id, table_name, keys)
    }
}

impl SharedTableStoreExt<SharedTableRowId, SharedTableRow> for SystemTableStore<SharedTableRowId, SharedTableRow> {
    fn scan(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<Vec<(String, SharedTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        Ok(results.into_iter().map(|(key, row)| (String::from_utf8_lossy(key.as_ref()).to_string(), row)).collect())
    }
    fn get(&self, _namespace_id: &str, _table_name: &str, row_id: &str) -> std::result::Result<Option<SharedTableRow>, KalamDbError> {
        let key = SharedTableRowId::new(row_id); Ok(EntityStore::get(self, &key)?)
    }
    fn put(&self, _namespace_id: &str, _table_name: &str, row_id: &str, row: &SharedTableRow) -> std::result::Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id); EntityStore::put(self, &key, row)?; Ok(())
    }
    fn delete(&self, _namespace_id: &str, _table_name: &str, row_id: &str, _hard: bool) -> std::result::Result<(), KalamDbError> {
        let key = SharedTableRowId::new(row_id); EntityStore::delete(self, &key)?; Ok(())
    }
    fn get_include_deleted(&self, _namespace_id: &str, _table_name: &str, row_id: &str) -> std::result::Result<Option<SharedTableRow>, KalamDbError> {
        let key = SharedTableRowId::new(row_id); Ok(EntityStore::get(self, &key)?)
    }
    fn create_column_family(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<(), KalamDbError> {
        let partition = kalamdb_store::Partition::new(self.partition.clone());
        self.backend.create_partition(&partition).map_err(|e| KalamDbError::Other(format!("Failed to create partition: {}", e)))?; Ok(())
    }
    fn drop_table(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<(), KalamDbError> {
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results { let key = SharedTableRowId::from_bytes(&key_bytes); EntityStore::delete(self, &key)?; }
        Ok(())
    }
    fn scan_iter(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| { let value_bytes = serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?; Ok((k, value_bytes)) });
        Ok(Box::new(iter))
    }
    fn delete_batch_by_keys(&self, _namespace_id: &str, _table_name: &str, keys: &[String]) -> std::result::Result<(), KalamDbError> {
        for key_str in keys { let key = SharedTableRowId::new(key_str); EntityStore::delete(self, &key)?; } Ok(())
    }
}

impl SharedTableStoreExt<StreamTableRowId, StreamTableRow> for SystemTableStore<StreamTableRowId, StreamTableRow> {
    fn scan(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<Vec<(String, StreamTableRow)>, KalamDbError> {
        let results = EntityStore::scan_all(self)?;
        Ok(results.into_iter().map(|(key, row)| (String::from_utf8_lossy(key.as_ref()).to_string(), row)).collect())
    }
    fn get(&self, _namespace_id: &str, _table_name: &str, row_id: &str) -> std::result::Result<Option<StreamTableRow>, KalamDbError> {
        let key = StreamTableRowId::new(row_id); Ok(EntityStore::get(self, &key)?)
    }
    fn put(&self, _namespace_id: &str, _table_name: &str, row_id: &str, row: &StreamTableRow) -> std::result::Result<(), KalamDbError> {
        let key = StreamTableRowId::new(row_id); EntityStore::put(self, &key, row)?; Ok(())
    }
    fn delete(&self, _namespace_id: &str, _table_name: &str, row_id: &str, _hard: bool) -> std::result::Result<(), KalamDbError> {
        let key = StreamTableRowId::new(row_id); EntityStore::delete(self, &key)?; Ok(())
    }
    fn get_include_deleted(&self, _namespace_id: &str, _table_name: &str, row_id: &str) -> std::result::Result<Option<StreamTableRow>, KalamDbError> {
        let key = StreamTableRowId::new(row_id); Ok(EntityStore::get(self, &key)?)
    }
    fn create_column_family(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<(), KalamDbError> {
        let partition = kalamdb_store::Partition::new(self.partition.clone());
        self.backend.create_partition(&partition).map_err(|e| KalamDbError::Other(format!("Failed to create partition: {}", e)))?; Ok(())
    }
    fn drop_table(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<(), KalamDbError> {
        let all_results = EntityStore::scan_all(self)?;
        for (key_bytes, _) in all_results { let key = StreamTableRowId::from_bytes(&key_bytes); EntityStore::delete(self, &key)?; }
        Ok(())
    }
    fn scan_iter(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<Box<dyn Iterator<Item = std::result::Result<(Vec<u8>, Vec<u8>), KalamDbError>> + Send>, KalamDbError> {
        let all = EntityStore::scan_all(self)?;
        let iter = all.into_iter().map(|(k, v)| { let value_bytes = serde_json::to_vec(&v).map_err(|e| KalamDbError::SerializationError(e.to_string()))?; Ok((k, value_bytes)) });
        Ok(Box::new(iter))
    }
    fn delete_batch_by_keys(&self, _namespace_id: &str, _table_name: &str, keys: &[String]) -> std::result::Result<(), KalamDbError> {
        for key_str in keys { let key = StreamTableRowId::new(key_str); EntityStore::delete(self, &key)?; } Ok(())
    }
}

impl SystemTableStore<StreamTableRowId, StreamTableRow> {
    pub fn cleanup_expired_rows(&self, _namespace_id: &str, _table_name: &str) -> std::result::Result<usize, KalamDbError> {
        let all_rows = EntityStore::scan_all(self)?; let mut deleted_count = 0;
        for (key_bytes, row) in all_rows {
            if let Some(ttl) = row.ttl_seconds {
                let inserted_at = chrono::DateTime::parse_from_rfc3339(&row.inserted_at).map_err(|e| KalamDbError::Other(e.to_string()))?;
                if chrono::Utc::now().timestamp() >= inserted_at.timestamp() + ttl as i64 {
                    let key = StreamTableRowId::from_bytes(&key_bytes); EntityStore::delete(self, &key)?; deleted_count += 1;
                }
            }
        }
        Ok(deleted_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_store::test_utils::InMemoryBackend;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct TestEntity { id: String, name: String, value: i32 }

    #[test]
    fn test_system_table_store_put_get() {
        let backend = Arc::new(InMemoryBackend::new());
        let store = SystemTableStore::<String, TestEntity>::new(backend, "test_system");
        let entity = TestEntity { id: "e1".into(), name: "Test Entity".into(), value: 42 };
        let key_e1 = "e1".to_string();
        store.put(&key_e1, &entity).unwrap();
        let retrieved = store.get(&key_e1).unwrap().unwrap();
        assert_eq!(retrieved, entity);
        let key_e999 = "e999".to_string();
        let missing = store.get(&key_e999).unwrap();
        assert!(missing.is_none());
    }
}
