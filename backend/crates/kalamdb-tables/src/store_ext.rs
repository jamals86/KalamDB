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
use kalamdb_store::EntityStore;

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
        self.scan_prefix_typed(user_key_prefix, None)
            .map_err(|e| TableError::Storage(e.to_string()))
    }

    fn delete_all_for_user(&self, user_key_prefix: &UserTableRowId) -> Result<usize, TableError> {
        let rows = self.scan_user(user_key_prefix)?;
        let count = rows.len();
        for (key, _) in rows {
            self.delete(&key).map_err(|e| TableError::Storage(e.to_string()))?;
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
        self.scan_all_typed(None, None, None)
            .map_err(|e| TableError::Storage(e.to_string()))
    }
}

/// Extension trait for stream table stores
pub trait StreamTableStoreExt {
    fn scan_all_stream(&self) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, TableError>;
}

impl StreamTableStoreExt for StreamTableStore {
    fn scan_all_stream(&self) -> Result<Vec<(StreamTableRowId, StreamTableRow)>, TableError> {
        self.scan_all(None).map_err(|e| TableError::Storage(e.to_string()))
    }
}
