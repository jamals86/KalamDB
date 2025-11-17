//! Extension traits for table stores
//!
//! Provides additional methods for user, shared, and stream table stores
//! that extend the basic EntityStore functionality.

use crate::error::TableError;
use crate::{
    SharedTableRow, SharedTableStore, StreamTableRow, StreamTableStore, UserTableRow,
    UserTableStore,
};
use kalamdb_commons::ids::{SharedTableRowId, StreamTableRowId, UserTableRowId};
use kalamdb_store::entity_store::EntityStore;

/// Extension trait for user table stores
pub trait UserTableStoreExt {
    fn scan_user(
        &self,
        user_key_prefix: &UserTableRowId,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, TableError>;
    fn delete_all_for_user(&self, user_key_prefix: &UserTableRowId) -> Result<usize, TableError>;
}

impl UserTableStoreExt for UserTableStore {
    fn scan_user(
        &self,
        user_key_prefix: &UserTableRowId,
    ) -> Result<Vec<(UserTableRowId, UserTableRow)>, TableError> {
        let raw_results = EntityStore::scan_prefix(self, user_key_prefix)
            .map_err(|e| TableError::Storage(e.to_string()))?;

        // Deserialize Vec<u8> keys back to UserTableRowId
        raw_results
            .into_iter()
            .map(|(key_bytes, value)| {
                let key = UserTableRowId::from_bytes(&key_bytes).map_err(|e| {
                    TableError::Serialization(format!("Failed to deserialize key: {}", e))
                })?;
                Ok((key, value))
            })
            .collect()
    }

    fn delete_all_for_user(&self, user_key_prefix: &UserTableRowId) -> Result<usize, TableError> {
        let rows = self.scan_user(user_key_prefix)?;
        let count = rows.len();
        for (key, _) in rows {
            EntityStore::delete(self, &key).map_err(|e| TableError::Storage(e.to_string()))?;
        }
        Ok(count)
    }
}

/// Extension trait for shared table stores
pub trait SharedTableStoreExt {
    fn scan_all_shared(&self) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, TableError>;
}

impl SharedTableStoreExt for SharedTableStore {
    fn scan_all_shared(&self) -> Result<Vec<(SharedTableRowId, SharedTableRow)>, TableError> {
        let raw_results =
            EntityStore::scan_all(self).map_err(|e| TableError::Storage(e.to_string()))?;

        // Deserialize Vec<u8> keys back to SharedTableRowId (SeqId)
        raw_results
            .into_iter()
            .map(|(key_bytes, value)| {
                let key = SharedTableRowId::from_bytes(&key_bytes).map_err(|e| {
                    TableError::Serialization(format!("Failed to deserialize key: {}", e))
                })?;
                Ok((key, value))
            })
            .collect()
    }
}

/// Extension trait for stream table stores
pub trait StreamTableStoreExt {
    fn scan_all_stream(&self) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, TableError>;
}

impl StreamTableStoreExt for StreamTableStore {
    fn scan_all_stream(&self) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, TableError> {
        let raw_results =
            EntityStore::scan_all(self).map_err(|e| TableError::Storage(e.to_string()))?;

        // Deserialize Vec<u8> keys back to StreamTableRowId
        raw_results
            .into_iter()
            .map(|(key_bytes, value)| {
                let key = StreamTableRowId::from_bytes(&key_bytes).map_err(|e| {
                    TableError::Serialization(format!("Failed to deserialize key: {}", e))
                })?;
                Ok((key, value))
            })
            .collect()
    }
}
