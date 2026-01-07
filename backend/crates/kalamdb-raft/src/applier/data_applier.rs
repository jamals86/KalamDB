//! Data Applier traits for persisting user and shared table data
//!
//! These traits are called by UserDataStateMachine and SharedDataStateMachine
//! after Raft consensus commits a command. All nodes (leader and followers)
//! call this, ensuring that all replicas persist the same data.
//!
//! The implementation lives in kalamdb-core using provider infrastructure.

use async_trait::async_trait;
use kalamdb_commons::models::UserId;
use kalamdb_commons::TableId;

use crate::RaftError;

/// Applier callback for user table data operations
///
/// This trait is called by UserDataStateMachine after Raft consensus commits
/// a data command. All nodes (leader and followers) call this, ensuring that
/// all replicas persist the same data to their local storage.
///
/// # Implementation
///
/// The implementation lives in kalamdb-core and uses table providers
/// to persist data to RocksDB.
#[async_trait]
pub trait UserDataApplier: Send + Sync {
    /// Insert rows into a user table
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `user_id` - The user who owns this data
    /// * `rows_data` - Serialized row data (bincode-encoded Vec<Row>)
    ///
    /// # Returns
    /// Number of rows inserted
    async fn insert(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        rows_data: &[u8],
    ) -> Result<usize, RaftError>;

    /// Update rows in a user table
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `user_id` - The user who owns this data
    /// * `updates_data` - Serialized update data
    /// * `filter_data` - Optional serialized filter
    ///
    /// # Returns
    /// Number of rows updated
    async fn update(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        updates_data: &[u8],
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError>;

    /// Delete rows from a user table
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `user_id` - The user who owns this data
    /// * `filter_data` - Optional serialized filter
    ///
    /// # Returns
    /// Number of rows deleted
    async fn delete(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError>;
}

/// Applier callback for shared table data operations
///
/// This trait is called by SharedDataStateMachine after Raft consensus commits
/// a data command. All nodes (leader and followers) call this.
#[async_trait]
pub trait SharedDataApplier: Send + Sync {
    /// Insert rows into a shared table
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `rows_data` - Serialized row data (bincode-encoded Vec<Row>)
    ///
    /// # Returns
    /// Number of rows inserted
    async fn insert(
        &self,
        table_id: &TableId,
        rows_data: &[u8],
    ) -> Result<usize, RaftError>;

    /// Update rows in a shared table
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `updates_data` - Serialized update data
    /// * `filter_data` - Optional serialized filter
    ///
    /// # Returns
    /// Number of rows updated
    async fn update(
        &self,
        table_id: &TableId,
        updates_data: &[u8],
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError>;

    /// Delete rows from a shared table
    ///
    /// # Arguments
    /// * `table_id` - The table identifier
    /// * `filter_data` - Optional serialized filter
    ///
    /// # Returns
    /// Number of rows deleted
    async fn delete(
        &self,
        table_id: &TableId,
        filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError>;
}

/// No-op applier for testing or standalone scenarios
pub struct NoOpUserDataApplier;

#[async_trait]
impl UserDataApplier for NoOpUserDataApplier {
    async fn insert(
        &self,
        _table_id: &TableId,
        _user_id: &UserId,
        _rows_data: &[u8],
    ) -> Result<usize, RaftError> {
        Ok(0)
    }

    async fn update(
        &self,
        _table_id: &TableId,
        _user_id: &UserId,
        _updates_data: &[u8],
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        Ok(0)
    }

    async fn delete(
        &self,
        _table_id: &TableId,
        _user_id: &UserId,
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        Ok(0)
    }
}

/// No-op applier for testing or standalone scenarios
pub struct NoOpSharedDataApplier;

#[async_trait]
impl SharedDataApplier for NoOpSharedDataApplier {
    async fn insert(
        &self,
        _table_id: &TableId,
        _rows_data: &[u8],
    ) -> Result<usize, RaftError> {
        Ok(0)
    }

    async fn update(
        &self,
        _table_id: &TableId,
        _updates_data: &[u8],
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        Ok(0)
    }

    async fn delete(
        &self,
        _table_id: &TableId,
        _filter_data: Option<&[u8]>,
    ) -> Result<usize, RaftError> {
        Ok(0)
    }
}
