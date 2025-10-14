// Message storage trait
use crate::error::StorageError;
use crate::models::Message;
use crate::storage::QueryParams;

/// Trait for message storage operations
pub trait MessageStore {
    /// Insert a message into storage
    fn insert_message(&self, message: &Message) -> Result<(), StorageError>;

    /// Get a message by its ID
    fn get_message(&self, msg_id: i64) -> Result<Option<Message>, StorageError>;

    /// Check if a message exists
    fn message_exists(&self, msg_id: i64) -> Result<bool, StorageError>;

    /// Delete a message by its ID
    fn delete_message(&self, msg_id: i64) -> Result<(), StorageError>;

    /// Get the total number of messages (approximate)
    fn message_count(&self) -> Result<u64, StorageError>;

    /// Query messages with filtering and pagination
    fn query_messages(&self, params: &QueryParams) -> Result<Vec<Message>, StorageError>;
}
